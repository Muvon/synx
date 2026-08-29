//! Remote-side agent. Invoked over SSH by the client:
//!     synx --agent /remote/path
//!
//! Communicates with the client over stdin (reads) and stdout (writes),
//! using the framed protocol from `protocol.rs`. All logs go to stderr
//! (which SSH forwards to the client's terminal).

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use tokio::io::{
    stdin, stdout, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter,
};
use tokio::sync::Mutex;

use crate::baseline::LiveBaseline;
use crate::cache::HashCache;
use crate::ignores::IgnoreStack;
use crate::paths::resolve_beneath;
use crate::peer::{
    apply_delete, apply_delta_to_file, apply_file_data, apply_mkdir, apply_rename, apply_symlink,
    cleanup_orphan_tmps, compute_delta, compute_signature, forward_local_events, live_loop,
    send_file, GitGate, Pending, Suppression,
};
use crate::protocol::{
    read_message, write_frame, write_message, EntryKind, Message, IO_BUF_SIZE, PROTOCOL_VERSION,
};
use crate::walker::{build_entry, ensure_root, walk_manifest};
use crate::watcher;

pub async fn run(path: PathBuf) -> Result<()> {
    // Wipe stale tmps left by a previous crashed run on this host.
    cleanup_orphan_tmps();

    let stdin = stdin();
    let stdout = stdout();
    let reader = BufReader::with_capacity(IO_BUF_SIZE, stdin);
    let writer_inner = BufWriter::with_capacity(IO_BUF_SIZE, stdout);
    let writer = Arc::new(Mutex::new(writer_inner));
    run_io(path, reader, writer).await
}

async fn run_io<R, W>(path: PathBuf, mut reader: R, writer: Arc<Mutex<W>>) -> Result<()>
where
    R: AsyncRead + AsyncReadExt + Unpin + Send + 'static,
    W: AsyncWrite + AsyncWriteExt + Unpin + Send,
{
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
    // Start watching before any tree traversal. Ignore matchers are populated
    // from the manifest below; until then events are buffered unfiltered and
    // filtered again before forwarding.
    let ignore_state = Arc::new(OnceLock::new());
    let mut watcher_handle = watcher::spawn(root.clone(), suppress.clone(), ignore_state.clone())?;

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
    let cache = HashCache::load(&root);
    let root_for_walk = root.clone();
    let walk_task = tokio::task::spawn_blocking(move || {
        let mut cache = cache;
        let manifest = walk_manifest(&root_for_walk, &mut cache)?;
        Ok::<_, anyhow::Error>((manifest, cache))
    });

    // Drain client's manifest (we don't need to keep it; the client orchestrates).
    let mut client_count = 0usize;
    match read_message(&mut reader).await? {
        Message::ManifestBegin => {}
        Message::Error(e) => anyhow::bail!("client: {e}"),
        m => anyhow::bail!("expected ManifestBegin, got {:?}", m),
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

    let (local_manifest, mut cache) = walk_task.await??;
    let ignores = Arc::new(IgnoreStack::from_manifest(&root, &local_manifest));
    let _ = ignore_state.set(ignores.clone());
    tracing::debug!("agent manifest: {} entries", local_manifest.len());

    {
        // Stream with write_frame (no per-entry flush) + one flush at the end;
        // the BufWriter auto-flushes when full so this can't deadlock.
        let mut w = writer.lock().await;
        write_frame(&mut *w, &Message::ManifestBegin, compress).await?;
        for e in &local_manifest {
            write_frame(&mut *w, &Message::ManifestEntry(e.clone()), compress).await?;
        }
        write_frame(&mut *w, &Message::ManifestEnd, compress).await?;
        w.flush().await?;
    }

    // ── Initial-sync op loop. Process whatever the client sends until SyncDone. ──
    // `suppress` and `pending` were created above (before the walk) so the
    // watcher already shares the suppression map. Marks recorded here
    // (one per apply op) are matched against watcher events later.
    //
    // Per-op apply errors (e.g. type conflict slipping past the plan check,
    // permission denied) are logged + reported to the client via
    // Message::Error and we CONTINUE — losing the session over one bad
    // file would just trigger the outer reconnect loop and repeat the
    // exact same failure indefinitely.
    let report_err = |path: &std::path::Path, e: &anyhow::Error| -> String {
        let msg = format!("apply {}: {}", path.display(), e);
        tracing::warn!("agent: {}", msg);
        msg
    };
    loop {
        let msg = read_message(&mut reader).await?;
        match msg {
            Message::FileData { entry, content } => {
                let path = entry.path.clone();
                let mtime = entry.mtime;
                let hash = entry.hash;
                if let Err(e) = apply_file_data(&root, &entry, &content) {
                    let s = report_err(&path, &e);
                    let mut w = writer.lock().await;
                    let _ = write_message(&mut *w, &Message::Error(s), compress).await;
                } else {
                    suppress.mark_set(path, mtime, hash);
                }
            }
            Message::FileStart { entry, .. } => {
                let path = entry.path.clone();
                if let Err(e) = pending.start(&root, entry).await {
                    let s = report_err(&path, &e);
                    let mut w = writer.lock().await;
                    let _ = write_message(&mut *w, &Message::Error(s), compress).await;
                }
            }
            Message::FileChunk { path, data } => {
                if let Err(e) = pending.chunk(&path, &data).await {
                    let s = report_err(&path, &e);
                    let mut w = writer.lock().await;
                    let _ = write_message(&mut *w, &Message::Error(s), compress).await;
                }
            }
            Message::FileEnd { path } => match pending.end(&root, &path).await {
                Ok(Some(entry)) => {
                    suppress.mark_set(entry.path, entry.mtime, entry.hash);
                }
                Ok(None) => {}
                Err(e) => {
                    let s = report_err(&path, &e);
                    let mut w = writer.lock().await;
                    let _ = write_message(&mut *w, &Message::Error(s), compress).await;
                }
            },
            Message::MkDir { entry } => {
                let path = entry.path.clone();
                let mtime = entry.mtime;
                if let Err(e) = apply_mkdir(&root, &entry) {
                    let s = report_err(&path, &e);
                    let mut w = writer.lock().await;
                    let _ = write_message(&mut *w, &Message::Error(s), compress).await;
                } else {
                    suppress.mark_mtime(path, mtime);
                }
            }
            Message::MkSymlink { entry } => {
                let path = entry.path.clone();
                let mtime = entry.mtime;
                if let Err(e) = apply_symlink(&root, &entry) {
                    let s = report_err(&path, &e);
                    let mut w = writer.lock().await;
                    let _ = write_message(&mut *w, &Message::Error(s), compress).await;
                } else {
                    suppress.mark_mtime(path, mtime);
                }
            }
            Message::Delete { path } => {
                if let Err(e) = apply_delete(&root, &path) {
                    let s = report_err(&path, &e);
                    let mut w = writer.lock().await;
                    let _ = write_message(&mut *w, &Message::Error(s), compress).await;
                } else {
                    suppress.mark_deleted(path);
                }
            }
            Message::Rename { from, to } => {
                if let Err(e) = apply_rename(&root, &from, &to) {
                    let s = report_err(&to, &e);
                    let mut w = writer.lock().await;
                    let _ = write_message(&mut *w, &Message::Error(s), compress).await;
                } else {
                    suppress.mark_deleted(from);
                    let mt = resolve_beneath(&root, &to)
                        .ok()
                        .and_then(|full| std::fs::symlink_metadata(full).ok())
                        .map(|m| {
                            use std::os::unix::fs::MetadataExt;
                            m.mtime() * 1_000_000_000 + m.mtime_nsec()
                        })
                        .unwrap_or(0);
                    suppress.mark_mtime(to, mt);
                }
            }
            Message::SignatureRequest { path, base_hash } => {
                // Read our local copy, verify hash, compute signature.
                let full = resolve_beneath(&root, &path)?;
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
                if let Err(e) = apply_delta_to_file(&root, &entry, base_hash, &delta) {
                    let s = report_err(&path, &e);
                    let mut w = writer.lock().await;
                    let _ = write_message(&mut *w, &Message::Error(s), compress).await;
                } else {
                    suppress.mark_set(path, mtime, hash);
                }
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
                match build_entry(&root, &path)? {
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
                            let full = resolve_beneath(&root, &entry.path)?;
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
                if let Some(entry) = build_entry(&root, &path)? {
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
    cache.save(&root);

    // Drain watcher events buffered during the walk + manifest exchange +
    // ops loop. Echoes of our own writes filter through `suppress`; real
    // user edits made on the remote during the startup window flow to the
    // client.
    // Agent has no plan/baseline; a disabled one keeps the shared signatures
    // uniform while skipping all persistence. The git gate, however, is real.
    let gate = GitGate::default();
    let live_baseline = LiveBaseline::disabled();
    let mut buffered: Vec<crate::watcher::FsEvent> = Vec::new();
    while let Ok(batch) = watcher_handle.events.try_recv() {
        buffered.extend(batch);
    }
    if !buffered.is_empty() {
        tracing::debug!("agent: draining {} buffered watcher events", buffered.len());
        forward_local_events(
            &root,
            buffered,
            &writer,
            compress,
            &suppress,
            false,
            &ignores,
            &gate,
            &live_baseline,
        )
        .await?;
    }

    // ── Live mode ──
    let ctx = crate::peer::SessionCtx {
        root,
        mode,
        compress,
        is_client: false,
        ignores,
        gate,
        baseline: live_baseline,
    };
    live_loop(ctx, reader, writer, suppress, pending, watcher_handle, None).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Entry, SyncMode};
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("synx-agent-{label}-{}-{nonce}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn entry(path: &str, kind: EntryKind, content: &[u8]) -> Entry {
        Entry {
            path: PathBuf::from(path),
            kind,
            size: content.len() as u64,
            mtime: 1_700_000_000_000_000_000,
            mode: if kind == EntryKind::Dir { 0o755 } else { 0o640 },
            hash: if kind == EntryKind::File {
                *blake3::hash(content).as_bytes()
            } else {
                [0; 32]
            },
            link_target: None,
        }
    }

    async fn encode(messages: impl IntoIterator<Item = Message>) -> Vec<u8> {
        let mut wire = Vec::new();
        for message in messages {
            write_message(&mut wire, &message, false).await.unwrap();
        }
        wire
    }

    #[tokio::test]
    async fn runs_handshake_initial_operations_and_live_shutdown_end_to_end() {
        let root = TestDir::new("session");
        let base = vec![b'a'; 64 * 1024];
        let mut updated = base.clone();
        updated[1000..1010].copy_from_slice(b"0123456789");
        fs::write(root.0.join("base"), &base).unwrap();
        fs::create_dir(root.0.join("request-dir")).unwrap();
        std::os::unix::fs::symlink("base", root.0.join("request-link")).unwrap();
        fs::write(root.0.join("conflict-file"), b"keep").unwrap();
        fs::create_dir(root.0.join("keep-dir")).unwrap();
        fs::write(root.0.join("keep-dir/sentinel"), b"safe").unwrap();
        let signature = compute_signature(&base);
        let delta = compute_delta(&signature, &updated).unwrap();
        let base_hash = *blake3::hash(&base).as_bytes();
        let updated_entry = entry("base", EntryKind::File, &updated);

        let whole = entry("whole", EntryKind::File, b"whole body");
        let chunked = entry("chunked", EntryKind::File, b"chunked body");
        let directory = entry("newdir", EntryKind::Dir, b"");
        let mut link = entry("link", EntryKind::Symlink, b"");
        link.link_target = Some(PathBuf::from("base"));
        let corrupt = entry("corrupt", EntryKind::File, b"expected");
        let conflict_dir = entry("conflict-file", EntryKind::Dir, b"");
        let mut over_dir = entry("keep-dir", EntryKind::Symlink, b"");
        over_dir.link_target = Some(PathBuf::from("base"));
        let bad_chunks = entry("bad-chunks", EntryKind::File, b"expected chunks");
        let blocked = entry("request-link/child", EntryKind::File, b"blocked");

        let input = encode([
            Message::Hello {
                version: PROTOCOL_VERSION,
                root: PathBuf::from("client"),
                mode: SyncMode::Both,
                compress: false,
            },
            Message::ManifestBegin,
            Message::ManifestEnd,
            Message::SignatureRequest {
                path: PathBuf::from("base"),
                base_hash,
            },
            Message::SignatureRequest {
                path: PathBuf::from("base"),
                base_hash: [9; 32],
            },
            Message::SignatureRequest {
                path: PathBuf::from("missing"),
                base_hash: [0; 32],
            },
            Message::Delta {
                entry: updated_entry.clone(),
                base_hash,
                delta,
            },
            Message::Delta {
                entry: updated_entry.clone(),
                base_hash,
                delta: b"invalid delta".to_vec(),
            },
            Message::PullDelta {
                path: PathBuf::from("base"),
                base_hash,
                sig: signature.clone(),
            },
            Message::PullDelta {
                path: PathBuf::from("base"),
                base_hash,
                sig: b"invalid signature".to_vec(),
            },
            Message::PullDelta {
                path: PathBuf::from("request-dir"),
                base_hash: [0; 32],
                sig: Vec::new(),
            },
            Message::PullDelta {
                path: PathBuf::from("request-link"),
                base_hash: [0; 32],
                sig: Vec::new(),
            },
            Message::PullDelta {
                path: PathBuf::from("missing"),
                base_hash: [0; 32],
                sig: Vec::new(),
            },
            Message::FileGet {
                path: PathBuf::from("base"),
            },
            Message::FileGet {
                path: PathBuf::from("missing"),
            },
            Message::FileData {
                entry: whole.clone(),
                content: b"whole body".to_vec(),
            },
            Message::FileStart {
                entry: chunked.clone(),
                total_size: chunked.size,
            },
            Message::FileChunk {
                path: chunked.path.clone(),
                data: b"chunked ".to_vec(),
            },
            Message::FileChunk {
                path: chunked.path.clone(),
                data: b"body".to_vec(),
            },
            Message::FileEnd {
                path: chunked.path.clone(),
            },
            Message::FileStart {
                entry: bad_chunks.clone(),
                total_size: bad_chunks.size,
            },
            Message::FileChunk {
                path: bad_chunks.path.clone(),
                data: b"wrong".to_vec(),
            },
            Message::FileEnd {
                path: bad_chunks.path.clone(),
            },
            Message::FileEnd {
                path: PathBuf::from("unknown"),
            },
            Message::FileStart {
                entry: blocked.clone(),
                total_size: blocked.size,
            },
            Message::MkDir {
                entry: directory.clone(),
            },
            Message::MkDir {
                entry: conflict_dir,
            },
            Message::MkSymlink {
                entry: link.clone(),
            },
            Message::MkSymlink { entry: over_dir },
            Message::Rename {
                from: whole.path.clone(),
                to: PathBuf::from("moved"),
            },
            Message::Delete {
                path: PathBuf::from("moved"),
            },
            Message::Delete {
                path: PathBuf::from("request-link/child"),
            },
            Message::Rename {
                from: PathBuf::from("missing"),
                to: PathBuf::from("still-missing"),
            },
            Message::FileData {
                entry: corrupt,
                content: b"wrong".to_vec(),
            },
            Message::Ping,
            Message::SyncDone,
            Message::Bye,
        ])
        .await;
        let writer = Arc::new(Mutex::new(Vec::new()));
        run_io(root.0.clone(), std::io::Cursor::new(input), writer.clone())
            .await
            .unwrap();

        assert_eq!(fs::read(root.0.join("base")).unwrap(), updated);
        assert_eq!(fs::read(root.0.join("chunked")).unwrap(), b"chunked body");
        assert!(root.0.join("newdir").is_dir());
        assert_eq!(
            fs::read_link(root.0.join("link")).unwrap(),
            Path::new("base")
        );
        assert!(!root.0.join("moved").exists());
        assert!(!root.0.join("corrupt").exists());

        let output = writer.lock().await.clone();
        let mut reader = output.as_slice();
        let mut messages = Vec::new();
        while !reader.is_empty() {
            messages.push(read_message(&mut reader).await.unwrap());
        }
        assert!(matches!(
            messages.first(),
            Some(Message::HelloAck {
                version: PROTOCOL_VERSION,
                root_existed: true
            })
        ));
        assert!(messages
            .iter()
            .any(|message| matches!(message, Message::ManifestBegin)));
        assert!(messages
            .iter()
            .any(|message| matches!(message, Message::ManifestEnd)));
        assert!(messages.iter().any(
            |message| matches!(message, Message::Signature { path, sig: Some(_) } if path == Path::new("base"))
        ));
        assert!(messages.iter().any(
            |message| matches!(message, Message::Signature { path, sig: None } if path == Path::new("base"))
        ));
        assert!(messages.iter().any(
            |message| matches!(message, Message::Delete { path } if path == Path::new("missing"))
        ));
        assert!(messages.iter().any(
            |message| matches!(message, Message::FileData { entry, content } if entry.path == Path::new("base") && content == &updated)
        ));
        assert!(messages
            .iter()
            .any(|message| matches!(message, Message::Error(error) if error.contains("corrupt"))));
        assert!(messages
            .iter()
            .any(|message| matches!(message, Message::SyncDone)));
    }

    #[tokio::test]
    async fn rejects_invalid_handshakes_before_touching_the_root() {
        let parent = TestDir::new("bad-handshake");
        let root = parent.0.join("not-created");
        for message in [
            Message::Ping,
            Message::Hello {
                version: PROTOCOL_VERSION + 1,
                root: PathBuf::from("client"),
                mode: SyncMode::Both,
                compress: false,
            },
        ] {
            let input = encode([message]).await;
            let writer = Arc::new(Mutex::new(Vec::new()));
            assert!(run_io(root.clone(), std::io::Cursor::new(input), writer)
                .await
                .is_err());
            assert!(!root.exists());
        }
    }

    #[tokio::test]
    async fn rejects_invalid_manifest_phases_and_honors_early_bye() {
        let parent = TestDir::new("bad-phases");
        let hello = || Message::Hello {
            version: PROTOCOL_VERSION,
            root: PathBuf::from("client"),
            mode: SyncMode::Both,
            compress: false,
        };
        let cases = [
            (
                "manifest-start",
                vec![hello(), Message::Error("stop".into())],
                true,
            ),
            (
                "manifest-body",
                vec![hello(), Message::ManifestBegin, Message::Ping],
                true,
            ),
            (
                "operation",
                vec![
                    hello(),
                    Message::ManifestBegin,
                    Message::ManifestEnd,
                    Message::Error("stop".into()),
                ],
                true,
            ),
            (
                "early-bye",
                vec![
                    hello(),
                    Message::ManifestBegin,
                    Message::ManifestEnd,
                    Message::Bye,
                ],
                false,
            ),
        ];

        for (name, messages, should_error) in cases {
            let input = encode(messages).await;
            let writer = Arc::new(Mutex::new(Vec::new()));
            let result = run_io(parent.0.join(name), std::io::Cursor::new(input), writer).await;
            assert_eq!(result.is_err(), should_error, "{name}");
        }
    }
}
