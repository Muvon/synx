//! Code shared between client and agent: filesystem mutations from
//! protocol messages, chunked transfer helpers, and the bidirectional
//! live-mode event loop.

use anyhow::{Context, Result};
use humansize::{format_size, BINARY};
use owo_colors::OwoColorize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::Mutex;

use crate::baseline::LiveBaseline;
use crate::cache::HashCache;
use crate::ignores::IgnoreStack;
use crate::paths::{resolve_beneath, INTERNAL_TMP_PREFIX};
use crate::protocol::{
    read_message, write_message, Entry, EntryKind, Message, SyncMode, CHUNK_SIZE, CHUNK_THRESHOLD,
};
use crate::walker::{build_entry, walk_manifest};
use crate::watcher::{self, FsEvent};

/// Suppression entries are pruned after this long. We use mtime comparison
/// to decide if an event is an echo, so the TTL only bounds memory growth —
/// it does NOT block legitimate user edits.
const SUPPRESS_TTL: Duration = Duration::from_secs(60);

/// Prune the suppression map at most this often. The TTL only bounds memory,
/// so a few seconds of staleness is harmless; gating the sweep keeps the
/// per-event echo check O(1) instead of O(map) after a large initial sync
/// populates thousands of entries.
const SUPPRESS_SWEEP: Duration = Duration::from_secs(5);

/// Read the mtime of a path as nanoseconds since the Unix epoch, or 0 if the
/// path doesn't exist or can't be stat'd. Does not follow symlinks.
fn lstat_mtime_ns(p: &Path) -> i64 {
    match fs::symlink_metadata(p) {
        Ok(m) => m
            .mtime()
            .saturating_mul(1_000_000_000)
            .saturating_add(m.mtime_nsec()),
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
    let Ok(full) = resolve_beneath(root, &entry.path) else {
        return false;
    };
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
                .saturating_add(meta.mtime_nsec());
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
    if entry.kind != EntryKind::File {
        anyhow::bail!("FileData entry is not a file: {}", entry.path.display());
    }
    if content.len() as u64 != entry.size || blake3::hash(content).as_bytes() != &entry.hash {
        anyhow::bail!("FileData content mismatch for {}", entry.path.display());
    }
    write_file_atomic(root, entry, content)
}

/// Place already-verified `content` at `entry.path` via tmp + rename and
/// stamp mode + mtime.
fn write_file_atomic(root: &Path, entry: &Entry, content: &[u8]) -> Result<()> {
    let full = resolve_beneath(root, &entry.path)?;
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create parent {}", parent.display()))?;
    }
    let tmp = tmp_path(&full);
    fs::write(&tmp, content).with_context(|| format!("write tmp {}", tmp.display()))?;
    finalize_path(&tmp, &full, entry.mode, entry.mtime)?;
    Ok(())
}

pub fn apply_mkdir(root: &Path, entry: &Entry) -> Result<()> {
    if entry.kind != EntryKind::Dir {
        anyhow::bail!("MkDir entry is not a directory: {}", entry.path.display());
    }
    let full = resolve_beneath(root, &entry.path)?;
    if fs::symlink_metadata(&full).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        anyhow::bail!(
            "refusing to chmod symlink as directory: {}",
            entry.path.display()
        );
    }
    fs::create_dir_all(&full).with_context(|| format!("mkdir {}", full.display()))?;
    let _ = fs::set_permissions(&full, fs::Permissions::from_mode(entry.mode | 0o700));
    Ok(())
}

pub fn apply_symlink(root: &Path, entry: &Entry) -> Result<()> {
    if entry.kind != EntryKind::Symlink {
        anyhow::bail!("MkSymlink entry is not a symlink: {}", entry.path.display());
    }
    let full = resolve_beneath(root, &entry.path)?;
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Ok(metadata) = fs::symlink_metadata(&full) {
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            anyhow::bail!(
                "refusing to replace directory with symlink: {}",
                entry.path.display()
            );
        }
        fs::remove_file(&full).with_context(|| format!("remove existing {}", full.display()))?;
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
    let full = resolve_beneath(root, rel)?;
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
    let from_full = resolve_beneath(root, from)?;
    let to_full = resolve_beneath(root, to)?;
    if let Some(parent) = to_full.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&from_full, &to_full)
        .with_context(|| format!("rename {} → {}", from_full.display(), to_full.display()))?;
    Ok(())
}

/// Legacy temporary directory used before temporary files moved beside their
/// destination. Retained only so startup can clean leftovers from old runs.
fn tmp_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".synx")
        .join("tmp")
}

/// Allocate beside the destination so rename(2) remains atomic even when the
/// sync root lives on a different mount than the user's home directory.
fn tmp_path(final_path: &Path) -> PathBuf {
    use std::time::SystemTime;
    let dir = final_path.parent().unwrap_or_else(|| Path::new("."));
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    dir.join(format!(
        "{INTERNAL_TMP_PREFIX}{}-{nanos}",
        std::process::id()
    ))
}

/// True iff `path` (relative to sync root) is `.git` or lies under `.git/`.
/// Cheap path-component check — no filesystem access.
pub fn is_under_git(rel: &Path) -> bool {
    rel.components()
        .next()
        .map(|c| c.as_os_str() == ".git")
        .unwrap_or(false)
}

/// True iff the sync root has a git operation in progress that would race
/// with file-level sync of `.git/`. While any of these markers exist, we
/// pause syncing of paths under `.git/` on both walk, push, and apply.
///
/// Why: git treats `.git/` as transactional state. Atomically renaming a
/// ref while we mid-stream a different version of that ref from the peer
/// causes "cannot lock ref" failures and breaks rebase/merge/cherry-pick.
/// Pausing only `.git/` (not the working tree) is correct — your source
/// edits keep syncing, only the VCS metadata is held back until the
/// in-progress operation finishes.
///
/// Staleness handling: a marker file older than `STALE_AFTER` is treated
/// as garbage left over by a crashed git or a synx-induced sync (the bug
/// before this guard existed). Pretending it's "busy" forever would
/// deadlock recovery, so we deliberately ignore stale markers and let
/// `.git/` sync resume. Real in-progress operations refresh markers
/// far more often than this threshold — even an interactive rebase
/// touching `done` / `git-rebase-todo` on every pick stays well inside
/// the window.
pub fn git_busy(root: &Path) -> bool {
    use std::time::SystemTime;
    /// Markers younger than this are treated as a live operation.
    /// Older = stale, ignored. 10 min is well past any normal git step
    /// and short enough that crash/leftover state self-heals.
    const STALE_AFTER: Duration = Duration::from_secs(600);

    let git_dir = root.join(".git");
    // `.git` may not exist, may be a worktree pointer file, or a real dir.
    // We only handle the regular-dir case; worktrees are uncommon enough
    // that paying for them isn't worth the complexity now.
    if !git_dir.is_dir() {
        return false;
    }
    const MARKERS: &[&str] = &[
        "rebase-merge",
        "rebase-apply",
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "BISECT_LOG",
        "index.lock",
        "HEAD.lock",
    ];
    let now = SystemTime::now();
    for m in MARKERS {
        let p = git_dir.join(m);
        let Ok(meta) = fs::metadata(&p) else {
            continue;
        };
        // For directories (rebase-merge, rebase-apply), use the newest
        // mtime among contents — git rewrites files inside on every step,
        // so the dir's own mtime can be older than its contents.
        let age = newest_age(&p, &meta, now);
        if age <= STALE_AFTER {
            return true;
        }
        tracing::debug!(
            "ignoring stale git marker {} (age {}s) — .git/ sync allowed",
            p.display(),
            age.as_secs()
        );
    }
    false
}

/// Most-recent age across `p` and (if `p` is a directory) one level of
/// children. Cheap — bounded `readdir`, no recursion. Falls back to the
/// passed `meta` if any stat fails.
fn newest_age(p: &Path, meta: &fs::Metadata, now: std::time::SystemTime) -> Duration {
    let age_of = |m: &fs::Metadata| -> Duration {
        m.modified()
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .unwrap_or_default()
    };
    let mut youngest = age_of(meta);
    if meta.is_dir() {
        if let Ok(rd) = fs::read_dir(p) {
            for ent in rd.flatten() {
                if let Ok(cm) = ent.metadata() {
                    let a = age_of(&cm);
                    if a < youngest {
                        youngest = a;
                    }
                }
            }
        }
    }
    youngest
}

/// Keep `.git/` paused this long after git's markers disappear. `git_busy`
/// flickers false between a multi-step operation's sub-steps (lock files
/// appear and vanish per step), so without hysteresis a burst of transient
/// `.git/` churn — lock files, half-written objects — leaks to the peer in
/// the gaps. This bridges them.
const GIT_SETTLE: Duration = Duration::from_secs(5);

/// Sticky gate around `git_busy` plus a defer queue for `.git/` events.
///
/// While git is busy (with hysteresis) we don't drop `.git/` events — we
/// stash them. Once git settles, the caller replays each deferred *path*
/// against its current on-disk state, so intermediate churn collapses to the
/// final result and stray lock files resolve to deletes. Deferred incoming
/// ops are replayed in arrival order.
#[derive(Clone, Default)]
pub struct GitGate {
    inner: Arc<std::sync::Mutex<GitGateInner>>,
}

#[derive(Default)]
struct GitGateInner {
    last_busy: Option<Instant>,
    /// Local `.git/` paths touched while paused — replayed by current state.
    deferred_out: HashSet<PathBuf>,
    /// Incoming `.git/` ops received while paused — replayed in order.
    deferred_in: Vec<Message>,
}

impl GitGate {
    /// Sticky busy check: true while git is mid-operation and for `GIT_SETTLE`
    /// after its markers clear. Refreshes the hysteresis timer when actually
    /// busy.
    pub fn busy(&self, root: &Path) -> bool {
        let raw = git_busy(root);
        let Ok(mut g) = self.inner.lock() else {
            return raw;
        };
        if raw {
            g.last_busy = Some(Instant::now());
            return true;
        }
        matches!(g.last_busy, Some(t) if t.elapsed() < GIT_SETTLE)
    }

    pub fn defer_out(&self, path: PathBuf) {
        if let Ok(mut g) = self.inner.lock() {
            g.deferred_out.insert(path);
        }
    }

    pub fn defer_in(&self, msg: Message) {
        if let Ok(mut g) = self.inner.lock() {
            g.deferred_in.push(msg);
        }
    }

    pub fn has_deferred(&self) -> bool {
        self.inner
            .lock()
            .map(|g| !g.deferred_out.is_empty() || !g.deferred_in.is_empty())
            .unwrap_or(false)
    }

    /// Drain everything deferred while git was busy; the caller replays it.
    pub fn take_deferred(&self) -> (Vec<PathBuf>, Vec<Message>) {
        let Ok(mut g) = self.inner.lock() else {
            return (Vec::new(), Vec::new());
        };
        let out: Vec<PathBuf> = g.deferred_out.drain().collect();
        let inc = std::mem::take(&mut g.deferred_in);
        (out, inc)
    }
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
// Git repo identity.
//
// Syncing root A over root B silently merges two unrelated projects.
// When both roots are git repos we compare their remote URL sets
// (normalized) and refuse on zero overlap — the same repo cloned via
// https and ssh still matches. Roots without `.git` or without any
// remote can't be identified and pass through unchecked.
// ─────────────────────────────────────────────────────────────

/// Normalized remote URLs of the git repo at `root`, sorted and deduped.
/// Empty when `root` is not a git repo, has no remotes, or its config
/// can't be read — identification is best-effort and never blocks sync.
pub fn git_remotes(root: &Path) -> Vec<String> {
    let dot = root.join(".git");
    let config = if dot.is_dir() {
        dot.join("config")
    } else if let Ok(content) = fs::read_to_string(&dot) {
        // `.git` is a file for submodules and linked worktrees:
        // "gitdir: <path>" relative to `root`.
        let Some(target) = content.trim().strip_prefix("gitdir:") else {
            return Vec::new();
        };
        let gitdir = root.join(target.trim());
        // Linked worktrees keep the config in the common dir.
        match fs::read_to_string(gitdir.join("commondir")) {
            Ok(common) => gitdir.join(common.trim()).join("config"),
            Err(_) => gitdir.join("config"),
        }
    } else {
        return Vec::new();
    };
    match fs::read_to_string(&config) {
        Ok(text) => parse_remote_urls(&text),
        Err(_) => Vec::new(),
    }
}

/// Extract and normalize `url = ...` values from `[remote "..."]`
/// sections of a git config. Sorted and deduped.
fn parse_remote_urls(config: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut in_remote = false;
    for line in config.lines() {
        let line = line.trim();
        if let Some(header) = line.strip_prefix('[') {
            in_remote = header
                .split([']', ' ', '\t', '"'])
                .next()
                .is_some_and(|section| section == "remote");
        } else if in_remote {
            if let Some((key, value)) = line.split_once('=') {
                if key.trim() == "url" {
                    let value = value.trim().trim_matches('"');
                    if !value.is_empty() {
                        urls.push(normalize_git_url(value));
                    }
                }
            }
        }
    }
    urls.sort();
    urls.dedup();
    urls
}

fn strip_userinfo(authority: &str) -> &str {
    authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host)
}

fn strip_default_port(host: &str) -> &str {
    [":22", ":80", ":443"]
        .iter()
        .find_map(|port| host.strip_suffix(port))
        .unwrap_or(host)
}

fn trim_repo_suffix(path: &str) -> &str {
    let path = path.trim_end_matches('/');
    path.strip_suffix(".git").unwrap_or(path)
}

/// Canonical form of a git remote URL for comparison: drops scheme,
/// userinfo, default ports (22/80/443) and a trailing `.git`/`/`, then
/// lowercases. `git@github.com:O/R.git`, `ssh://git@github.com:22/O/R`
/// and `https://github.com/O/R` all normalize to `github.com/o/r`.
pub fn normalize_git_url(url: &str) -> String {
    let url = url.trim();
    let (host, path) = if let Some((_, rest)) = url.split_once("://") {
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        (strip_default_port(strip_userinfo(authority)), path)
    } else if let Some((prefix, suffix)) = url.split_once(':') {
        if prefix.contains('/') {
            // A plain local path that happens to contain ':'.
            return trim_repo_suffix(url).to_lowercase();
        }
        // scp-like syntax: [user@]host:path
        (strip_userinfo(prefix), suffix)
    } else {
        return trim_repo_suffix(url).to_lowercase();
    };
    let path = trim_repo_suffix(path);
    if path.is_empty() {
        host.to_lowercase()
    } else {
        format!("{}/{}", host.to_lowercase(), path.to_lowercase())
    }
}

/// True when both sides are identifiable git repos sharing no remote —
/// i.e. syncing would merge two different projects.
pub fn git_remotes_conflict(local: &[String], remote: &[String]) -> bool {
    !local.is_empty() && !remote.is_empty() && !local.iter().any(|url| remote.contains(url))
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
    if entry.kind != EntryKind::File {
        anyhow::bail!("delta entry is not a file: {}", entry.path.display());
    }
    let full = resolve_beneath(root, &entry.path)?;
    if fs::symlink_metadata(&full)?.file_type().is_symlink() {
        anyhow::bail!("delta base is a symlink: {}", entry.path.display());
    }
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
    write_file_atomic(root, entry, &new_content)
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
    if let Err(e) = fs::rename(tmp, final_path) {
        // Don't leak the tmp; we're about to bail.
        let _ = fs::remove_file(tmp);
        use std::io::ErrorKind;
        let hint = match e.kind() {
            ErrorKind::CrossesDevices => {
                " — target on a different filesystem than $HOME; set TMPDIR to a path on the same fs"
            }
            ErrorKind::IsADirectory | ErrorKind::DirectoryNotEmpty => {
                " — target exists as a directory (type conflict); resolve manually"
            }
            _ => "",
        };
        anyhow::bail!(
            "rename {} → {} failed: {}{}",
            tmp.display(),
            final_path.display(),
            e,
            hint
        );
    }
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
    file: Option<fs::File>,
    tmp: PathBuf,
    bytes_written: u64,
    /// Stream-hash chunks as they arrive. Compared against `entry.hash` at
    /// `end()` time so we can refuse to publish a tmp whose bytes don't
    /// match what the sender claimed (corruption, dropped chunks, etc.).
    hasher: blake3::Hasher,
}

impl Drop for InFlight {
    fn drop(&mut self) {
        let _ = self.file.take();
        let _ = fs::remove_file(&self.tmp);
    }
}

#[derive(Default, Clone)]
pub struct Pending {
    inner: Arc<Mutex<HashMap<PathBuf, InFlight>>>,
}

impl Pending {
    pub async fn start(&self, root: &Path, entry: Entry) -> Result<()> {
        if entry.kind != EntryKind::File {
            anyhow::bail!("FileStart entry is not a file: {}", entry.path.display());
        }
        let full = resolve_beneath(root, &entry.path)?;
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = tmp_path(&full);
        let file = fs::File::create(&tmp).with_context(|| format!("open tmp {}", tmp.display()))?;
        let path = entry.path.clone();
        let replaced = self.inner.lock().await.insert(
            path,
            InFlight {
                entry,
                file: Some(file),
                tmp,
                bytes_written: 0,
                hasher: blake3::Hasher::new(),
            },
        );
        if let Some(previous) = replaced {
            let previous_tmp = previous.tmp.clone();
            drop(previous);
            let _ = fs::remove_file(previous_tmp);
        }
        Ok(())
    }

    pub async fn chunk(&self, path: &Path, data: &[u8]) -> Result<()> {
        let mut g = self.inner.lock().await;
        if let Some(s) = g.get_mut(path) {
            s.file
                .as_mut()
                .expect("in-flight file is present until finalization")
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
        let Some(mut s) = self.inner.lock().await.remove(path) else {
            return Ok(None);
        };
        if let Some(file) = s.file.as_ref() {
            file.sync_all().ok();
        }
        drop(s.file.take());

        let actual = *std::mem::take(&mut s.hasher).finalize().as_bytes();
        if actual != s.entry.hash {
            // Drop the bad tmp; do NOT replace the target. Loud error so
            // the session tears down — the reconnect loop will then redo
            // the transfer cleanly on the next attempt.
            anyhow::bail!(
                "chunked transfer hash mismatch for {}: {} bytes received, hash {} vs expected {}",
                s.entry.path.display(),
                s.bytes_written,
                blake3::Hash::from(actual).to_hex(),
                blake3::Hash::from(s.entry.hash).to_hex()
            );
        }

        let full = resolve_beneath(root, &s.entry.path)?;
        finalize_path(&s.tmp, &full, s.entry.mode, s.entry.mtime)?;
        Ok(Some(s.entry.clone()))
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
    if entry.kind != EntryKind::File {
        anyhow::bail!("send_file entry is not a file: {}", entry.path.display());
    }
    let full = resolve_beneath(root, &entry.path)?;
    let size = entry.size as usize;
    // Like rsync's skip-compress policy, don't burn zstd CPU on formats that
    // are already compressed. Framing is per message, so this is safe to
    // decide independently for each file and does not change the protocol.
    let payload_compress = compress && !is_precompressed(&entry.path);
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
            payload_compress,
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
                payload_compress,
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
                payload_compress,
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

/// Common compressed/container formats. Trying zstd and then discarding its
/// output when it is larger saves no bandwidth but can dominate CPU for large
/// media, archives, packages, and build artifacts.
fn is_precompressed(path: &Path) -> bool {
    const EXTENSIONS: &[&str] = &[
        "3g2", "3gp", "7z", "aac", "ace", "apk", "avi", "bz2", "deb", "dmg", "ear", "flac", "flv",
        "gpg", "gz", "iso", "jar", "jpeg", "jpg", "lz4", "lzma", "m4a", "m4v", "mkv", "mov", "mp3",
        "mp4", "mpeg", "mpg", "ogg", "opus", "png", "rar", "rpm", "tgz", "webm", "webp", "xz",
        "zip", "zst",
    ];
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

#[cfg(test)]
#[path = "peer_tests.rs"]
mod tests;

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
    /// The watcher observed a local delete we did NOT apply ourselves.
    /// Feeds the stale-create guard (`is_recently_deleted`) so in-flight
    /// peer data can't resurrect the path, but is never an echo — the
    /// delete still has to be forwarded to the peer.
    ObservedDeleted,
}

const NO_HASH: [u8; 32] = [0u8; 32];

#[derive(Default)]
struct SuppInner {
    map: HashMap<PathBuf, (ApplyState, Instant)>,
    /// Last time we pruned expired entries; gates the O(map) sweep.
    last_sweep: Option<Instant>,
}

/// Synchronous suppression map — uses `std::sync::Mutex` so the watcher's
/// notify thread can update it eagerly (before debouncing) and so all
/// methods are callable from both sync and async contexts without holding
/// an async lock across awaits.
#[derive(Default, Clone)]
pub struct Suppression {
    inner: Arc<std::sync::Mutex<SuppInner>>,
}

impl Suppression {
    /// Record that the path now exists with `mtime` and (optionally) `hash`.
    /// Use `NO_HASH` for dirs / symlinks.
    pub fn mark_set(&self, path: PathBuf, mtime_ns: i64, hash: [u8; 32]) {
        if let Ok(mut g) = self.inner.lock() {
            g.map.insert(
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
            g.map.insert(path, (ApplyState::Deleted, Instant::now()));
        }
    }

    /// Record a delete the watcher observed but we did not apply. Never
    /// downgrades a live applied `Deleted` mark — the watcher echo of our
    /// own apply must stay suppressible, or every applied delete would
    /// bounce a redundant Delete back to the peer.
    pub fn mark_observed_deleted(&self, path: PathBuf) {
        if let Ok(mut g) = self.inner.lock() {
            match g.map.get(&path) {
                Some((ApplyState::Deleted, t)) if t.elapsed() < SUPPRESS_TTL => {}
                _ => {
                    g.map
                        .insert(path, (ApplyState::ObservedDeleted, Instant::now()));
                }
            }
        }
    }

    /// True if we recently deleted (or sent a delete for, or observed the
    /// local deletion of) this path.
    pub fn is_recently_deleted(&self, path: &Path) -> bool {
        let Ok(g) = self.inner.lock() else {
            return false;
        };
        matches!(
            g.map.get(path),
            Some((ApplyState::Deleted | ApplyState::ObservedDeleted, _))
        )
    }

    /// Return the content hash we have on record for this file, if any.
    /// Used by the sender to skip retransmitting unchanged content.
    pub fn prior_hash(&self, path: &Path) -> Option<[u8; 32]> {
        let g = self.inner.lock().ok()?;
        match g.map.get(path) {
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
        // Prune expired entries at most once per SUPPRESS_SWEEP rather than on
        // every event — see SUPPRESS_SWEEP.
        let sweep_due = g
            .last_sweep
            .map(|t| now.duration_since(t) >= SUPPRESS_SWEEP)
            .unwrap_or(true);
        if sweep_due {
            g.map
                .retain(|_, (_, t)| now.duration_since(*t) < SUPPRESS_TTL);
            g.last_sweep = Some(now);
        }

        let key: &Path = match ev {
            FsEvent::Created(p) | FsEvent::Modified(p) | FsEvent::Removed(p) => p,
            FsEvent::Renamed { to, .. } => to,
        };
        let Some((state, _)) = g.map.get(key) else {
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

/// Static per-session configuration shared between `live_loop`,
/// `handle_incoming`, and helpers. Pulled out into a struct so those
/// signatures stay narrow.
pub struct SessionCtx {
    pub root: PathBuf,
    pub mode: SyncMode,
    pub compress: bool,
    pub is_client: bool,
    pub ignores: Arc<IgnoreStack>,
    /// Sticky `.git/` pause + defer queue (see `GitGate`).
    pub gate: GitGate,
    /// Live three-way-diff baseline, kept current as the loop converges.
    /// Disabled (no-op) on the agent side.
    pub baseline: LiveBaseline,
}

fn directions(mode: SyncMode, is_client: bool) -> (bool, bool) {
    match (mode, is_client) {
        (SyncMode::Both, _) => (true, true),
        (SyncMode::Push, true) => (true, false),
        (SyncMode::Push, false) => (false, true),
        (SyncMode::Pull, true) => (false, true),
        (SyncMode::Pull, false) => (true, false),
    }
}

/// How often the client re-walks the tree to catch changes the watcher
/// never reported. Unchanged files cost one stat (hash cache), and a tick
/// with no watcher activity since the last one is skipped outright, so an
/// idle session never re-walks.
const RECONCILE_INTERVAL: Duration = Duration::from_secs(30);

/// Diff the live tree against the converged baseline and return synthetic
/// events for every divergence — the safety net for watcher misses.
///
/// Why this exists: the watcher pipeline is lossy under rapid churn. Measured
/// on macOS FSEvents (see `watcher::tests::exploratory_git_style_churn`),
/// unlink+recreate collapses into a single coalesced notification that the
/// debouncer may drop entirely, and event *kinds* are unreliable (a delete
/// can surface as `Modified`). A `git checkout` + `pull --rebase` rewrites
/// hundreds of files in exactly this pattern, leaving the peer silently
/// desynced until a restart re-ran the manifest diff. This sweep is that
/// diff, run periodically against the live baseline.
///
/// No-op while git is mid-operation (the walk would exclude `.git/` and
/// misreport those paths as deleted) and when there is no baseline yet.
async fn reconcile_sweep(
    root: &Path,
    baseline: &LiveBaseline,
    cache: &Arc<StdMutex<HashCache>>,
    gate: &GitGate,
) -> Result<Vec<FsEvent>> {
    if baseline.is_empty() {
        return Ok(Vec::new());
    }
    if gate.busy(root) {
        return Ok(Vec::new());
    }

    let walk_root = root.to_path_buf();
    let cache = Arc::clone(cache);
    let (manifest, excluded) =
        tokio::task::spawn_blocking(move || -> Result<(Vec<Entry>, Vec<PathBuf>)> {
            let mut cache = cache
                .lock()
                .map_err(|_| anyhow::anyhow!("hash cache mutex poisoned"))?;
            walk_manifest(&walk_root, &mut cache, None)
        })
        .await??;

    let on_disk: HashSet<&Path> = manifest.iter().map(|e| e.path.as_path()).collect();
    // Diff under the baseline lock rather than cloning the whole map first;
    // nothing else mutates it while the live loop task is in here.
    let events = baseline.with_entries(|base| {
        let mut events = Vec::new();
        for entry in &manifest {
            match base.get(&entry.path) {
                Some(converged) if converged.same_content(entry) => {}
                _ => events.push(FsEvent::Modified(entry.path.clone())),
            }
        }
        for path in base.keys() {
            if !on_disk.contains(path.as_path()) {
                // Git went busy between the gate check above and the walk:
                // the walker paused the subtree, so its absence from the
                // manifest is not deletion evidence (same contract as the
                // manifest exchange).
                if excluded.iter().any(|prefix| path.starts_with(prefix)) {
                    continue;
                }
                events.push(FsEvent::Removed(path.clone()));
            }
        }
        events
    });
    Ok(events)
}

/// Run one sweep and forward whatever it found through the normal event
/// path (echo suppression, ignore rules, and disk-state grounding all
/// apply). Also persists the hash cache if the walk hashed new content.
async fn run_reconcile_sweep<W>(
    ctx: &SessionCtx,
    writer: &Arc<Mutex<W>>,
    suppress: &Suppression,
    cache: &Arc<StdMutex<HashCache>>,
) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let events = reconcile_sweep(&ctx.root, &ctx.baseline, cache, &ctx.gate).await?;
    if events.is_empty() {
        return Ok(());
    }
    tracing::debug!("reconcile: {} divergent path(s)", events.len());
    forward_local_events(
        &ctx.root,
        events,
        writer,
        ctx.compress,
        suppress,
        ctx.is_client,
        &ctx.ignores,
        &ctx.gate,
        &ctx.baseline,
    )
    .await?;
    if let Ok(mut cache) = cache.lock() {
        cache.save(&ctx.root);
    }
    Ok(())
}

pub async fn live_loop<R, W>(
    ctx: SessionCtx,
    mut reader: R,
    writer: Arc<Mutex<W>>,
    // Carried over from the init-sync apply phase: marks for every file we
    // wrote during initial sync stay valid here (TTL 60s) so the watcher's
    // FSEvents/inotify echoes for those writes are filtered, not bounced
    // back to the peer as spurious local changes.
    suppress: Suppression,
    pending: Pending,
    // The watcher is spawned BEFORE the initial sync so events for files
    // the user modifies during the walk/exchange/apply window aren't lost.
    // Caller owns spawning; we receive the live channel + keepalive here.
    watcher_handle: watcher::WatcherHandle,
    // Client-only: shared hash cache enabling the periodic reconciliation
    // sweep (see `reconcile_sweep`). `None` on the agent.
    reconcile: Option<Arc<StdMutex<HashCache>>>,
) -> Result<()>
where
    R: AsyncRead + AsyncReadExt + Unpin + Send + 'static,
    W: AsyncWrite + AsyncWriteExt + Unpin + Send,
{
    let (send_local, apply_remote) = directions(ctx.mode, ctx.is_client);

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

    let watcher::WatcherHandle {
        events: mut event_rx,
        activity,
        ids: _,
        keepalive: _watcher,
    } = watcher_handle;

    let sigint = tokio::signal::ctrl_c();
    tokio::pin!(sigint);

    // Drives the deferred-`.git/` replay: once git settles, the queued events
    // are flushed against their current state. 1s latency to resume `.git/`
    // sync after an operation finishes is imperceptible.
    let mut git_tick = tokio::time::interval(Duration::from_secs(1));
    git_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Drives the client-side reconciliation sweep: re-walk the tree and
    // diff it against the baseline to catch changes the watcher never
    // reported. FSEvents/debouncer coalescing drops real events during
    // rapid churn (git checkout/rebase); without this net they'd stay
    // unsynced until a restart.
    let mut reconcile_tick = tokio::time::interval(RECONCILE_INTERVAL);
    reconcile_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            biased;

            _ = &mut sigint => {
                tracing::info!("ctrl+c — closing");
                let mut w = writer.lock().await;
                let _ = write_message(&mut *w, &Message::Bye, ctx.compress).await;
                break;
            }

            msg = msg_rx.recv() => {
                match msg {
                    Some(Ok(Message::Bye)) => break,
                    Some(Ok(m)) => {
                        // Per-op apply errors are non-fatal — log and
                        // continue. Connection-level failures appear as
                        // Err from the reader task (the next arm).
                        if let Err(e) = handle_incoming(&ctx, m, &suppress, &pending, &writer, apply_remote).await {
                            tracing::warn!("apply failed: {}", e);
                            let mut w = writer.lock().await;
                            let _ = write_message(&mut *w, &Message::Error(format!("{e}")), ctx.compress).await;
                        }
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
                    forward_local_events(&ctx.root, events, &writer, ctx.compress, &suppress, ctx.is_client, &ctx.ignores, &ctx.gate, &ctx.baseline).await?;
                }
            }

            _ = git_tick.tick() => {
                // Replay `.git/` events deferred during a git operation, but
                // only once git has actually settled.
                if ctx.gate.has_deferred() && !ctx.gate.busy(&ctx.root) {
                    let (paths, msgs) = ctx.gate.take_deferred();
                    if send_local && !paths.is_empty() {
                        // Synthesize a Modified for each touched path;
                        // forward_local_events re-reads current state, so a
                        // path that's now gone becomes a Delete.
                        let events: Vec<FsEvent> = paths.into_iter().map(FsEvent::Modified).collect();
                        forward_local_events(&ctx.root, events, &writer, ctx.compress, &suppress, ctx.is_client, &ctx.ignores, &ctx.gate, &ctx.baseline).await?;
                    }
                    for m in msgs {
                        if let Err(e) = handle_incoming(&ctx, m, &suppress, &pending, &writer, apply_remote).await {
                            tracing::warn!("deferred apply failed: {}", e);
                        }
                    }
                    // A git mass rewrite is the highest-risk window for
                    // swallowed watcher events — sweep now instead of
                    // waiting for the periodic tick.
                    if send_local {
                        if let Some(cache) = &reconcile {
                            if let Err(e) = run_reconcile_sweep(&ctx, &writer, &suppress, cache).await {
                                tracing::warn!("reconcile sweep failed: {}", e);
                            }
                        }
                    }
                }
            }

            _ = reconcile_tick.tick() => {
                // The sweep exists to catch events the watcher dropped under
                // churn; a tree it reported nothing about has nothing to
                // catch, so skip the walk entirely.
                if send_local && activity.swap(false, Ordering::Relaxed) {
                    if let Some(cache) = &reconcile {
                        if let Err(e) = run_reconcile_sweep(&ctx, &writer, &suppress, cache).await {
                            tracing::warn!("reconcile sweep failed: {}", e);
                        }
                    }
                }
            }
        }
    }

    // Flush the latest converged state before we tear down / reconnect.
    ctx.baseline.persist_now();
    reader_task.abort();
    Ok(())
}

pub async fn handle_incoming<W>(
    ctx: &SessionCtx,
    msg: Message,
    suppress: &Suppression,
    pending: &Pending,
    writer: &Arc<Mutex<W>>,
    apply_remote: bool,
) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    // Locals so the existing body reads naturally and we don't repeat
    // `ctx.foo` access dozens of times. Cheap; nothing is cloned.
    let root: &Path = &ctx.root;
    let compress = ctx.compress;
    let ignores = &ctx.ignores;
    // Only the client prints user-facing event lines. The agent's stderr is
    // forwarded over SSH to the same terminal, so any logs there would just
    // duplicate the client's transcript.
    let log_event = ctx.is_client;
    // If git is mid-operation locally, don't apply any change under `.git/` —
    // the peer (who may NOT be busy) would clobber our in-progress rebase/merge
    // state and break ref locking. Sticky (hysteresis) so brief gaps between
    // git's sub-steps don't open the gate.
    let busy = ctx.gate.busy(root);
    let path_of = |m: &Message| -> Option<PathBuf> {
        match m {
            Message::FileData { entry, .. } => Some(entry.path.clone()),
            Message::FileStart { entry, .. } => Some(entry.path.clone()),
            Message::FileChunk { path, .. } => Some(path.clone()),
            Message::FileEnd { path } => Some(path.clone()),
            Message::MkDir { entry } => Some(entry.path.clone()),
            Message::MkSymlink { entry } => Some(entry.path.clone()),
            Message::Delete { path } => Some(path.clone()),
            Message::Rename { from: _, to } => Some(to.clone()),
            Message::Delta { entry, .. } => Some(entry.path.clone()),
            Message::Touch { path, .. } => Some(path.clone()),
            _ => None,
        }
    };
    if busy {
        if let Some(p) = path_of(&msg) {
            if is_under_git(&p) {
                // Defer, don't drop: replayed once git settles (see live_loop).
                tracing::debug!("git busy: defer incoming for {}", p.display());
                ctx.gate.defer_in(msg);
                return Ok(());
            }
        }
    }
    match msg {
        Message::FileData { entry, content } => {
            if !apply_remote {
                return Ok(());
            }
            if ignores.is_ignored_rel(&entry.path, false) {
                tracing::debug!("ignored (recv FileData): {}", entry.path.display());
                return Ok(());
            }
            // Receiver dedup: if our disk already has this exact content,
            // skip the write entirely (and the noisy log line).
            if is_already_equal(root, &entry) {
                let mt = lstat_mtime_ns(&root.join(&entry.path));
                suppress.mark_set(entry.path.clone(), mt, entry.hash);
                tracing::trace!("dedup (recv FileData): {}", entry.path.display());
                ctx.baseline.set(entry);
                return Ok(());
            }
            // Stale-create guard: peer is sending us a file we just deleted.
            // Their FileData was already on the wire when our Delete arrived,
            // so drop it instead of resurrecting the file the user removed.
            let full = resolve_beneath(root, &entry.path)?;
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
            ctx.baseline.set(entry);
        }
        Message::FileStart { entry, .. } => {
            if !apply_remote {
                return Ok(());
            }
            if ignores.is_ignored_rel(&entry.path, false) {
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
                ctx.baseline.set(entry);
                return Ok(());
            }
            // Stale-create guard (chunked transfer variant).
            let full = resolve_beneath(root, &entry.path)?;
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
            if ignores.is_ignored_rel(&path, false) {
                return Ok(());
            }
            pending.chunk(&path, &data).await?;
        }
        Message::FileEnd { path } => {
            if !apply_remote {
                return Ok(());
            }
            if ignores.is_ignored_rel(&path, false) {
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
                ctx.baseline.set(entry);
            }
        }
        Message::Touch { path, mtime, mode } => {
            if !apply_remote {
                return Ok(());
            }
            if ignores.is_ignored_rel(&path, false) {
                return Ok(());
            }
            let full = resolve_beneath(root, &path)?;
            let Ok(meta) = fs::symlink_metadata(&full) else {
                // No file to touch (we may have deleted it, or never had it).
                // Drop quietly; if peer actually needs us to create it they'll
                // re-send a full FileData.
                tracing::debug!("touch for missing path: {}", path.display());
                return Ok(());
            };
            if !meta.is_file() || meta.file_type().is_symlink() {
                anyhow::bail!("touch target is not a regular file: {}", path.display());
            }
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
            if ignores.is_ignored_rel(&entry.path, true) {
                tracing::debug!("ignored (recv MkDir): {}", entry.path.display());
                return Ok(());
            }
            if is_already_equal(root, &entry) {
                let mt = lstat_mtime_ns(&root.join(&entry.path));
                suppress.mark_mtime(entry.path.clone(), mt);
                ctx.baseline.set(entry);
                return Ok(());
            }
            let full = resolve_beneath(root, &entry.path)?;
            if !full.exists() && suppress.is_recently_deleted(&entry.path) {
                tracing::debug!(
                    "dropping stale MkDir after delete: {}",
                    entry.path.display()
                );
                return Ok(());
            }
            apply_mkdir(root, &entry)?;
            ctx.baseline.set(entry.clone());
            // Use the actual on-disk mtime (dir mtime changes whenever
            // children are added) so future echoes match precisely.
            let mt = lstat_mtime_ns(&full);
            suppress.mark_mtime(entry.path, mt);
        }
        Message::MkSymlink { entry } => {
            if !apply_remote {
                return Ok(());
            }
            if ignores.is_ignored_rel(&entry.path, false) {
                return Ok(());
            }
            if is_already_equal(root, &entry) {
                let mt = lstat_mtime_ns(&root.join(&entry.path));
                suppress.mark_mtime(entry.path.clone(), mt);
                ctx.baseline.set(entry);
                return Ok(());
            }
            let full = resolve_beneath(root, &entry.path)?;
            if !full.exists() && suppress.is_recently_deleted(&entry.path) {
                tracing::debug!(
                    "dropping stale MkSymlink after delete: {}",
                    entry.path.display()
                );
                return Ok(());
            }
            apply_symlink(root, &entry)?;
            ctx.baseline.set(entry.clone());
            let mt = lstat_mtime_ns(&full);
            suppress.mark_mtime(entry.path, mt);
        }
        Message::Delete { path } => {
            if !apply_remote {
                return Ok(());
            }
            if ignores.is_ignored_rel(&path, false) && ignores.is_ignored_rel(&path, true) {
                return Ok(());
            }
            let existed_before = resolve_beneath(root, &path)
                .ok()
                .is_some_and(|full| fs::symlink_metadata(full).is_ok());
            apply_delete(root, &path)?;
            ctx.baseline.remove(&path);
            suppress.mark_deleted(path.clone());
            if existed_before && log_event {
                eprintln!("  {} × {}", "←".bright_cyan(), path.display());
            }
        }
        Message::Rename { from, to } => {
            if !apply_remote {
                return Ok(());
            }
            if ignores.is_ignored_rel(&from, false) || ignores.is_ignored_rel(&to, false) {
                tracing::debug!(
                    "ignored (recv Rename): {} → {}",
                    from.display(),
                    to.display()
                );
                return Ok(());
            }
            // Stale-rename guard: if the source is gone because we just
            // deleted it, a Rename(from, to) is meaningless — drop it.
            let from_full = resolve_beneath(root, &from)?;
            if !from_full.exists() && suppress.is_recently_deleted(&from) {
                tracing::debug!(
                    "dropping stale Rename after delete: {} → {}",
                    from.display(),
                    to.display()
                );
                return Ok(());
            }
            apply_rename(root, &from, &to)?;
            ctx.baseline.rename(&from, &to);
            if let Some(e) = build_entry(root, &to)? {
                ctx.baseline.set(e);
            }
            suppress.mark_deleted(from);
            let mt = lstat_mtime_ns(&root.join(&to));
            suppress.mark_mtime(to, mt);
        }
        Message::FileGet { path } => {
            if ignores.is_ignored_rel(&path, false) && ignores.is_ignored_rel(&path, true) {
                return Ok(());
            }
            if let Some(entry) = build_entry(root, &path)? {
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
        // Per-op error reported by the peer (type conflict, perm denied,
        // etc.). Log and keep the session alive — bailing would just
        // trigger a reconnect that repeats the same failure.
        Message::Error(e) => tracing::warn!("peer error: {e}"),
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
///   - First event is `Created` AND last event is `Removed` AND the path is
///     gone from disk → **drop the whole path**. The file lived and died
///     inside the debouncer window; it's an ephemeral artifact (Vim's `4913`
///     probe, atomic-write tmps, IDE scratch files). Sending a Delete for
///     something the peer never saw is noise.
///   - First event is `Created` AND last event is `Removed` BUT the path
///     exists on disk → keep as `Modified`. FSEvents coalesces an
///     unlink+recreate into a single notification carrying both the
///     Created and Removed flags; the file is alive with new content, so
///     dropping it would silently desync the peer.
///   - Otherwise → keep the last event (most recent state wins).
///
/// Renames are keyed on their destination path.
fn coalesce(root: &Path, events: Vec<FsEvent>) -> Vec<FsEvent> {
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
    // Paths kept as `Modified` instead of their recorded last event
    // (removed+recreated inside the window — see policy above).
    let mut as_modified: HashMap<usize, PathBuf> = HashMap::new();
    for &(first, last) in first_last.values() {
        if matches!(events[first], FsEvent::Created(_))
            && matches!(events[last], FsEvent::Removed(_))
        {
            let key = key_of(&events[first]);
            if root.join(&key).symlink_metadata().is_ok() {
                // Removed+recreated inside the window (coalesced FSEvent):
                // the file is alive — sync its current state.
                keep.insert(last);
                as_modified.insert(last, key);
            }
            // Ephemeral: created and gone before we even fired. Skip entirely.
            continue;
        }
        keep.insert(last);
    }

    events
        .into_iter()
        .enumerate()
        .filter(|(i, _)| keep.contains(i))
        .map(|(i, ev)| match as_modified.get(&i) {
            Some(path) => FsEvent::Modified(path.clone()),
            None => ev,
        })
        .collect()
}

/// Apply authoritative ignore rules at the send boundary. The watcher also
/// filters after its ignore state is ready, but this second check handles
/// events buffered while the manifest was discovering nested ignore files.
fn filter_outgoing_event(root: &Path, ignores: &IgnoreStack, event: FsEvent) -> Option<FsEvent> {
    match event {
        FsEvent::Created(path) => {
            let is_dir = root.join(&path).is_dir();
            (!ignores.is_ignored_rel(&path, is_dir)).then_some(FsEvent::Created(path))
        }
        FsEvent::Modified(path) => {
            let is_dir = root.join(&path).is_dir();
            (!ignores.is_ignored_rel(&path, is_dir)).then_some(FsEvent::Modified(path))
        }
        FsEvent::Removed(path) => {
            let ignored =
                ignores.is_ignored_rel(&path, false) || ignores.is_ignored_rel(&path, true);
            (!ignored).then_some(FsEvent::Removed(path))
        }
        FsEvent::Renamed { from, to } => {
            let from_ignored = ignores.is_ignored_rel(&from, false);
            let to_ignored = ignores.is_ignored_rel(&to, root.join(&to).is_dir());
            match (from_ignored, to_ignored) {
                (false, false) => Some(FsEvent::Renamed { from, to }),
                (false, true) => Some(FsEvent::Removed(from)),
                (true, false) => Some(FsEvent::Created(to)),
                (true, true) => None,
            }
        }
    }
}

#[allow(clippy::too_many_arguments)] // distinct session pieces; also called pre-SessionCtx
pub async fn forward_local_events<W>(
    root: &Path,
    events: Vec<FsEvent>,
    writer: &Arc<Mutex<W>>,
    compress: bool,
    suppress: &Suppression,
    is_client: bool,
    ignores: &IgnoreStack,
    gate: &GitGate,
    baseline: &LiveBaseline,
) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    // Only the client prints. On the agent, the same eprintln would be
    // forwarded over SSH stderr and duplicate every transfer line.
    let log_event = is_client;
    let events = coalesce(root, events)
        .into_iter()
        .filter_map(|event| filter_outgoing_event(root, ignores, event));
    // Once per batch: if git is mid-operation, defer every event that touches
    // .git/. Prevents partial rebase/merge state from leaking to the peer
    // where it would race with the peer's own ref updates. Sticky (hysteresis)
    // so the brief gaps between git's sub-steps don't open the gate.
    let pause_git = gate.busy(root);
    for ev in events {
        // Watcher event kinds are hints, not truth. Measured on macOS
        // FSEvents: a plain delete can arrive as `Modified`, a write as
        // `Created`, and an unlink+recreate collapses into one coalesced
        // notification — so a `Removed` may surface for a path that has
        // since come back (git checkout/rebase rewrite files this way).
        // Ground the decision in current disk state: a "remove" for a path
        // that exists is a content send, never a delete. Deletes only leave
        // here when the file is verifiably gone.
        let ev = match ev {
            FsEvent::Removed(ref p) if root.join(p).symlink_metadata().is_ok() => {
                FsEvent::Modified(p.clone())
            }
            ev => ev,
        };
        if suppress.is_echo(root, &ev) {
            tracing::trace!("echo suppressed: {:?}", ev);
            continue;
        }
        if pause_git {
            let key = match &ev {
                FsEvent::Created(p) | FsEvent::Modified(p) | FsEvent::Removed(p) => p,
                FsEvent::Renamed { to, .. } => to,
            };
            if is_under_git(key) {
                // Defer, don't drop: replayed against current state once git
                // settles (see live_loop's git_tick).
                tracing::debug!("git busy: defer event {:?}", ev);
                gate.defer_out(key.clone());
                continue;
            }
        }

        match ev {
            FsEvent::Created(p) | FsEvent::Modified(p) => {
                let entry = match build_entry(root, &p)? {
                    Some(e) => e,
                    None => {
                        // The path doesn't exist anymore. This commonly
                        // happens when a Remove + Modify fire in the same
                        // debouncer batch and coalesce kept the Modify
                        // (FSEvents on macOS is chatty during `rm`). The
                        // user's intent is a delete — treat it as such.
                        if log_event {
                            eprintln!("  {} × {}", "→".bright_green(), p.display());
                        }
                        {
                            let mut w = writer.lock().await;
                            write_message(&mut *w, &Message::Delete { path: p.clone() }, compress)
                                .await?;
                        }
                        baseline.remove(&p);
                        suppress.mark_deleted(p);
                        continue;
                    }
                };
                let path_clone = entry.path.clone();
                let entry_mtime = entry.mtime;
                let entry_hash = entry.hash;
                let entry_kind = entry.kind;
                // Snapshot for the baseline before `entry` is moved into the
                // outgoing message below.
                let baseline_entry = entry.clone();
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
                // This content is now on both sides — record it as the
                // converged baseline (so a later delete of it is detectable).
                baseline.set(baseline_entry);
            }
            FsEvent::Removed(p) => {
                if log_event {
                    eprintln!("  {} × {}", "→".bright_green(), p.display());
                }
                {
                    let mut w = writer.lock().await;
                    write_message(&mut *w, &Message::Delete { path: p.clone() }, compress).await?;
                }
                // Record that *we* deleted this — receiver dedup uses this
                // to drop stale FileData / MkDir for the same path arriving
                // out-of-order from the peer.
                baseline.remove(&p);
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
                let entry = build_entry(root, &to)?;
                // The peer now holds our last converged content under `to`;
                // when that is still what's on disk, only metadata is owed.
                let unchanged = entry.as_ref().is_some_and(|e| {
                    baseline.with_entries(|b| b.get(&from).is_some_and(|old| old.same_content(e)))
                });
                // Re-key the converged subtree: the move deleted and created
                // nothing, and the sweep must not conclude otherwise.
                baseline.rename(&from, &to);
                if let Some(entry) = entry {
                    let to_mtime = entry.mtime;
                    let baseline_entry = entry.clone();
                    match entry.kind {
                        EntryKind::File if unchanged => {
                            let mut w = writer.lock().await;
                            write_message(
                                &mut *w,
                                &Message::Touch {
                                    path: to.clone(),
                                    mtime: entry.mtime,
                                    mode: entry.mode,
                                },
                                compress,
                            )
                            .await?;
                        }
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
                    baseline.set(baseline_entry);
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
