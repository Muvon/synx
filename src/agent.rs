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
    apply_delete, apply_file_data, apply_mkdir, apply_rename, apply_symlink, live_loop, send_file,
    Pending,
};
use crate::protocol::{read_message, write_message, EntryKind, Message, PROTOCOL_VERSION};
use crate::walker::{build_entry, ensure_root, walk_manifest};

pub async fn run(path: PathBuf) -> Result<()> {
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
    let pending = Pending::default();
    loop {
        let msg = read_message(&mut reader).await?;
        match msg {
            Message::FileData { entry, content } => apply_file_data(&root, &entry, &content)?,
            Message::FileStart { entry, .. } => pending.start(&root, entry).await?,
            Message::FileChunk { path, data } => pending.chunk(&path, &data).await?,
            Message::FileEnd { path } => {
                let _ = pending.end(&root, &path).await?;
            }
            Message::MkDir { entry } => apply_mkdir(&root, &entry)?,
            Message::MkSymlink { entry } => apply_symlink(&root, &entry)?,
            Message::Delete { path } => apply_delete(&root, &path)?,
            Message::Rename { from, to } => apply_rename(&root, &from, &to)?,
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

    // ── Live mode ──
    let ignores = Arc::new(IgnoreStack::load(&root));
    live_loop(root, reader, writer, mode, compress, false, ignores).await
}
