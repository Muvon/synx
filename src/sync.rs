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

use crate::baseline::Baseline;
use crate::cache::HashCache;
use crate::cli::ClientArgs;
use crate::ignores::IgnoreStack;
use crate::peer::{
    apply_delete, apply_delta_to_file, apply_file_data, apply_mkdir, apply_rename, apply_symlink,
    compute_delta, compute_signature, forward_local_events, git_busy, is_under_git, live_loop,
    send_file, Pending, Suppression,
};
use crate::protocol::{
    read_message, write_message, Entry, EntryKind, Message, SyncMode, PROTOCOL_VERSION,
};
use crate::transport::{parse_remote, spawn_ssh};
use crate::walker::{ensure_root, walk_manifest};
use crate::watcher;

pub async fn run(args: ClientArgs) -> Result<()> {
    let local_root = ensure_root(std::path::Path::new(&args.local))
        .with_context(|| format!("invalid local path: {}", args.local))?;
    let remote = parse_remote(&args.remote)?;
    crate::ui::banner(&local_root, &remote, args.mode);

    // Wipe orphan tmps from any previous crashed run before we start.
    crate::peer::cleanup_orphan_tmps();

    let mut delay = std::time::Duration::from_secs(1);
    let max_delay = std::time::Duration::from_secs(30);
    let mut first_attempt = true;

    loop {
        match run_session(&local_root, &remote, &args, first_attempt).await {
            Ok(()) => return Ok(()),
            Err(e) if is_fatal(&e) => return Err(e),
            Err(e) => {
                crate::ui::warn(&format!(
                    "connection lost: {} — reconnecting in {}s",
                    short_err(&e),
                    delay.as_secs()
                ));
                // Cancel-safe wait so Ctrl-C during the back-off exits cleanly.
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = tokio::signal::ctrl_c() => {
                        crate::ui::info("interrupted");
                        return Ok(());
                    }
                }
                delay = (delay * 2).min(max_delay);
                first_attempt = false;
            }
        }
    }
}

/// Errors that auto-reconnect can't fix — config-level issues. Anything
/// network / connection / peer-side gets retried.
fn is_fatal(e: &anyhow::Error) -> bool {
    let s = e.to_string().to_lowercase();
    s.contains("protocol mismatch")
        || s.contains("invalid local path")
        || s.contains("remote must be ")
}

fn short_err(e: &anyhow::Error) -> String {
    let s = e.to_string();
    s.lines().next().unwrap_or(&s).to_string()
}

/// One full session: spawn ssh, handshake, initial sync, live mode.
/// Returns `Ok(())` on clean exit (Ctrl-C, `--once` done); `Err` on any
/// connection-level failure (the outer loop will reconnect).
async fn run_session(
    local_root: &std::path::Path,
    remote: &crate::transport::Remote,
    args: &ClientArgs,
    first_attempt: bool,
) -> Result<()> {
    let mut child = spawn_ssh(remote, args.ssh_opts.as_deref(), &args.remote_synx)?;
    let stdin = child.stdin.take().context("ssh stdin missing")?;
    let stdout = child.stdout.take().context("ssh stdout missing")?;
    let mut reader = BufReader::new(stdout);
    let writer_inner = BufWriter::new(stdin);
    let compress = !args.no_compress;

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
            if !root_existed && first_attempt {
                crate::ui::warn(&format!("remote path created: {}", remote.path));
            } else if first_attempt {
                crate::ui::ok("connected");
            } else {
                crate::ui::ok("reconnected");
            }
        }
        Message::Error(e) => anyhow::bail!("remote rejected handshake: {e}"),
        m => anyhow::bail!("unexpected handshake reply: {:?}", m),
    }

    run_inner(
        local_root.to_path_buf(),
        args.clone(),
        compress,
        reader,
        writer,
        child,
    )
    .await
}

async fn run_inner(
    local_root: PathBuf,
    args: ClientArgs,
    compress: bool,
    mut reader: BufReader<ChildStdout>,
    writer: Arc<Mutex<BufWriter<ChildStdin>>>,
    mut child: tokio::process::Child,
) -> Result<()> {
    // Spawn the watcher BEFORE the walk so events for files the user
    // modifies during walk / manifest exchange / init-sync apply aren't
    // lost (notify uses "events since now" at registration; events from
    // before are never delivered). Events queue in the unbounded channel
    // until init sync completes, then we drain + replay them with the
    // suppress map populated.
    let suppress = Suppression::default();
    let pending = Pending::default();
    let mut watcher_handle = watcher::spawn(local_root.clone(), suppress.clone())?;

    // ── Local manifest (parallel walk with hash cache) ──
    let cache = Arc::new(StdMutex::new(HashCache::load(&local_root)));
    let started = Instant::now();
    let root_for_walk = local_root.clone();
    let cache_for_walk = cache.clone();
    let mut local_manifest =
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

    // ── Stale-.git/ recovery ──
    // If local has leftover .git/ entries (from old-version synx propagating
    // mid-rebase state, or a crashed git), remote has NO .git/, AND remote
    // looks like a real populated workspace (substantial non-.git content),
    // the user has clearly chosen to wipe .git/. Mirror that here so local
    // doesn't stay stuck with phantom rebase state forever. Skip if local
    // is genuinely mid-operation (git_busy after stale check).
    let local_has_git = local_manifest.iter().any(|e| is_under_git(&e.path));
    let remote_has_git = remote_manifest.iter().any(|e| is_under_git(&e.path));
    let remote_non_git = remote_manifest
        .iter()
        .filter(|e| !is_under_git(&e.path))
        .count();
    if local_has_git && !remote_has_git && remote_non_git >= 5 && !git_busy(&local_root) {
        crate::ui::warn(
            "local has leftover .git/ but remote has none — cleaning local .git/ to match",
        );
        let local_git_path = local_root.join(".git");
        if let Err(e) = std::fs::remove_dir_all(&local_git_path) {
            crate::ui::warn(&format!("failed to clean local .git/: {e}"));
        } else {
            // Strip .git/* entries from the manifest so the diff plan
            // doesn't try to push them back.
            local_manifest.retain(|e| !is_under_git(&e.path));
        }
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
    // Baseline = the converged manifest from our last successful sync. It is
    // the common ancestor that lets the three-way diff distinguish a genuine
    // deletion from a creation on the peer (see build_plan). Empty on first
    // run → conservative pull-back, no deletes propagate that session.
    let baseline = Baseline::load(&local_root);
    if baseline.is_empty() {
        tracing::debug!("no baseline yet — deletions won't propagate until next sync");
    }
    let plan = build_plan(&local_manifest, &remote_manifest, &baseline, args.mode);
    plan.print();

    if args.dry_run {
        let mut w = writer.lock().await;
        let _ = write_message(&mut *w, &Message::Bye, compress).await;
        drop(w);
        let _ = child.wait().await;
        return Ok(());
    }

    // Apply remote-originated deletions locally. These are paths the peer
    // removed whose local copy is still byte-identical to the last baseline,
    // so it's a propagated deletion (not an unsynced local edit). Destructive,
    // hence gated on the baseline match computed in build_plan.
    for path in &plan.del_local {
        match apply_delete(&local_root, path) {
            Ok(()) => {
                suppress.mark_deleted(path.clone());
                eprintln!("  {} × {}", "←".bright_cyan(), path.display());
            }
            Err(e) => tracing::warn!("local delete {}: {}", path.display(), e),
        }
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
    let del_remote_plan = plan.del_remote.clone();
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

        // Phase 0: propagate local deletions to the remote. The agent's
        // init-sync loop applies these (apply_delete + mark_deleted).
        for path in del_remote_plan {
            let mut w = writer_for_send.lock().await;
            write_message(&mut *w, &Message::Delete { path }, compress).await?;
        }

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
    // Per-op apply errors during init sync are non-fatal — bailing here
    // would tear down the session and the reconnect loop would hit the
    // same error forever.
    let mut bytes_recv: u64 = 0;
    let mut received_files: u64 = 0;
    let warn_apply = |path: &std::path::Path, e: &anyhow::Error| {
        tracing::warn!("apply {} failed: {}", path.display(), e);
    };
    loop {
        let msg = read_message(&mut reader).await?;
        match msg {
            Message::FileData { entry, content } => {
                bytes_recv += content.len() as u64;
                let mt = entry.mtime;
                let hash = entry.hash;
                let path = entry.path.clone();
                if let Err(e) = apply_file_data(&local_root, &entry, &content) {
                    warn_apply(&path, &e);
                } else {
                    received_files += 1;
                    suppress.mark_set(path, mt, hash);
                }
            }
            Message::FileStart { entry, .. } => {
                let path = entry.path.clone();
                if let Err(e) = pending.start(&local_root, entry).await {
                    warn_apply(&path, &e);
                }
            }
            Message::FileChunk { path, data } => {
                bytes_recv += data.len() as u64;
                if let Err(e) = pending.chunk(&path, &data).await {
                    warn_apply(&path, &e);
                }
            }
            Message::FileEnd { path } => match pending.end(&local_root, &path).await {
                Ok(Some(entry)) => {
                    received_files += 1;
                    suppress.mark_set(entry.path, entry.mtime, entry.hash);
                }
                Ok(None) => {}
                Err(e) => warn_apply(&path, &e),
            },
            Message::MkDir { entry } => {
                let path = entry.path.clone();
                if let Err(e) = apply_mkdir(&local_root, &entry) {
                    warn_apply(&path, &e);
                } else {
                    let mt = std::fs::metadata(local_root.join(&path))
                        .ok()
                        .map(|m| {
                            use std::os::unix::fs::MetadataExt;
                            m.mtime() * 1_000_000_000 + m.mtime_nsec()
                        })
                        .unwrap_or(entry.mtime);
                    suppress.mark_mtime(path, mt);
                }
            }
            Message::MkSymlink { entry } => {
                let path = entry.path.clone();
                if let Err(e) = apply_symlink(&local_root, &entry) {
                    warn_apply(&path, &e);
                } else {
                    let mt = std::fs::symlink_metadata(local_root.join(&path))
                        .ok()
                        .map(|m| {
                            use std::os::unix::fs::MetadataExt;
                            m.mtime() * 1_000_000_000 + m.mtime_nsec()
                        })
                        .unwrap_or(entry.mtime);
                    suppress.mark_mtime(path, mt);
                }
            }
            Message::Delete { path } => {
                if let Err(e) = apply_delete(&local_root, &path) {
                    warn_apply(&path, &e);
                } else {
                    suppress.mark_deleted(path);
                }
            }
            Message::Rename { from, to } => {
                if let Err(e) = apply_rename(&local_root, &from, &to) {
                    warn_apply(&to, &e);
                } else {
                    suppress.mark_deleted(from);
                    let mt = std::fs::symlink_metadata(local_root.join(&to))
                        .ok()
                        .map(|m| {
                            use std::os::unix::fs::MetadataExt;
                            m.mtime() * 1_000_000_000 + m.mtime_nsec()
                        })
                        .unwrap_or(0);
                    suppress.mark_mtime(to, mt);
                }
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
                let path = entry.path.clone();
                let mt = entry.mtime;
                let hash = entry.hash;
                if let Err(e) = apply_delta_to_file(&local_root, &entry, base_hash, &delta) {
                    warn_apply(&path, &e);
                } else {
                    received_files += 1;
                    suppress.mark_set(path, mt, hash);
                }
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
            // Remote reported a per-op failure (type conflict, perm denied,
            // git busy on its side). Log and continue — bailing here would
            // tear down the session and retry forever.
            Message::Error(e) => tracing::warn!("remote: {e}"),
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

    // Persist the converged manifest as the next session's baseline. After
    // init sync the local tree equals the merged result: start from the local
    // manifest, drop what we just deleted locally, and overwrite pulled paths
    // with the remote's version (we now hold its content). This is what lets
    // the next run tell a genuine deletion from a peer creation.
    {
        let remote_by_path: HashMap<&PathBuf, &Entry> =
            remote_manifest.iter().map(|e| (&e.path, e)).collect();
        let deleted_local: std::collections::HashSet<PathBuf> =
            plan.del_local.iter().cloned().collect();
        let mut converged: HashMap<PathBuf, Entry> = local_manifest
            .iter()
            .filter(|e| !deleted_local.contains(&e.path))
            .map(|e| (e.path.clone(), e.clone()))
            .collect();
        for p in &plan.get {
            if let Some(r) = remote_by_path.get(p) {
                converged.insert(p.clone(), (*r).clone());
            }
        }
        Baseline::from_map(converged).save(&local_root);
    }

    let _ = received_files;

    if args.once {
        let mut w = writer.lock().await;
        let _ = write_message(&mut *w, &Message::Bye, compress).await;
        drop(w);
        let _ = child.wait().await;
        return Ok(());
    }

    // Drain any watcher events that accumulated during the walk + manifest
    // exchange + init-sync apply. With suppress now populated for every
    // file we just wrote, echoes of our own writes filter out and real
    // user edits made during the startup window flow through to the peer.
    let mut buffered: Vec<crate::watcher::FsEvent> = Vec::new();
    while let Ok(batch) = watcher_handle.events.try_recv() {
        buffered.extend(batch);
    }
    if !buffered.is_empty() {
        tracing::debug!("draining {} buffered watcher events", buffered.len());
        forward_local_events(&local_root, buffered, &writer, compress, &suppress, true).await?;
    }

    crate::ui::info("watching for changes — ctrl+c to stop");
    let ctx = crate::peer::SessionCtx {
        root: local_root,
        mode: args.mode,
        compress,
        is_client: true,
        ignores,
    };
    let result = live_loop(ctx, reader, writer, suppress, pending, watcher_handle).await;
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
    match read_message(reader).await? {
        Message::ManifestBegin => {}
        Message::Error(e) => anyhow::bail!("remote: {e}"),
        m => anyhow::bail!("expected ManifestBegin, got {:?}", m),
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
    /// Deletions to propagate to the remote: paths the user removed locally
    /// whose remote copy is still byte-identical to the last synced baseline.
    del_remote: Vec<PathBuf>,
    /// Deletions to apply locally: paths the remote removed whose local copy
    /// is still byte-identical to the last synced baseline.
    del_local: Vec<PathBuf>,
    /// Paths where local and remote disagree on `kind` (e.g. one side has
    /// a file, the other a directory). Skipped from sync because blindly
    /// overwriting would either fail with EISDIR or destroy a directory
    /// tree. User must resolve by hand.
    conflicts: Vec<(PathBuf, EntryKind, EntryKind)>,
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
        let del_note = if self.del_remote.is_empty() && self.del_local.is_empty() {
            String::new()
        } else {
            format!(
                "  •  delete {} remote {} local",
                self.del_remote.len().to_string().bright_red(),
                self.del_local.len().to_string().bright_red(),
            )
        };
        crate::ui::info(&format!(
            "plan: push {} files ({}) {} dirs {} links  •  pull {} entries{}",
            push_files.to_string().bright_green(),
            format_size(push_bytes, BINARY).bright_green(),
            push_dirs,
            push_links,
            self.get.len().to_string().bright_cyan(),
            del_note,
        ));
        if !self.conflicts.is_empty() {
            crate::ui::warn(&format!(
                "{} type conflicts (file/dir/symlink mismatch) — skipped, resolve manually:",
                self.conflicts.len()
            ));
            for (path, lk, rk) in self.conflicts.iter().take(10) {
                eprintln!(
                    "    {}  local={:?}  remote={:?}",
                    path.display().to_string().bright_yellow(),
                    lk,
                    rk
                );
            }
            if self.conflicts.len() > 10 {
                eprintln!("    … and {} more", self.conflicts.len() - 10);
            }
        }
    }
}

/// Three-way diff. `baseline` is the converged manifest from the last
/// successful sync (empty on first run). It is what lets us tell a genuine
/// deletion ("present in baseline, gone here, unchanged there") apart from a
/// creation on the other side ("not in baseline") — the two are identical
/// from the live manifests alone. When the evidence is ambiguous (no
/// baseline) or the surviving side has its own changes (modify-vs-delete),
/// we never propagate the destructive op: we keep the data.
fn build_plan(local: &[Entry], remote: &[Entry], baseline: &Baseline, mode: SyncMode) -> Plan {
    let local_map: HashMap<&PathBuf, &Entry> = local.iter().map(|e| (&e.path, e)).collect();
    let remote_map: HashMap<&PathBuf, &Entry> = remote.iter().map(|e| (&e.path, e)).collect();

    let mut all_paths: Vec<&PathBuf> = local_map
        .keys()
        .copied()
        .chain(remote_map.keys().copied())
        .collect();
    all_paths.sort();
    all_paths.dedup();

    // What this sync direction is allowed to mutate.
    let (allow_push, allow_pull) = match mode {
        SyncMode::Push => (true, false),
        SyncMode::Pull => (false, true),
        SyncMode::Both => (true, true),
    };

    let mut push: Vec<Entry> = Vec::new();
    let mut get: Vec<PathBuf> = Vec::new();
    let mut del_remote: Vec<PathBuf> = Vec::new();
    let mut del_local: Vec<PathBuf> = Vec::new();
    let mut conflicts: Vec<(PathBuf, EntryKind, EntryKind)> = Vec::new();

    for p in all_paths {
        let l = local_map.get(p).copied();
        let r = remote_map.get(p).copied();
        let b = baseline.get(p);
        match (l, r) {
            (Some(l), None) => {
                // Present locally, absent on remote. Either the remote
                // deleted a file we both had, or we created it. It's a real
                // remote deletion only if our copy is unchanged since the
                // baseline; otherwise (we changed it, or no baseline) keep
                // local and re-push.
                let remote_deleted = b.map(|b| l.same_content(b)).unwrap_or(false);
                if remote_deleted && allow_pull {
                    del_local.push(l.path.clone());
                } else if allow_push {
                    push.push(l.clone());
                }
            }
            (None, Some(r)) => {
                // Absent locally, present on remote. Either we deleted a file
                // we both had, or the remote created it. It's a real local
                // deletion only if the remote copy is unchanged since the
                // baseline; otherwise (remote changed it, or no baseline)
                // keep remote and pull.
                let local_deleted = b.map(|b| r.same_content(b)).unwrap_or(false);
                if local_deleted && allow_push {
                    del_remote.push(r.path.clone());
                } else if allow_pull {
                    get.push(r.path.clone());
                }
            }
            (Some(l), Some(r)) => {
                if l.same_content(r) {
                    continue;
                }
                // Type mismatch (file vs dir vs symlink): blindly applying
                // would either fail (EISDIR) or destroy a directory tree.
                // Skip and surface for manual resolution.
                if l.kind != r.kind {
                    conflicts.push((l.path.clone(), l.kind, r.kind));
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

    Plan {
        push,
        get,
        del_remote,
        del_local,
        conflicts,
    }
}
