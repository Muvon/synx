//! Code shared between client and agent: filesystem mutations from
//! protocol messages, chunked transfer helpers, and the bidirectional
//! live-mode event loop.

use anyhow::{Context, Result};
use humansize::{format_size, BINARY};
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::Mutex;

use crate::ignores::IgnoreStack;
use crate::protocol::{
    read_message, write_message, Entry, EntryKind, Message, SyncMode, CHUNK_SIZE, CHUNK_THRESHOLD,
};
use crate::walker::build_entry;
use crate::watcher::{self, FsEvent};

/// Suppression entries are pruned after this long. We use mtime comparison
/// to decide if an event is an echo, so the TTL only bounds memory growth —
/// it does NOT block legitimate user edits.
const SUPPRESS_TTL: Duration = Duration::from_secs(60);

/// Read the mtime of a path as nanoseconds since the Unix epoch, or 0 if the
/// path doesn't exist or can't be stat'd. Does not follow symlinks.
fn lstat_mtime_ns(p: &Path) -> i64 {
    match fs::symlink_metadata(p) {
        Ok(m) => m
            .mtime()
            .saturating_mul(1_000_000_000)
            .saturating_add(m.mtime_nsec() as i64),
        Err(_) => 0,
    }
}

/// True if the local filesystem already has exactly what `entry` describes.
/// Lets us short-circuit echoes coming back from the peer.
///
/// Comparison is layered:
///   1. Size mismatch → not equal (cheap reject).
///   2. mtime match → assume equal (cheap stat-only fast path; matches
///      git's heuristic).
///   3. mtime drift → hash the file and compare to `entry.hash`. Robust
///      against filesystem-level rounding of `set_file_mtime` writes.
fn is_already_equal(root: &Path, entry: &Entry) -> bool {
    let full = root.join(&entry.path);
    let Ok(meta) = fs::symlink_metadata(&full) else {
        return false;
    };
    let ft = meta.file_type();
    match entry.kind {
        EntryKind::File => {
            if ft.is_symlink() || !ft.is_file() {
                return false;
            }
            if meta.len() != entry.size {
                return false;
            }
            let mt = meta
                .mtime()
                .saturating_mul(1_000_000_000)
                .saturating_add(meta.mtime_nsec() as i64);
            if mt == entry.mtime {
                return true;
            }
            // mtime drifted but size matches — fall back to a hash compare.
            // Cheap on small files; correct on anything where we set mtime
            // but the FS rounded it. Skip the zero hash (means peer didn't
            // compute one, so we can't be sure either way → treat as differ).
            if entry.hash == [0u8; 32] {
                return false;
            }
            match crate::walker::hash_file(&full) {
                Ok(h) => h == entry.hash,
                Err(_) => false,
            }
        }
        EntryKind::Dir => ft.is_dir() && !ft.is_symlink(),
        EntryKind::Symlink => {
            ft.is_symlink() && fs::read_link(&full).ok().as_ref() == entry.link_target.as_ref()
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Apply: deterministic, atomic filesystem mutations.
// ─────────────────────────────────────────────────────────────

pub fn apply_file_data(root: &Path, entry: &Entry, content: &[u8]) -> Result<()> {
    let full = root.join(&entry.path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create parent {}", parent.display()))?;
    }
    let tmp = tmp_path();
    fs::write(&tmp, content).with_context(|| format!("write tmp {}", tmp.display()))?;
    finalize_path(&tmp, &full, entry.mode, entry.mtime)?;
    Ok(())
}

pub fn apply_mkdir(root: &Path, entry: &Entry) -> Result<()> {
    let full = root.join(&entry.path);
    fs::create_dir_all(&full).with_context(|| format!("mkdir {}", full.display()))?;
    let _ = fs::set_permissions(&full, fs::Permissions::from_mode(entry.mode | 0o700));
    Ok(())
}

pub fn apply_symlink(root: &Path, entry: &Entry) -> Result<()> {
    let full = root.join(&entry.path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::symlink_metadata(&full).is_ok() {
        if fs::remove_file(&full).is_err() {
            let _ = fs::remove_dir_all(&full);
        }
    }
    let target = entry
        .link_target
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("symlink without target"))?;
    std::os::unix::fs::symlink(target, &full)
        .with_context(|| format!("symlink {}", full.display()))?;
    Ok(())
}

pub fn apply_delete(root: &Path, rel: &Path) -> Result<()> {
    let full = root.join(rel);
    let meta = match fs::symlink_metadata(&full) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if meta.is_dir() && !meta.file_type().is_symlink() {
        fs::remove_dir_all(&full)?;
    } else {
        fs::remove_file(&full)?;
    }
    Ok(())
}

pub fn apply_rename(root: &Path, from: &Path, to: &Path) -> Result<()> {
    let from_full = root.join(from);
    let to_full = root.join(to);
    if let Some(parent) = to_full.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&from_full, &to_full)
        .with_context(|| format!("rename {} → {}", from_full.display(), to_full.display()))?;
    Ok(())
}

/// Where our tmp files live. `~/.synx/tmp/`. Same filesystem as the user's
/// home directory, which on any normal Unix install is the same filesystem
/// as their work dirs — so `rename(2)` to the sync target is atomic.
fn tmp_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".synx")
        .join("tmp")
}

/// Allocate a fresh tmp path in the system tmp dir.
/// Pid + nanos give uniqueness across concurrent synx processes and runs.
fn tmp_path() -> PathBuf {
    use std::time::SystemTime;
    let dir = tmp_dir();
    let _ = fs::create_dir_all(&dir);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    dir.join(format!("{}.{}", std::process::id(), nanos))
}

/// Remove tmp files left over from a previous crashed run.
/// Age-based (> 1 hour) so we don't step on a concurrently-running synx.
/// Cheap; safe to call at startup of both client and agent.
pub fn cleanup_orphan_tmps() {
    use std::time::{Duration, SystemTime};
    let dir = tmp_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    let cutoff = SystemTime::now() - Duration::from_secs(3600);
    for entry in entries.flatten() {
        if let Ok(meta) = entry.metadata() {
            if meta.modified().map(|m| m < cutoff).unwrap_or(false) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Delta sync helpers (fast_rsync, SIMD-accelerated librsync).
//
// fast_rsync internally uses MD4 for block hashes (its origin is the rsync
// wire format). We MUST verify any delta-apply result with our own crypto
// hash (blake3) before accepting it.
// ─────────────────────────────────────────────────────────────

/// Block size for delta signatures. 4 KiB is the librsync default and works
/// well across a wide range of file sizes / change patterns.
const RSYNC_BLOCK_SIZE: u32 = 4096;
/// Number of bytes of the strong (MD4) hash kept per block in the signature.
/// 8 bytes is enough to avoid block collisions in practice; we verify the
/// final assembled result with blake3 anyway.
const RSYNC_STRONG_LEN: u32 = 8;

pub fn compute_signature(content: &[u8]) -> Vec<u8> {
    let sig = fast_rsync::Signature::calculate(
        content,
        fast_rsync::SignatureOptions {
            block_size: RSYNC_BLOCK_SIZE,
            crypto_hash_size: RSYNC_STRONG_LEN,
        },
    );
    sig.serialized().to_vec()
}

pub fn compute_delta(sig_bytes: &[u8], new_content: &[u8]) -> Result<Vec<u8>> {
    let sig = fast_rsync::Signature::deserialize(sig_bytes.to_vec())
        .map_err(|e| anyhow::anyhow!("signature parse: {e:?}"))?;
    let indexed = sig.index();
    let mut delta = Vec::new();
    fast_rsync::diff(&indexed, new_content, &mut delta)
        .map_err(|e| anyhow::anyhow!("delta diff: {e:?}"))?;
    Ok(delta)
}

pub fn apply_delta_mem(base: &[u8], delta: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(base.len());
    fast_rsync::apply(base, delta, &mut out).map_err(|e| anyhow::anyhow!("delta apply: {e:?}"))?;
    Ok(out)
}

/// Apply a delta to whatever's currently at `entry.path`, verify the result's
/// blake3 hash, and atomically replace the file.
pub fn apply_delta_to_file(
    root: &Path,
    entry: &Entry,
    base_hash: [u8; 32],
    delta: &[u8],
) -> Result<()> {
    let full = root.join(&entry.path);
    let base = fs::read(&full).with_context(|| format!("read base {}", full.display()))?;
    // Verify the base matches what the sender computed against. If not,
    // someone changed the file mid-flight; refuse rather than corrupt.
    let our_base_hash = blake3::hash(&base);
    if our_base_hash.as_bytes() != &base_hash {
        anyhow::bail!(
            "delta base hash mismatch for {} (file changed under us)",
            entry.path.display()
        );
    }
    let new_content = apply_delta_mem(&base, delta)?;
    // Verify the assembled result against the sender's authoritative blake3.
    // fast_rsync only uses MD4 internally, so this is the only honest check.
    let result_hash = blake3::hash(&new_content);
    if result_hash.as_bytes() != &entry.hash {
        anyhow::bail!("delta result hash mismatch for {}", entry.path.display());
    }
    apply_file_data(root, entry, &new_content)?;
    Ok(())
}

/// Move `tmp` into place at `final_path` and stamp mode + mtime.
///
/// Uses `rename(2)` only — atomic. Tmp lives under `~/.synx/tmp/`, target
/// lives under the user's sync root; on a normal install both are on the
/// home filesystem so rename succeeds. If they aren't (target on a
/// different mount), rename fails with EXDEV and the error propagates —
/// loud failure rather than a silent non-atomic fallback.
fn finalize_path(tmp: &Path, final_path: &Path, mode: u32, mtime: i64) -> Result<()> {
    let _ = fs::set_permissions(tmp, fs::Permissions::from_mode(mode));
    fs::rename(tmp, final_path).with_context(|| {
        format!(
            "rename {} → {} (if this is EXDEV, the sync target is on a different filesystem than $HOME — \
             set TMPDIR to a path on the same fs as your target)",
            tmp.display(),
            final_path.display()
        )
    })?;
    let ft = filetime::FileTime::from_unix_time(
        mtime.div_euclid(1_000_000_000),
        mtime.rem_euclid(1_000_000_000) as u32,
    );
    let _ = filetime::set_file_mtime(final_path, ft);
    Ok(())
}

// ─────────────────────────────────────────────────────────────
// Chunked write state — for files larger than CHUNK_THRESHOLD,
// we receive them in 4 MiB chunks into a tmp file, then rename.
// ─────────────────────────────────────────────────────────────

struct InFlight {
    entry: Entry,
    file: fs::File,
    tmp: PathBuf,
    bytes_written: u64,
    /// Stream-hash chunks as they arrive. Compared against `entry.hash` at
    /// `end()` time so we can refuse to publish a tmp whose bytes don't
    /// match what the sender claimed (corruption, dropped chunks, etc.).
    hasher: blake3::Hasher,
}

#[derive(Default, Clone)]
pub struct Pending {
    inner: Arc<Mutex<HashMap<PathBuf, InFlight>>>,
}

impl Pending {
    pub async fn start(&self, root: &Path, entry: Entry) -> Result<()> {
        let full = root.join(&entry.path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = tmp_path();
        let file = fs::File::create(&tmp).with_context(|| format!("open tmp {}", tmp.display()))?;
        let path = entry.path.clone();
        self.inner.lock().await.insert(
            path,
            InFlight {
                entry,
                file,
                tmp,
                bytes_written: 0,
                hasher: blake3::Hasher::new(),
            },
        );
        Ok(())
    }

    pub async fn chunk(&self, path: &Path, data: &[u8]) -> Result<()> {
        let mut g = self.inner.lock().await;
        if let Some(s) = g.get_mut(path) {
            s.file
                .write_all(data)
                .with_context(|| format!("write chunk {}", path.display()))?;
            s.hasher.update(data);
            s.bytes_written += data.len() as u64;
        }
        Ok(())
    }

    /// Finalize: verify the assembled hash matches what the sender said.
    /// On mismatch, delete the tmp and bail — the real target is untouched
    /// and the next session's manifest diff will re-attempt the transfer.
    pub async fn end(&self, root: &Path, path: &Path) -> Result<Option<Entry>> {
        let Some(s) = self.inner.lock().await.remove(path) else {
            return Ok(None);
        };
        let InFlight {
            entry,
            file,
            tmp,
            bytes_written,
            hasher,
        } = s;
        file.sync_all().ok();
        drop(file);

        let actual = *hasher.finalize().as_bytes();
        if actual != entry.hash {
            // Drop the bad tmp; do NOT replace the target. Loud error so
            // the session tears down — the reconnect loop will then redo
            // the transfer cleanly on the next attempt.
            let _ = fs::remove_file(&tmp);
            anyhow::bail!(
                "chunked transfer hash mismatch for {}: {} bytes received, hash {} vs expected {}",
                entry.path.display(),
                bytes_written,
                blake3::Hash::from(actual).to_hex(),
                blake3::Hash::from(entry.hash).to_hex()
            );
        }

        let full = root.join(&entry.path);
        finalize_path(&tmp, &full, entry.mode, entry.mtime)?;
        Ok(Some(entry))
    }
}

// ─────────────────────────────────────────────────────────────
// Chunked sender: send either FileData (small) or FileStart +
// FileChunk* + FileEnd (large). Streaming read from disk → wire.
// ─────────────────────────────────────────────────────────────

pub async fn send_file<W>(
    writer: &Arc<Mutex<W>>,
    root: &Path,
    entry: &Entry,
    compress: bool,
) -> Result<u64>
where
    W: AsyncWriteExt + Unpin,
{
    let full = root.join(&entry.path);
    let size = entry.size as usize;
    if size < CHUNK_THRESHOLD {
        // File may have vanished between manifest stat and now (user `rm`'d).
        // Return 0 so the caller knows there's nothing to send; subsequent
        // Removed event will propagate the delete.
        let content = match fs::read(&full) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e).with_context(|| format!("read {}", full.display())),
        };
        let sent = content.len() as u64;
        let mut w = writer.lock().await;
        write_message(
            &mut *w,
            &Message::FileData {
                entry: entry.clone(),
                content,
            },
            compress,
        )
        .await?;
        Ok(sent)
    } else {
        let mut file = match fs::File::open(&full) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e).with_context(|| format!("open {}", full.display())),
        };
        {
            let mut w = writer.lock().await;
            write_message(
                &mut *w,
                &Message::FileStart {
                    entry: entry.clone(),
                    total_size: entry.size,
                },
                compress,
            )
            .await?;
        }
        let mut buf = vec![0u8; CHUNK_SIZE];
        let mut total: u64 = 0;
        loop {
            // On Unix the fd stays valid even after unlink, so reads keep
            // working. A read error here is something real (disk failure).
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            total += n as u64;
            let mut w = writer.lock().await;
            write_message(
                &mut *w,
                &Message::FileChunk {
                    path: entry.path.clone(),
                    data: buf[..n].to_vec(),
                },
                compress,
            )
            .await?;
        }
        let mut w = writer.lock().await;
        write_message(
            &mut *w,
            &Message::FileEnd {
                path: entry.path.clone(),
            },
            compress,
        )
        .await?;
        Ok(total)
    }
}

// ─────────────────────────────────────────────────────────────
// Loop suppression — when we apply an incoming change, our own
// watcher will see it; we silence that one specific echo using
// the *current state* of the path (mtime / existence), not just
// path+TTL. This avoids blocking real user edits that happen to
// occur shortly after an apply.
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum ApplyState {
    /// We last saw the path in this state. `hash` is the file's content hash
    /// for regular files; `[0u8; 32]` for dirs / symlinks (we don't track
    /// their "content"). Used for echo suppression (mtime match) AND
    /// sender-side dedup (hash match → send `Touch` instead of full file).
    Set { mtime: i64, hash: [u8; 32] },
    /// We just deleted the path and expect it to not exist.
    Deleted,
}

const NO_HASH: [u8; 32] = [0u8; 32];

/// Synchronous suppression map — uses `std::sync::Mutex` so the watcher's
/// notify thread can update it eagerly (before debouncing) and so all
/// methods are callable from both sync and async contexts without holding
/// an async lock across awaits.
#[derive(Default, Clone)]
pub struct Suppression {
    inner: Arc<std::sync::Mutex<HashMap<PathBuf, (ApplyState, Instant)>>>,
}

impl Suppression {
    /// Record that the path now exists with `mtime` and (optionally) `hash`.
    /// Use `NO_HASH` for dirs / symlinks.
    pub fn mark_set(&self, path: PathBuf, mtime_ns: i64, hash: [u8; 32]) {
        if let Ok(mut g) = self.inner.lock() {
            g.insert(
                path,
                (
                    ApplyState::Set {
                        mtime: mtime_ns,
                        hash,
                    },
                    Instant::now(),
                ),
            );
        }
    }

    /// Convenience: mark without a content hash (dirs, symlinks, or unknown).
    pub fn mark_mtime(&self, path: PathBuf, mtime_ns: i64) {
        self.mark_set(path, mtime_ns, NO_HASH);
    }

    pub fn mark_deleted(&self, path: PathBuf) {
        if let Ok(mut g) = self.inner.lock() {
            g.insert(path, (ApplyState::Deleted, Instant::now()));
        }
    }

    /// True if we recently deleted (or sent a delete for) this path.
    pub fn is_recently_deleted(&self, path: &Path) -> bool {
        let Ok(g) = self.inner.lock() else {
            return false;
        };
        matches!(g.get(path), Some((ApplyState::Deleted, _)))
    }

    /// Return the content hash we have on record for this file, if any.
    /// Used by the sender to skip retransmitting unchanged content.
    pub fn prior_hash(&self, path: &Path) -> Option<[u8; 32]> {
        let g = self.inner.lock().ok()?;
        match g.get(path) {
            Some((ApplyState::Set { hash, .. }, _)) if *hash != NO_HASH => Some(*hash),
            _ => None,
        }
    }

    /// True if this event is the echo of our own previous apply — the path's
    /// current state still matches what we recorded.
    pub fn is_echo(&self, root: &Path, ev: &FsEvent) -> bool {
        let Ok(mut g) = self.inner.lock() else {
            return false;
        };
        let now = Instant::now();
        g.retain(|_, (_, t)| now.duration_since(*t) < SUPPRESS_TTL);

        let key: &Path = match ev {
            FsEvent::Created(p) | FsEvent::Modified(p) | FsEvent::Removed(p) => p,
            FsEvent::Renamed { to, .. } => to,
        };
        let Some((state, _)) = g.get(key) else {
            return false;
        };
        match (state, ev) {
            (
                ApplyState::Set {
                    mtime: expected, ..
                },
                FsEvent::Created(_),
            )
            | (
                ApplyState::Set {
                    mtime: expected, ..
                },
                FsEvent::Modified(_),
            )
            | (
                ApplyState::Set {
                    mtime: expected, ..
                },
                FsEvent::Renamed { .. },
            ) => {
                let cur = lstat_mtime_ns(&root.join(key));
                cur != 0 && cur == *expected
            }
            (ApplyState::Deleted, FsEvent::Removed(_)) => !root.join(key).exists(),
            _ => false,
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Live mode: a generic bidirectional loop driven by tokio::select.
// ─────────────────────────────────────────────────────────────

fn directions(mode: SyncMode, is_client: bool) -> (bool, bool) {
    match (mode, is_client) {
        (SyncMode::Both, _) => (true, true),
        (SyncMode::Push, true) => (true, false),
        (SyncMode::Push, false) => (false, true),
        (SyncMode::Pull, true) => (false, true),
        (SyncMode::Pull, false) => (true, false),
    }
}

pub async fn live_loop<R, W>(
    root: PathBuf,
    mut reader: R,
    writer: Arc<Mutex<W>>,
    mode: SyncMode,
    compress: bool,
    is_client: bool,
    ignores: Arc<IgnoreStack>,
) -> Result<()>
where
    R: AsyncRead + AsyncReadExt + Unpin + Send + 'static,
    W: AsyncWrite + AsyncWriteExt + Unpin + Send,
{
    let (send_local, apply_remote) = directions(mode, is_client);

    // Dedicated reader task → channel. read_exact is not cancel-safe in select!.
    let (msg_tx, mut msg_rx) =
        tokio::sync::mpsc::unbounded_channel::<Result<Message, anyhow::Error>>();
    let reader_task = tokio::spawn(async move {
        loop {
            match read_message(&mut reader).await {
                Ok(m) => {
                    if msg_tx.send(Ok(m)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = msg_tx.send(Err(e.into()));
                    break;
                }
            }
        }
    });

    let suppress = Suppression::default();
    let pending = Pending::default();

    // The watcher must share our suppression map so it can mark Deleted
    // eagerly (in its notify-thread callback) before debouncing. Otherwise a
    // peer's stale `FileData` arriving after the user's `rm` but before our
    // debouncer fires would resurrect the file.
    let watcher::WatcherHandle {
        events: mut event_rx,
        keepalive: _watcher,
    } = watcher::spawn(root.clone(), suppress.clone())?;

    let sigint = tokio::signal::ctrl_c();
    tokio::pin!(sigint);

    loop {
        tokio::select! {
            biased;

            _ = &mut sigint => {
                tracing::info!("ctrl+c — closing");
                let mut w = writer.lock().await;
                let _ = write_message(&mut *w, &Message::Bye, compress).await;
                break;
            }

            msg = msg_rx.recv() => {
                match msg {
                    Some(Ok(Message::Bye)) => break,
                    Some(Ok(m)) => {
                        handle_incoming(&root, m, &suppress, &pending, compress, &writer, apply_remote, Some(&ignores), is_client).await?;
                    }
                    Some(Err(e)) => {
                        tracing::debug!("peer closed: {e}");
                        break;
                    }
                    None => break,
                }
            }

            ev = event_rx.recv() => {
                let Some(events) = ev else { break };
                if send_local {
                    forward_local_events(&root, events, &writer, compress, &suppress, is_client).await?;
                }
            }
        }
    }

    reader_task.abort();
    Ok(())
}

/// True if `ignores` rejects this path. `None` means "no filter".
fn ignored(ignores: Option<&IgnoreStack>, rel: &Path, is_dir: bool) -> bool {
    match ignores {
        Some(s) => s.is_ignored_rel(rel, is_dir),
        None => false,
    }
}

pub async fn handle_incoming<W>(
    root: &Path,
    msg: Message,
    suppress: &Suppression,
    pending: &Pending,
    compress: bool,
    writer: &Arc<Mutex<W>>,
    apply_remote: bool,
    ignores: Option<&IgnoreStack>,
    is_client: bool,
) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    // Only the client prints user-facing event lines. The agent's stderr is
    // forwarded over SSH to the same terminal, so any logs there would just
    // duplicate the client's transcript.
    let log_event = is_client;
    match msg {
        Message::FileData { entry, content } => {
            if !apply_remote {
                return Ok(());
            }
            if ignored(ignores, &entry.path, false) {
                tracing::debug!("ignored (recv FileData): {}", entry.path.display());
                return Ok(());
            }
            // Receiver dedup: if our disk already has this exact content,
            // skip the write entirely (and the noisy log line).
            if is_already_equal(root, &entry) {
                let mt = lstat_mtime_ns(&root.join(&entry.path));
                suppress.mark_set(entry.path.clone(), mt, entry.hash);
                tracing::trace!("dedup (recv FileData): {}", entry.path.display());
                return Ok(());
            }
            // Stale-create guard: peer is sending us a file we just deleted.
            // Their FileData was already on the wire when our Delete arrived,
            // so drop it instead of resurrecting the file the user removed.
            let full = root.join(&entry.path);
            if !full.exists() && suppress.is_recently_deleted(&entry.path) {
                tracing::debug!(
                    "dropping stale FileData after delete: {}",
                    entry.path.display()
                );
                return Ok(());
            }
            let size = content.len();
            let hash = entry.hash;
            apply_file_data(root, &entry, &content)?;
            // Use the *actual* on-disk mtime so our own watcher's echo of
            // this write matches exactly (set_file_mtime may be FS-rounded).
            // Store hash too, so future sender checks can dedup via Touch.
            let mt = lstat_mtime_ns(&full);
            suppress.mark_set(entry.path.clone(), mt, hash);
            if log_event {
                eprintln!(
                    "  {} {}  {}",
                    "←".bright_cyan(),
                    entry.path.display(),
                    format_size(size, BINARY).dimmed()
                );
            }
        }
        Message::FileStart { entry, .. } => {
            if !apply_remote {
                return Ok(());
            }
            if ignored(ignores, &entry.path, false) {
                tracing::debug!("ignored (recv FileStart): {}", entry.path.display());
                return Ok(());
            }
            // Same receiver dedup at the chunked path. If we already have it,
            // don't open a tmp file — subsequent chunks for this path become
            // no-ops (Pending::chunk silently drops chunks for unknown paths).
            if is_already_equal(root, &entry) {
                let mt = lstat_mtime_ns(&root.join(&entry.path));
                suppress.mark_set(entry.path.clone(), mt, entry.hash);
                tracing::trace!("dedup (recv FileStart): {}", entry.path.display());
                return Ok(());
            }
            // Stale-create guard (chunked transfer variant).
            let full = root.join(&entry.path);
            if !full.exists() && suppress.is_recently_deleted(&entry.path) {
                tracing::debug!(
                    "dropping stale FileStart after delete: {}",
                    entry.path.display()
                );
                return Ok(());
            }
            pending.start(root, entry).await?;
        }
        Message::FileChunk { path, data } => {
            if !apply_remote {
                return Ok(());
            }
            if ignored(ignores, &path, false) {
                return Ok(());
            }
            pending.chunk(&path, &data).await?;
        }
        Message::FileEnd { path } => {
            if !apply_remote {
                return Ok(());
            }
            if ignored(ignores, &path, false) {
                return Ok(());
            }
            if let Some(entry) = pending.end(root, &path).await? {
                let mt = lstat_mtime_ns(&root.join(&entry.path));
                suppress.mark_set(entry.path.clone(), mt, entry.hash);
                if log_event {
                    eprintln!(
                        "  {} {}  {}",
                        "←".bright_cyan(),
                        entry.path.display(),
                        format_size(entry.size, BINARY).dimmed()
                    );
                }
            }
        }
        Message::Touch { path, mtime, mode } => {
            if !apply_remote {
                return Ok(());
            }
            if ignored(ignores, &path, false) {
                return Ok(());
            }
            let full = root.join(&path);
            let Ok(_meta) = fs::symlink_metadata(&full) else {
                // No file to touch (we may have deleted it, or never had it).
                // Drop quietly; if peer actually needs us to create it they'll
                // re-send a full FileData.
                tracing::debug!("touch for missing path: {}", path.display());
                return Ok(());
            };
            let _ = fs::set_permissions(&full, fs::Permissions::from_mode(mode));
            let ft = filetime::FileTime::from_unix_time(
                mtime.div_euclid(1_000_000_000),
                mtime.rem_euclid(1_000_000_000) as u32,
            );
            let _ = filetime::set_file_mtime(&full, ft);
            // Mark using the actual on-disk mtime so our own watcher's echo
            // of this metadata write matches exactly. Preserve any hash we
            // had on record (content didn't change).
            let prior = suppress.prior_hash(&path).unwrap_or(NO_HASH);
            let new_mtime = lstat_mtime_ns(&full);
            suppress.mark_set(path.clone(), new_mtime, prior);
            if log_event {
                eprintln!(
                    "  {} {}  {}",
                    "←".bright_cyan(),
                    path.display(),
                    "(touch)".dimmed()
                );
            }
        }
        Message::MkDir { entry } => {
            if !apply_remote {
                return Ok(());
            }
            if ignored(ignores, &entry.path, true) {
                tracing::debug!("ignored (recv MkDir): {}", entry.path.display());
                return Ok(());
            }
            if is_already_equal(root, &entry) {
                let mt = lstat_mtime_ns(&root.join(&entry.path));
                suppress.mark_mtime(entry.path.clone(), mt);
                return Ok(());
            }
            let full = root.join(&entry.path);
            if !full.exists() && suppress.is_recently_deleted(&entry.path) {
                tracing::debug!(
                    "dropping stale MkDir after delete: {}",
                    entry.path.display()
                );
                return Ok(());
            }
            apply_mkdir(root, &entry)?;
            // Use the actual on-disk mtime (dir mtime changes whenever
            // children are added) so future echoes match precisely.
            let mt = lstat_mtime_ns(&full);
            suppress.mark_mtime(entry.path, mt);
        }
        Message::MkSymlink { entry } => {
            if !apply_remote {
                return Ok(());
            }
            if ignored(ignores, &entry.path, false) {
                return Ok(());
            }
            if is_already_equal(root, &entry) {
                let mt = lstat_mtime_ns(&root.join(&entry.path));
                suppress.mark_mtime(entry.path.clone(), mt);
                return Ok(());
            }
            let full = root.join(&entry.path);
            if !full.exists() && suppress.is_recently_deleted(&entry.path) {
                tracing::debug!(
                    "dropping stale MkSymlink after delete: {}",
                    entry.path.display()
                );
                return Ok(());
            }
            apply_symlink(root, &entry)?;
            let mt = lstat_mtime_ns(&full);
            suppress.mark_mtime(entry.path, mt);
        }
        Message::Delete { path } => {
            if !apply_remote {
                return Ok(());
            }
            if ignored(ignores, &path, false) && ignored(ignores, &path, true) {
                return Ok(());
            }
            let existed_before = fs::symlink_metadata(root.join(&path)).is_ok();
            apply_delete(root, &path)?;
            suppress.mark_deleted(path.clone());
            if existed_before && log_event {
                eprintln!(
                    "  {} {}",
                    "←".bright_cyan(),
                    format!("× {}", path.display())
                );
            }
        }
        Message::Rename { from, to } => {
            if !apply_remote {
                return Ok(());
            }
            if ignored(ignores, &from, false) || ignored(ignores, &to, false) {
                tracing::debug!(
                    "ignored (recv Rename): {} → {}",
                    from.display(),
                    to.display()
                );
                return Ok(());
            }
            // Stale-rename guard: if the source is gone because we just
            // deleted it, a Rename(from, to) is meaningless — drop it.
            let from_full = root.join(&from);
            if !from_full.exists() && suppress.is_recently_deleted(&from) {
                tracing::debug!(
                    "dropping stale Rename after delete: {} → {}",
                    from.display(),
                    to.display()
                );
                return Ok(());
            }
            apply_rename(root, &from, &to)?;
            suppress.mark_deleted(from);
            let mt = lstat_mtime_ns(&root.join(&to));
            suppress.mark_mtime(to, mt);
        }
        Message::FileGet { path } => {
            if ignored(ignores, &path, false) && ignored(ignores, &path, true) {
                return Ok(());
            }
            if let Some(entry) = build_entry(root, &path, None)? {
                match entry.kind {
                    EntryKind::File => {
                        let _ = send_file(writer, root, &entry, compress).await?;
                    }
                    EntryKind::Dir => {
                        let mut w = writer.lock().await;
                        write_message(&mut *w, &Message::MkDir { entry }, compress).await?;
                    }
                    EntryKind::Symlink => {
                        let mut w = writer.lock().await;
                        write_message(&mut *w, &Message::MkSymlink { entry }, compress).await?;
                    }
                }
            }
        }
        Message::Ping => {
            let mut w = writer.lock().await;
            let _ = write_message(&mut *w, &Message::Pong, compress).await;
        }
        Message::Pong => {}
        Message::Error(e) => anyhow::bail!("peer error: {e}"),
        other => {
            tracing::debug!(
                "ignoring message in live: {:?}",
                std::mem::discriminant(&other)
            );
        }
    }
    Ok(())
}

/// Collapse a batch of watcher events to at most one per path.
///
/// Per-path policy:
///   - First event is `Created` AND last event is `Removed` → **drop the
///     whole path**. The file lived and died inside the debouncer window;
///     it's an ephemeral artifact (Vim's `4913` probe, atomic-write tmps,
///     IDE scratch files). Sending a Delete for something the peer never
///     saw is noise.
///   - Otherwise → keep the last event (most recent state wins).
///
/// Renames are keyed on their destination path.
fn coalesce(events: Vec<FsEvent>) -> Vec<FsEvent> {
    use std::collections::HashSet;

    let key_of = |ev: &FsEvent| -> PathBuf {
        match ev {
            FsEvent::Created(p) | FsEvent::Modified(p) | FsEvent::Removed(p) => p.clone(),
            FsEvent::Renamed { to, .. } => to.clone(),
        }
    };

    // For each path, remember the first and last event indices in this batch.
    let mut first_last: HashMap<PathBuf, (usize, usize)> = HashMap::new();
    for (i, ev) in events.iter().enumerate() {
        let key = key_of(ev);
        first_last
            .entry(key)
            .and_modify(|(_, last)| *last = i)
            .or_insert((i, i));
    }

    let mut keep: HashSet<usize> = HashSet::with_capacity(first_last.len());
    for &(first, last) in first_last.values() {
        if matches!(events[first], FsEvent::Created(_))
            && matches!(events[last], FsEvent::Removed(_))
        {
            // Ephemeral: created and gone before we even fired. Skip entirely.
            continue;
        }
        keep.insert(last);
    }

    events
        .into_iter()
        .enumerate()
        .filter(|(i, _)| keep.contains(i))
        .map(|(_, ev)| ev)
        .collect()
}

async fn forward_local_events<W>(
    root: &Path,
    events: Vec<FsEvent>,
    writer: &Arc<Mutex<W>>,
    compress: bool,
    suppress: &Suppression,
    is_client: bool,
) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    // Only the client prints. On the agent, the same eprintln would be
    // forwarded over SSH stderr and duplicate every transfer line.
    let log_event = is_client;
    let events = coalesce(events);
    for ev in events {
        if suppress.is_echo(root, &ev) {
            tracing::trace!("echo suppressed: {:?}", ev);
            continue;
        }

        match ev {
            FsEvent::Created(p) | FsEvent::Modified(p) => {
                let entry = match build_entry(root, &p, None)? {
                    Some(e) => e,
                    None => {
                        // The path doesn't exist anymore. This commonly
                        // happens when a Remove + Modify fire in the same
                        // debouncer batch and coalesce kept the Modify
                        // (FSEvents on macOS is chatty during `rm`). The
                        // user's intent is a delete — treat it as such.
                        if log_event {
                            eprintln!("  {} {}", "→".bright_green(), format!("× {}", p.display()));
                        }
                        {
                            let mut w = writer.lock().await;
                            write_message(&mut *w, &Message::Delete { path: p.clone() }, compress)
                                .await?;
                        }
                        suppress.mark_deleted(p);
                        continue;
                    }
                };
                let path_clone = entry.path.clone();
                let entry_mtime = entry.mtime;
                let entry_hash = entry.hash;
                let entry_kind = entry.kind;
                match entry.kind {
                    EntryKind::Dir => {
                        let mut w = writer.lock().await;
                        write_message(&mut *w, &Message::MkDir { entry }, compress).await?;
                    }
                    EntryKind::Symlink => {
                        let mut w = writer.lock().await;
                        write_message(&mut *w, &Message::MkSymlink { entry }, compress).await?;
                    }
                    EntryKind::File => {
                        // Content-unchanged optimization: if we already
                        // synced this exact content (matching hash on
                        // record), send a lightweight Touch — mtime + mode
                        // only — instead of re-transmitting the body.
                        if suppress.prior_hash(&entry.path) == Some(entry.hash) {
                            if log_event {
                                eprintln!(
                                    "  {} {}  {}",
                                    "→".bright_green(),
                                    entry.path.display(),
                                    "(touch)".dimmed()
                                );
                            }
                            let mut w = writer.lock().await;
                            write_message(
                                &mut *w,
                                &Message::Touch {
                                    path: entry.path.clone(),
                                    mtime: entry.mtime,
                                    mode: entry.mode,
                                },
                                compress,
                            )
                            .await?;
                        } else {
                            let size = entry.size;
                            let is_big = (size as usize) >= CHUNK_THRESHOLD;
                            // For big files, show a start marker so the user
                            // can see something's in flight; chunked transfer
                            // can take a while on slow links.
                            if log_event && is_big {
                                eprintln!(
                                    "  {} {}  {}  {}",
                                    "→".bright_green(),
                                    entry.path.display(),
                                    format_size(size, BINARY).dimmed(),
                                    "…".bright_yellow()
                                );
                            }
                            let sent = send_file(writer, root, &entry, compress).await?;
                            if log_event {
                                if sent == 0 && size > 0 {
                                    // File vanished between manifest stat
                                    // and send_file's open — treat as a
                                    // delete; the watcher's Removed event
                                    // (already queued) will dispatch it.
                                    eprintln!(
                                        "  {} {}  {}",
                                        "→".bright_green(),
                                        entry.path.display(),
                                        "(vanished — delete will follow)".dimmed()
                                    );
                                } else if is_big {
                                    eprintln!(
                                        "  {} {}  {}  {}",
                                        "→".bright_green(),
                                        entry.path.display(),
                                        format_size(sent, BINARY).dimmed(),
                                        "✓".bright_green()
                                    );
                                } else {
                                    eprintln!(
                                        "  {} {}  {}",
                                        "→".bright_green(),
                                        entry.path.display(),
                                        format_size(sent, BINARY).dimmed()
                                    );
                                }
                            }
                        }
                    }
                }
                // Mark our own outgoing state. Catches: (a) the peer echoes
                // our payload back, (b) if the user then deletes & we get a
                // stale Create back, drop it, (c) next watcher fire for this
                // same content → sender skip via prior_hash.
                let hash_to_mark = match entry_kind {
                    EntryKind::File => entry_hash,
                    _ => NO_HASH,
                };
                suppress.mark_set(path_clone, entry_mtime, hash_to_mark);
            }
            FsEvent::Removed(p) => {
                if log_event {
                    eprintln!("  {} {}", "→".bright_green(), format!("× {}", p.display()));
                }
                {
                    let mut w = writer.lock().await;
                    write_message(&mut *w, &Message::Delete { path: p.clone() }, compress).await?;
                }
                // Record that *we* deleted this — receiver dedup uses this
                // to drop stale FileData / MkDir for the same path arriving
                // out-of-order from the peer.
                suppress.mark_deleted(p);
            }
            FsEvent::Renamed { from, to } => {
                {
                    let mut w = writer.lock().await;
                    write_message(
                        &mut *w,
                        &Message::Rename {
                            from: from.clone(),
                            to: to.clone(),
                        },
                        compress,
                    )
                    .await?;
                }
                if let Some(entry) = build_entry(root, &to, None)? {
                    let to_mtime = entry.mtime;
                    match entry.kind {
                        EntryKind::File => {
                            send_file(writer, root, &entry, compress).await?;
                        }
                        EntryKind::Dir => {
                            let mut w = writer.lock().await;
                            write_message(&mut *w, &Message::MkDir { entry }, compress).await?;
                        }
                        EntryKind::Symlink => {
                            let mut w = writer.lock().await;
                            write_message(&mut *w, &Message::MkSymlink { entry }, compress).await?;
                        }
                    }
                    suppress.mark_mtime(to.clone(), to_mtime);
                }
                suppress.mark_deleted(from.clone());
                if log_event {
                    eprintln!(
                        "  {} {} → {}",
                        "→".bright_green(),
                        from.display(),
                        to.display()
                    );
                }
            }
        }
    }
    Ok(())
}
