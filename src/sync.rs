//! Client-side orchestration: handshake, manifest exchange, plan & apply
//! the initial diff, then hand off to the bidirectional live loop.

use anyhow::{Context, Result};
use humansize::{format_size, BINARY};
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Instant;
use tokio::io::{BufReader, BufWriter};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::Mutex;

use crate::cache::HashCache;
use crate::cli::ClientArgs;
use crate::ignores::IgnoreStack;
use crate::peer::{
    apply_delete, apply_delta_to_file, apply_file_data, apply_mkdir, apply_rename, apply_symlink,
    compute_delta, compute_signature, live_loop, send_file, Pending, Suppression,
};
use crate::protocol::{
    read_message, write_message, Entry, EntryKind, Message, SyncMode, PROTOCOL_VERSION,
};
use crate::transport::{parse_remote, spawn_ssh};
use crate::walker::{ensure_root, walk_manifest};

pub async fn run(args: ClientArgs) -> Result<()> {
    let local_root = ensure_root(std::path::Path::new(&args.local))
        .with_context(|| format!("invalid local path: {}", args.local))?;
    let remote = parse_remote(&args.remote)?;
    crate::ui::banner(&local_root, &remote, args.mode);

    let mut child = spawn_ssh(&remote, args.ssh_opts.as_deref(), &args.remote_synx)?;
    let stdin = child.stdin.take().context("ssh stdin missing")?;
    let stdout = child.stdout.take().context("ssh stdout missing")?;
    let mut reader = BufReader::new(stdout);
    let writer_inner = BufWriter::new(stdin);
    let compress = !args.no_compress;

    // ── Handshake ──
    {
        let mut w = writer_inner;
        write_message(
            &mut w,
            &Message::Hello {
                version: PROTOCOL_VERSION,
                root: PathBuf::from(&remote.path),
                mode: args.mode,
                compress,
            },
            false,
        )
        .await?;
        let writer = Arc::new(Mutex::new(w));

        match read_message(&mut reader).await? {
            Message::HelloAck {
                version,
                root_existed,
            } => {
                if version != PROTOCOL_VERSION {
                    anyhow::bail!(
                        "protocol mismatch (local={PROTOCOL_VERSION}, remote={version}). \
                         Update synx on both sides."
                    );
                }
                if !root_existed {
                    crate::ui::warn(&format!("remote path created: {}", remote.path));
                } else {
                    crate::ui::ok("connected");
                }
            }
            Message::Error(e) => anyhow::bail!("remote rejected handshake: {e}"),
            m => anyhow::bail!("unexpected handshake reply: {:?}", m),
        }

        run_inner(local_root, args, compress, reader, writer, child).await
    }
}

async fn run_inner(
    local_root: PathBuf,
    args: ClientArgs,
    compress: bool,
    mut reader: BufReader<ChildStdout>,
    writer: Arc<Mutex<BufWriter<ChildStdin>>>,
    mut child: tokio::process::Child,
) -> Result<()> {
    // ── Local manifest (parallel walk with hash cache) ──
    let cache = Arc::new(StdMutex::new(HashCache::load(&local_root)));
    let started = Instant::now();
    let root_for_walk = local_root.clone();
    let cache_for_walk = cache.clone();
    let local_manifest =
        tokio::task::spawn_blocking(move || walk_manifest(&root_for_walk, &cache_for_walk))
            .await??;
    let walk_ms = started.elapsed().as_millis();
    tracing::debug!(
        "local walk: {} entries in {} ms",
        local_manifest.len(),
        walk_ms
    );

    // ── Send local manifest in parallel with receiving remote's ──
    let writer_for_send = writer.clone();
    let local_manifest_clone = local_manifest.clone();
    let send_manifest_task = tokio::spawn(async move {
        let mut w = writer_for_send.lock().await;
        write_message(&mut *w, &Message::ManifestBegin, compress).await?;
        for e in &local_manifest_clone {
            write_message(&mut *w, &Message::ManifestEntry(e.clone()), compress).await?;
        }
        write_message(&mut *w, &Message::ManifestEnd, compress).await?;
        Ok::<(), anyhow::Error>(())
    });

    let raw_remote = receive_manifest(&mut reader).await?;
    send_manifest_task.await??;

    // .gitignore / .synxignore is authoritative for what we will sync.
    // The remote agent doesn't know our ignore rules, so we filter its
    // manifest through our local IgnoreStack before computing the plan.
    let ignores = Arc::new(IgnoreStack::load(&local_root));
    let before = raw_remote.len();
    let remote_manifest: Vec<Entry> = raw_remote
        .into_iter()
        .filter(|e| {
            let is_dir = matches!(e.kind, EntryKind::Dir);
            !ignores.is_ignored_rel(&e.path, is_dir)
        })
        .collect();
    let filtered = before - remote_manifest.len();
    if filtered > 0 {
        tracing::debug!("filtered {} ignored remote entries", filtered);
    }

    crate::ui::info(&format!(
        "manifests:  local {}  •  remote {}{}",
        local_manifest.len().to_string().bright_white(),
        remote_manifest.len().to_string().bright_white(),
        if filtered > 0 {
            format!(" ({} ignored)", filtered).dimmed().to_string()
        } else {
            String::new()
        },
    ));

    // Build path indices up-front. `remote_hash_by` tells the push side
    // which files are present on the remote at a different content (delta
    // candidates). `local_file_by` tells the pull side which files we
    // already have (so we can offer the server a signature → get a delta
    // back instead of the whole file).
    let remote_hash_by: HashMap<PathBuf, [u8; 32]> = remote_manifest
        .iter()
        .filter(|e| matches!(e.kind, EntryKind::File))
        .map(|e| (e.path.clone(), e.hash))
        .collect();
    let local_file_by: HashMap<PathBuf, Entry> = local_manifest
        .iter()
        .filter(|e| matches!(e.kind, EntryKind::File))
        .map(|e| (e.path.clone(), e.clone()))
        .collect();

    // ── Diff ──
    let plan = build_plan(&local_manifest, &remote_manifest, args.mode);
    plan.print();

    if args.dry_run {
        let mut w = writer.lock().await;
        let _ = write_message(&mut *w, &Message::Bye, compress).await;
        drop(w);
        let _ = child.wait().await;
        return Ok(());
    }

    // ── Execute initial sync ──
    // Phase 1 (sequential):    dirs + symlinks. Parents must exist.
    // Phase 2a (sequential):   delta sync for files where remote has an
    //                          older version and the file is in our size
    //                          band — saves wire bytes on large mutable
    //                          files (logs, dumps, binaries that change
    //                          slightly). Sequential because each item is
    //                          a request → response → delta round-trip.
    // Phase 2b (parallel):     full push for everything else, Semaphore-
    //                          bounded so we don't blow up RAM.
    // Phase 3 (sequential):    FileGet pulls + SyncDone marker.
    const MAX_CONCURRENT_PUSHES: usize = 8;
    /// Below this size, the round-trip + signature overhead exceeds the
    /// bytes saved by sending a delta — just push the whole thing.
    const DELTA_MIN_SIZE: u64 = 256 * 1024;
    /// Above this, we don't want to load `base + new` into RAM at once.
    /// Larger files fall back to the chunked full-file path.
    const DELTA_MAX_SIZE: u64 = 256 * 1024 * 1024;

    // Channel: recv loop forwards `Signature` messages here so the send
    // task (which fired the matching `SignatureRequest`) can await them.
    let (sig_tx, mut sig_rx) = tokio::sync::mpsc::unbounded_channel::<(PathBuf, Option<Vec<u8>>)>();

    let push_plan = plan.push.clone();
    let get_plan = plan.get.clone();
    let writer_for_send = writer.clone();
    let local_root_for_send = local_root.clone();
    let remote_hash_by_send = remote_hash_by.clone();
    let local_file_by_send = local_file_by.clone();
    let send_task = tokio::spawn(async move {
        use std::sync::atomic::{AtomicU64, Ordering};
        use tokio::sync::Semaphore;

        let (non_files, all_files): (Vec<Entry>, Vec<Entry>) = push_plan
            .into_iter()
            .partition(|e| !matches!(e.kind, EntryKind::File));

        // Phase 1.
        for e in non_files {
            let mut w = writer_for_send.lock().await;
            match e.kind {
                EntryKind::Dir => {
                    write_message(&mut *w, &Message::MkDir { entry: e }, compress).await?
                }
                EntryKind::Symlink => {
                    write_message(&mut *w, &Message::MkSymlink { entry: e }, compress).await?
                }
                EntryKind::File => unreachable!(),
            }
        }

        // Split files: delta candidates (remote has different content of
        // tractable size) vs full-push candidates.
        let (delta_files, files): (Vec<Entry>, Vec<Entry>) = all_files.into_iter().partition(|e| {
            if e.size < DELTA_MIN_SIZE || e.size > DELTA_MAX_SIZE {
                return false;
            }
            match remote_hash_by_send.get(&e.path) {
                // Remote has it AND content differs → worth a delta.
                Some(remote_hash) => remote_hash != &e.hash,
                None => false,
            }
        });

        let bytes = Arc::new(AtomicU64::new(0));
        let bytes_saved = Arc::new(AtomicU64::new(0));

        // Phase 2a: delta sync, one file at a time.
        for entry in delta_files {
            let base_hash = remote_hash_by_send
                .get(&entry.path)
                .copied()
                .unwrap_or([0u8; 32]);
            // Request signature.
            {
                let mut w = writer_for_send.lock().await;
                write_message(
                    &mut *w,
                    &Message::SignatureRequest {
                        path: entry.path.clone(),
                        base_hash,
                    },
                    compress,
                )
                .await?;
            }
            // Await signature response (recv loop forwards via sig_tx).
            let (path, sig_opt) = sig_rx
                .recv()
                .await
                .ok_or_else(|| anyhow::anyhow!("signature channel closed"))?;
            if path != entry.path {
                anyhow::bail!(
                    "signature response out of order: expected {}, got {}",
                    entry.path.display(),
                    path.display()
                );
            }
            match sig_opt {
                Some(sig_bytes) => {
                    // Compute delta against the version remote has.
                    let full = local_root_for_send.join(&entry.path);
                    let new_content =
                        tokio::task::spawn_blocking(move || std::fs::read(full)).await??;
                    let new_len = new_content.len() as u64;
                    let entry_clone = entry.clone();
                    let delta = tokio::task::spawn_blocking(move || {
                        compute_delta(&sig_bytes, &new_content)
                    })
                    .await??;
                    let delta_len = delta.len() as u64;
                    {
                        let mut w = writer_for_send.lock().await;
                        write_message(
                            &mut *w,
                            &Message::Delta {
                                entry: entry_clone,
                                base_hash,
                                delta,
                            },
                            compress,
                        )
                        .await?;
                    }
                    bytes.fetch_add(delta_len, Ordering::Relaxed);
                    if new_len > delta_len {
                        bytes_saved.fetch_add(new_len - delta_len, Ordering::Relaxed);
                    }
                    tracing::debug!(
                        "delta {}: {} → {} bytes",
                        entry.path.display(),
                        new_len,
                        delta_len
                    );
                }
                None => {
                    // Server couldn't produce a signature (file gone, hash
                    // mismatch, etc.) — fall back to full transfer.
                    tracing::debug!("delta fallback (full send): {}", entry.path.display());
                    let n =
                        send_file(&writer_for_send, &local_root_for_send, &entry, compress).await?;
                    bytes.fetch_add(n, Ordering::Relaxed);
                }
            }
        }

        // Phase 2b: full push, parallel.
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_PUSHES));
        let mut handles = Vec::with_capacity(files.len());
        for e in files {
            let permit = semaphore.clone().acquire_owned().await?;
            let writer = writer_for_send.clone();
            let root = local_root_for_send.clone();
            let bytes = bytes.clone();
            handles.push(tokio::spawn(async move {
                let _p = permit;
                let n = send_file(&writer, &root, &e, compress).await?;
                bytes.fetch_add(n, Ordering::Relaxed);
                Ok::<(), anyhow::Error>(())
            }));
        }
        for h in handles {
            h.await??;
        }

        // Phase 3: pulls.
        // Split into:
        //   delta_pulls — we have an older copy of this file in our size
        //     band; ship its signature so the server can reply with a
        //     `Delta` (mirror of push-delta but client-initiated).
        //   regular    — we don't have the file at all, or it's outside
        //     the delta band; do a plain FileGet.
        let (delta_pulls, regular_pulls): (Vec<PathBuf>, Vec<PathBuf>) = get_plan
            .into_iter()
            .partition(|p| match local_file_by_send.get(p) {
                Some(local) => local.size >= DELTA_MIN_SIZE && local.size <= DELTA_MAX_SIZE,
                None => false,
            });

        for path in delta_pulls {
            let local_entry = local_file_by_send
                .get(&path)
                .expect("partitioned by local_file_by_send membership");
            let base_hash = local_entry.hash;
            let full = local_root_for_send.join(&path);
            // Read + signature off the runtime; both are blocking.
            let sig = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, anyhow::Error> {
                let content = std::fs::read(&full)?;
                Ok(compute_signature(&content))
            })
            .await??;
            let mut w = writer_for_send.lock().await;
            write_message(
                &mut *w,
                &Message::PullDelta {
                    path,
                    base_hash,
                    sig,
                },
                compress,
            )
            .await?;
        }

        for path in regular_pulls {
            let mut w = writer_for_send.lock().await;
            write_message(&mut *w, &Message::FileGet { path }, compress).await?;
        }
        let mut w = writer_for_send.lock().await;
        write_message(&mut *w, &Message::SyncDone, compress).await?;
        let saved = bytes_saved.load(Ordering::Relaxed);
        if saved > 0 {
            tracing::info!("delta sync saved {} bytes on the wire", saved);
        }
        Ok::<u64, anyhow::Error>(bytes.load(Ordering::Relaxed))
    });

    // Receive: apply incoming server responses until peer SyncDone.
    let pending = Pending::default();
    let suppress = Suppression::default();
    let mut bytes_recv: u64 = 0;
    let mut received_files: u64 = 0;
    loop {
        let msg = read_message(&mut reader).await?;
        match msg {
            Message::FileData { entry, content } => {
                bytes_recv += content.len() as u64;
                received_files += 1;
                let mt = entry.mtime;
                let hash = entry.hash;
                let path = entry.path.clone();
                apply_file_data(&local_root, &entry, &content)?;
                suppress.mark_set(path, mt, hash);
            }
            Message::FileStart { entry, .. } => pending.start(&local_root, entry).await?,
            Message::FileChunk { path, data } => {
                bytes_recv += data.len() as u64;
                pending.chunk(&path, &data).await?;
            }
            Message::FileEnd { path } => {
                if let Some(entry) = pending.end(&local_root, &path).await? {
                    received_files += 1;
                    suppress.mark_set(entry.path, entry.mtime, entry.hash);
                }
            }
            Message::MkDir { entry } => {
                let path = entry.path.clone();
                apply_mkdir(&local_root, &entry)?;
                let mt = std::fs::metadata(local_root.join(&path))
                    .ok()
                    .map(|m| {
                        use std::os::unix::fs::MetadataExt;
                        m.mtime() * 1_000_000_000 + m.mtime_nsec() as i64
                    })
                    .unwrap_or(entry.mtime);
                suppress.mark_mtime(path, mt);
            }
            Message::MkSymlink { entry } => {
                let path = entry.path.clone();
                apply_symlink(&local_root, &entry)?;
                let mt = std::fs::symlink_metadata(local_root.join(&path))
                    .ok()
                    .map(|m| {
                        use std::os::unix::fs::MetadataExt;
                        m.mtime() * 1_000_000_000 + m.mtime_nsec() as i64
                    })
                    .unwrap_or(entry.mtime);
                suppress.mark_mtime(path, mt);
            }
            Message::Delete { path } => {
                apply_delete(&local_root, &path)?;
                suppress.mark_deleted(path);
            }
            Message::Rename { from, to } => {
                apply_rename(&local_root, &from, &to)?;
                suppress.mark_deleted(from);
                let mt = std::fs::symlink_metadata(local_root.join(&to))
                    .ok()
                    .map(|m| {
                        use std::os::unix::fs::MetadataExt;
                        m.mtime() * 1_000_000_000 + m.mtime_nsec() as i64
                    })
                    .unwrap_or(0);
                suppress.mark_mtime(to, mt);
            }
            Message::Signature { path, sig } => {
                // Forward to send_task, which has a matching SignatureRequest
                // waiting on the other end of this channel.
                if sig_tx.send((path, sig)).is_err() {
                    tracing::debug!("signature delivered after send_task ended");
                }
            }
            Message::Delta {
                entry,
                base_hash,
                delta,
            } => {
                // Response to one of our PullDelta requests. Patch our local
                // (older) copy in place after both base- and result-hash
                // verification (done inside apply_delta_to_file).
                bytes_recv += delta.len() as u64;
                received_files += 1;
                let path = entry.path.clone();
                let mt = entry.mtime;
                let hash = entry.hash;
                apply_delta_to_file(&local_root, &entry, base_hash, &delta)?;
                suppress.mark_set(path, mt, hash);
            }
            Message::Touch { path, mtime, mode } => {
                // Server replied with metadata-only update (no content
                // changed). Set mode + mtime if the file exists locally.
                let full = local_root.join(&path);
                if std::fs::symlink_metadata(&full).is_ok() {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&full, std::fs::Permissions::from_mode(mode));
                    let ft = filetime::FileTime::from_unix_time(
                        mtime.div_euclid(1_000_000_000),
                        mtime.rem_euclid(1_000_000_000) as u32,
                    );
                    let _ = filetime::set_file_mtime(&full, ft);
                    suppress.mark_mtime(path, mtime);
                }
            }
            Message::SyncDone => break,
            Message::Error(e) => anyhow::bail!("remote: {e}"),
            Message::Bye => return Ok(()),
            _ => tracing::debug!("ignored msg in init-sync recv"),
        }
    }

    let bytes_sent = send_task.await??;

    crate::ui::ok(&format!(
        "initial sync: {} sent, {} received in {:.1}s",
        format_size(bytes_sent, BINARY).bright_green(),
        format_size(bytes_recv, BINARY).bright_cyan(),
        started.elapsed().as_secs_f32(),
    ));

    // Persist cache (we may have hashed new files).
    if let Ok(c) = cache.lock() {
        c.save(&local_root);
    }

    let _ = received_files;

    if args.once {
        let mut w = writer.lock().await;
        let _ = write_message(&mut *w, &Message::Bye, compress).await;
        drop(w);
        let _ = child.wait().await;
        return Ok(());
    }

    crate::ui::info("watching for changes — ctrl+c to stop");
    let result = live_loop(
        local_root, reader, writer, args.mode, compress, true, ignores,
    )
    .await;
    let _ = child.wait().await;
    result
}

// ─────────────────────────────────────────────────────────────
// Manifest reception
// ─────────────────────────────────────────────────────────────

async fn receive_manifest<R>(reader: &mut R) -> Result<Vec<Entry>>
where
    R: tokio::io::AsyncReadExt + Unpin,
{
    loop {
        match read_message(reader).await? {
            Message::ManifestBegin => break,
            Message::Error(e) => anyhow::bail!("remote: {e}"),
            m => anyhow::bail!("expected ManifestBegin, got {:?}", m),
        }
    }
    let mut entries = Vec::new();
    loop {
        match read_message(reader).await? {
            Message::ManifestEntry(e) => entries.push(e),
            Message::ManifestEnd => break,
            Message::Error(e) => anyhow::bail!("remote: {e}"),
            m => anyhow::bail!("during manifest: {:?}", m),
        }
    }
    Ok(entries)
}

// ─────────────────────────────────────────────────────────────
// Diff plan
// ─────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct Plan {
    push: Vec<Entry>,
    get: Vec<PathBuf>,
}

impl Plan {
    fn print(&self) {
        let push_files = self
            .push
            .iter()
            .filter(|e| matches!(e.kind, EntryKind::File))
            .count();
        let push_dirs = self
            .push
            .iter()
            .filter(|e| matches!(e.kind, EntryKind::Dir))
            .count();
        let push_links = self
            .push
            .iter()
            .filter(|e| matches!(e.kind, EntryKind::Symlink))
            .count();
        let push_bytes: u64 = self
            .push
            .iter()
            .filter(|e| matches!(e.kind, EntryKind::File))
            .map(|e| e.size)
            .sum();
        crate::ui::info(&format!(
            "plan: push {} files ({}) {} dirs {} links  •  pull {} entries",
            push_files.to_string().bright_green(),
            format_size(push_bytes, BINARY).bright_green(),
            push_dirs,
            push_links,
            self.get.len().to_string().bright_cyan(),
        ));
    }
}

fn build_plan(local: &[Entry], remote: &[Entry], mode: SyncMode) -> Plan {
    let local_map: HashMap<&PathBuf, &Entry> = local.iter().map(|e| (&e.path, e)).collect();
    let remote_map: HashMap<&PathBuf, &Entry> = remote.iter().map(|e| (&e.path, e)).collect();

    let mut all_paths: Vec<&PathBuf> = local_map
        .keys()
        .copied()
        .chain(remote_map.keys().copied())
        .collect();
    all_paths.sort();
    all_paths.dedup();

    let mut push: Vec<Entry> = Vec::new();
    let mut get: Vec<PathBuf> = Vec::new();

    for p in all_paths {
        let l = local_map.get(p).copied();
        let r = remote_map.get(p).copied();
        match (l, r) {
            (Some(l), None) => {
                if matches!(mode, SyncMode::Push | SyncMode::Both) {
                    push.push(l.clone());
                }
            }
            (None, Some(r)) => {
                if matches!(mode, SyncMode::Pull | SyncMode::Both) {
                    get.push(r.path.clone());
                }
            }
            (Some(l), Some(r)) => {
                if l.same_content(r) {
                    continue;
                }
                let local_wins = match mode {
                    SyncMode::Push => true,
                    SyncMode::Pull => false,
                    SyncMode::Both => l.mtime >= r.mtime,
                };
                if local_wins {
                    push.push(l.clone());
                } else {
                    get.push(r.path.clone());
                }
            }
            (None, None) => unreachable!(),
        }
    }

    // Dirs first, then symlinks, then files — guarantees parents exist
    // before children when applied on the receiving side.
    push.sort_by_key(|e| {
        let prio = match e.kind {
            EntryKind::Dir => 0u8,
            EntryKind::Symlink => 1,
            EntryKind::File => 2,
        };
        (prio, e.path.clone())
    });

    Plan { push, get }
}
