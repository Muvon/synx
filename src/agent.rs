//! Remote-side agent. Invoked over SSH by the client:
//!     synx --agent /remote/path
//!
//! Communicates with the client over stdin (reads) and stdout (writes),
//! using the framed protocol from `protocol.rs`. All logs go to stderr
//! (which SSH forwards to the client's terminal).

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::io::{stdin, stdout, BufReader, BufWriter, Stdout};
use tokio::sync::Mutex;

use crate::cache::HashCache;
use crate::ignores::IgnoreStack;
use crate::peer::{
    apply_delete, apply_delta_to_file, apply_file_data, apply_mkdir, apply_rename, apply_symlink,
    cleanup_orphan_tmps, compute_delta, compute_signature, forward_local_events, live_loop,
    send_file, Pending, Suppression,
};
use crate::protocol::{read_message, write_message, EntryKind, Message, PROTOCOL_VERSION};
use crate::walker::{build_entry, ensure_root, walk_manifest};
use crate::watcher;

pub async fn run(path: PathBuf) -> Result<()> {
    // Wipe stale tmps left by a previous crashed run on this host.
    cleanup_orphan_tmps();

    let stdin = stdin();
    let stdout = stdout();
    let mut reader = BufReader::new(stdin);
    let writer_inner = BufWriter::new(stdout);

    // ── Handshake ──
    let hello = read_message(&mut reader).await.context("reading Hello")?;
    let (mode, compress) = match hello {
        Message::Hello {
            version,
            root: _client_root,
            mode,
            compress,
        } => {
            if version != PROTOCOL_VERSION {
                anyhow::bail!("protocol mismatch (remote={PROTOCOL_VERSION}, client={version})");
            }
            (mode, compress)
        }
        other => anyhow::bail!("expected Hello, got {:?}", other),
    };

    let root_existed = path.exists();
    let root = ensure_root(&path)?;

    // Spawn watcher BEFORE the walk so events for files modified during
    // the walk / manifest exchange / init-sync apply window are captured
    // (notify uses "events since now" at registration). Events accumulate
    // in the channel until we drain + replay them after init sync.
    let suppress = Suppression::default();
    let pending = Pending::default();
    let mut watcher_handle = watcher::spawn(root.clone(), suppress.clone())?;

    let writer: Arc<Mutex<BufWriter<Stdout>>> = Arc::new(Mutex::new(writer_inner));
    {
        let mut w = writer.lock().await;
        write_message(
            &mut *w,
            &Message::HelloAck {
                version: PROTOCOL_VERSION,
                root_existed,
            },
            false,
        )
        .await?;
    }

    // ── Walk + send manifest, concurrently receive client manifest ──
    let cache = Arc::new(StdMutex::new(HashCache::load(&root)));
    let root_for_walk = root.clone();
    let cache_for_walk = cache.clone();
    let walk_task =
        tokio::task::spawn_blocking(move || walk_manifest(&root_for_walk, &cache_for_walk));

    // Drain client's manifest (we don't need to keep it; the client orchestrates).
    let mut client_count = 0usize;
    loop {
        match read_message(&mut reader).await? {
            Message::ManifestBegin => break,
            Message::Error(e) => anyhow::bail!("client: {e}"),
            m => anyhow::bail!("expected ManifestBegin, got {:?}", m),
        }
    }
    loop {
        match read_message(&mut reader).await? {
            Message::ManifestEntry(_) => client_count += 1,
            Message::ManifestEnd => break,
            Message::Error(e) => anyhow::bail!("client: {e}"),
            m => anyhow::bail!("during client manifest: {:?}", m),
        }
    }
    tracing::debug!("client manifest: {client_count} entries");

    let local_manifest = walk_task.await??;
    tracing::debug!("agent manifest: {} entries", local_manifest.len());

    {
        let mut w = writer.lock().await;
        write_message(&mut *w, &Message::ManifestBegin, compress).await?;
        for e in &local_manifest {
            write_message(&mut *w, &Message::ManifestEntry(e.clone()), compress).await?;
        }
        write_message(&mut *w, &Message::ManifestEnd, compress).await?;
    }

    // ── Initial-sync op loop. Process whatever the client sends until SyncDone. ──
    // `suppress` and `pending` were created above (before the walk) so the
    // watcher already shares the suppression map. Marks recorded here
    // (one per apply op) are matched against watcher events later.
    loop {
        let msg = read_message(&mut reader).await?;
        match msg {
            Message::FileData { entry, content } => {
                let path = entry.path.clone();
                let mtime = entry.mtime;
                let hash = entry.hash;
                apply_file_data(&root, &entry, &content)?;
                suppress.mark_set(path, mtime, hash);
            }
            Message::FileStart { entry, .. } => pending.start(&root, entry).await?,
            Message::FileChunk { path, data } => pending.chunk(&path, &data).await?,
            Message::FileEnd { path } => {
                if let Some(entry) = pending.end(&root, &path).await? {
                    suppress.mark_set(entry.path, entry.mtime, entry.hash);
                }
            }
            Message::MkDir { entry } => {
                let path = entry.path.clone();
                let mtime = entry.mtime;
                apply_mkdir(&root, &entry)?;
                suppress.mark_mtime(path, mtime);
            }
            Message::MkSymlink { entry } => {
                let path = entry.path.clone();
                let mtime = entry.mtime;
                apply_symlink(&root, &entry)?;
                suppress.mark_mtime(path, mtime);
            }
            Message::Delete { path } => {
                apply_delete(&root, &path)?;
                suppress.mark_deleted(path);
            }
            Message::Rename { from, to } => {
                apply_rename(&root, &from, &to)?;
                suppress.mark_deleted(from);
                let mt = std::fs::symlink_metadata(root.join(&to))
                    .ok()
                    .map(|m| {
                        use std::os::unix::fs::MetadataExt;
                        m.mtime() * 1_000_000_000 + m.mtime_nsec() as i64
                    })
                    .unwrap_or(0);
                suppress.mark_mtime(to, mt);
            }
            Message::SignatureRequest { path, base_hash } => {
                // Read our local copy, verify hash, compute signature.
                let full = root.join(&path);
                let sig_opt = match std::fs::read(&full) {
                    Ok(content) => {
                        let actual = blake3::hash(&content);
                        if actual.as_bytes() == &base_hash {
                            Some(compute_signature(&content))
                        } else {
                            tracing::debug!(
                                "signature: base mismatch for {} (file changed)",
                                path.display()
                            );
                            None
                        }
                    }
                    Err(e) => {
                        tracing::debug!("signature: read {}: {e}", path.display());
                        None
                    }
                };
                let mut w = writer.lock().await;
                write_message(
                    &mut *w,
                    &Message::Signature { path, sig: sig_opt },
                    compress,
                )
                .await?;
            }
            Message::Delta {
                entry,
                base_hash,
                delta,
            } => {
                let path = entry.path.clone();
                let mtime = entry.mtime;
                let hash = entry.hash;
                apply_delta_to_file(&root, &entry, base_hash, &delta)?;
                suppress.mark_set(path, mtime, hash);
            }
            Message::PullDelta {
                path,
                base_hash,
                sig,
            } => {
                // Client wants this file and has shipped us a signature of
                // what it already has. If we can produce a delta smaller
                // than the file itself, do that. Otherwise fall back to a
                // normal send.
                match build_entry(&root, &path, None)? {
                    None => {
                        // We don't have it — tell client to delete its copy.
                        let mut w = writer.lock().await;
                        write_message(&mut *w, &Message::Delete { path }, compress).await?;
                    }
                    Some(entry) => match entry.kind {
                        EntryKind::Dir => {
                            let mut w = writer.lock().await;
                            write_message(&mut *w, &Message::MkDir { entry }, compress).await?;
                        }
                        EntryKind::Symlink => {
                            let mut w = writer.lock().await;
                            write_message(&mut *w, &Message::MkSymlink { entry }, compress).await?;
                        }
                        EntryKind::File => {
                            let full = root.join(&entry.path);
                            let new_content = std::fs::read(&full)?;
                            // 75% threshold: if the delta isn't meaningfully
                            // smaller than the full file, just send the file.
                            let delta_worth_it_max = entry.size.saturating_mul(3) / 4;
                            match compute_delta(&sig, &new_content) {
                                Ok(delta) if (delta.len() as u64) < delta_worth_it_max => {
                                    let mut w = writer.lock().await;
                                    write_message(
                                        &mut *w,
                                        &Message::Delta {
                                            entry,
                                            base_hash,
                                            delta,
                                        },
                                        compress,
                                    )
                                    .await?;
                                }
                                _ => {
                                    send_file(&writer, &root, &entry, compress).await?;
                                }
                            }
                        }
                    },
                }
            }
            Message::FileGet { path } => {
                if let Some(entry) = build_entry(&root, &path, None)? {
                    match entry.kind {
                        EntryKind::File => {
                            send_file(&writer, &root, &entry, compress).await?;
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
                } else {
                    tracing::warn!("FileGet for missing path: {}", path.display());
                }
            }
            Message::SyncDone => break,
            Message::Error(e) => anyhow::bail!("client: {e}"),
            Message::Bye => return Ok(()),
            other => tracing::debug!("ignoring during init sync: {:?}", other),
        }
    }

    // Tell the client we're done responding.
    {
        let mut w = writer.lock().await;
        write_message(&mut *w, &Message::SyncDone, compress).await?;
    }

    // Persist our cache.
    if let Ok(c) = cache.lock() {
        c.save(&root);
    }

    // Drain watcher events buffered during the walk + manifest exchange +
    // ops loop. Echoes of our own writes filter through `suppress`; real
    // user edits made on the remote during the startup window flow to the
    // client.
    let ignores = Arc::new(IgnoreStack::load(&root));
    let mut buffered: Vec<crate::watcher::FsEvent> = Vec::new();
    while let Ok(batch) = watcher_handle.events.try_recv() {
        buffered.extend(batch);
    }
    if !buffered.is_empty() {
        tracing::debug!("agent: draining {} buffered watcher events", buffered.len());
        forward_local_events(&root, buffered, &writer, compress, &suppress, false).await?;
    }

    // ── Live mode ──
    live_loop(
        root,
        reader,
        writer,
        mode,
        compress,
        false,
        ignores,
        suppress,
        pending,
        watcher_handle,
    )
    .await
}
