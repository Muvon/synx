//! Code shared between client and agent: filesystem mutations from
//! protocol messages, chunked transfer helpers, and the bidirectional
//! live-mode event loop.

use anyhow::{Context, Result};
use humansize::{format_size, BINARY};
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
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

const SUPPRESS_TTL: Duration = Duration::from_secs(3);

// ─────────────────────────────────────────────────────────────
// Apply: deterministic, atomic filesystem mutations.
// ─────────────────────────────────────────────────────────────

pub fn apply_file_data(root: &Path, entry: &Entry, content: &[u8]) -> Result<()> {
    let full = root.join(&entry.path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create parent {}", parent.display()))?;
    }
    let tmp = tmp_sibling(&full);
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

fn tmp_sibling(final_path: &Path) -> PathBuf {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name = final_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "tmp".to_string());
    let tmp_name = format!(".synx-tmp.{}.{}.{}", name, std::process::id(), nanos);
    final_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(tmp_name)
}

fn finalize_path(tmp: &Path, final_path: &Path, mode: u32, mtime: i64) -> Result<()> {
    let _ = fs::set_permissions(tmp, fs::Permissions::from_mode(mode));
    fs::rename(tmp, final_path)
        .with_context(|| format!("rename {} → {}", tmp.display(), final_path.display()))?;
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
        let tmp = tmp_sibling(&full);
        let file = fs::File::create(&tmp).with_context(|| format!("open tmp {}", tmp.display()))?;
        let path = entry.path.clone();
        self.inner.lock().await.insert(
            path,
            InFlight {
                entry,
                file,
                tmp,
                bytes_written: 0,
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
            s.bytes_written += data.len() as u64;
        }
        Ok(())
    }

    pub async fn end(&self, root: &Path, path: &Path) -> Result<Option<Entry>> {
        let Some(s) = self.inner.lock().await.remove(path) else {
            return Ok(None);
        };
        s.file.sync_all().ok();
        drop(s.file);
        let full = root.join(&s.entry.path);
        finalize_path(&s.tmp, &full, s.entry.mode, s.entry.mtime)?;
        Ok(Some(s.entry))
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
        let content = fs::read(&full).with_context(|| format!("read {}", full.display()))?;
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
        let mut file = fs::File::open(&full).with_context(|| format!("open {}", full.display()))?;
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
// watcher will see it; we silence that echo so the change
// doesn't bounce back to the peer.
// ─────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
pub struct Suppression {
    inner: Arc<Mutex<HashMap<PathBuf, Instant>>>,
}

impl Suppression {
    pub async fn mark(&self, path: PathBuf) {
        self.inner.lock().await.insert(path, Instant::now());
    }
    pub async fn check(&self, path: &Path) -> bool {
        let mut g = self.inner.lock().await;
        let now = Instant::now();
        g.retain(|_, t| now.duration_since(*t) < SUPPRESS_TTL);
        g.contains_key(path)
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

    let watcher::WatcherHandle {
        events: mut event_rx,
        keepalive: _watcher,
    } = watcher::spawn(root.clone())?;

    let suppress = Suppression::default();
    let pending = Pending::default();

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
                        handle_incoming(&root, m, &suppress, &pending, compress, &writer, apply_remote, Some(&ignores)).await?;
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
                    forward_local_events(&root, events, &writer, compress, &suppress).await?;
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
) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    match msg {
        Message::FileData { entry, content } => {
            if !apply_remote { return Ok(()); }
            if ignored(ignores, &entry.path, false) {
                tracing::debug!("ignored (recv FileData): {}", entry.path.display());
                return Ok(());
            }
            let size = content.len();
            apply_file_data(root, &entry, &content)?;
            suppress.mark(entry.path.clone()).await;
            eprintln!(
                "  {} {}  {}",
                "←".bright_cyan(),
                entry.path.display(),
                format_size(size, BINARY).dimmed()
            );
        }
        Message::FileStart { entry, .. } => {
            if !apply_remote { return Ok(()); }
            if ignored(ignores, &entry.path, false) {
                tracing::debug!("ignored (recv FileStart): {}", entry.path.display());
                return Ok(());
            }
            pending.start(root, entry).await?;
        }
        Message::FileChunk { path, data } => {
            if !apply_remote { return Ok(()); }
            if ignored(ignores, &path, false) { return Ok(()); }
            pending.chunk(&path, &data).await?;
        }
        Message::FileEnd { path } => {
            if !apply_remote { return Ok(()); }
            if ignored(ignores, &path, false) { return Ok(()); }
            if let Some(entry) = pending.end(root, &path).await? {
                suppress.mark(entry.path.clone()).await;
                eprintln!(
                    "  {} {}  {}",
                    "←".bright_cyan(),
                    entry.path.display(),
                    format_size(entry.size, BINARY).dimmed()
                );
            }
        }
        Message::MkDir { entry } => {
            if !apply_remote { return Ok(()); }
            if ignored(ignores, &entry.path, true) {
                tracing::debug!("ignored (recv MkDir): {}", entry.path.display());
                return Ok(());
            }
            apply_mkdir(root, &entry)?;
            suppress.mark(entry.path).await;
        }
        Message::MkSymlink { entry } => {
            if !apply_remote { return Ok(()); }
            if ignored(ignores, &entry.path, false) { return Ok(()); }
            apply_symlink(root, &entry)?;
            suppress.mark(entry.path).await;
        }
        Message::Delete { path } => {
            if !apply_remote { return Ok(()); }
            if ignored(ignores, &path, false) && ignored(ignores, &path, true) {
                // Both file-form and dir-form are ignored — nothing to do.
                return Ok(());
            }
            apply_delete(root, &path)?;
            suppress.mark(path.clone()).await;
            eprintln!(
                "  {} {}",
                "←".bright_cyan(),
                format!("delete {}", path.display()).dimmed()
            );
        }
        Message::Rename { from, to } => {
            if !apply_remote { return Ok(()); }
            // If either endpoint is ignored, the rename is a no-op for sync.
            if ignored(ignores, &from, false) || ignored(ignores, &to, false) {
                tracing::debug!(
                    "ignored (recv Rename): {} → {}",
                    from.display(),
                    to.display()
                );
                return Ok(());
            }
            apply_rename(root, &from, &to)?;
            suppress.mark(from).await;
            suppress.mark(to).await;
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

async fn forward_local_events<W>(
    root: &Path,
    events: Vec<FsEvent>,
    writer: &Arc<Mutex<W>>,
    compress: bool,
    suppress: &Suppression,
) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    for ev in events {
        let primary = match &ev {
            FsEvent::Created(p) | FsEvent::Modified(p) | FsEvent::Removed(p) => p.clone(),
            FsEvent::Renamed { to, .. } => to.clone(),
        };
        if suppress.check(&primary).await {
            tracing::trace!("suppressed: {}", primary.display());
            continue;
        }

        match ev {
            FsEvent::Created(p) | FsEvent::Modified(p) => {
                let entry = match build_entry(root, &p, None)? {
                    Some(e) => e,
                    None => continue,
                };
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
                        let size = entry.size;
                        eprintln!(
                            "  {} {}  {}",
                            "→".bright_green(),
                            entry.path.display(),
                            format_size(size, BINARY).dimmed()
                        );
                        send_file(writer, root, &entry, compress).await?;
                    }
                }
            }
            FsEvent::Removed(p) => {
                eprintln!(
                    "  {} {}",
                    "→".bright_green(),
                    format!("delete {}", p.display()).dimmed()
                );
                let mut w = writer.lock().await;
                write_message(&mut *w, &Message::Delete { path: p }, compress).await?;
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
                }
                eprintln!(
                    "  {} {} → {}",
                    "→".bright_green(),
                    from.display(),
                    to.display()
                );
            }
        }
    }
    Ok(())
}
