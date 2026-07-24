use anyhow::Result;
use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::ignores::IgnoreStack;
use crate::peer::Suppression;

/// What our higher layers care about, regardless of platform quirks.
#[derive(Debug, Clone)]
pub enum FsEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Removed(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
}

pub struct WatcherHandle {
    pub events: mpsc::UnboundedReceiver<Vec<FsEvent>>,
    /// Held to keep the debouncer + watcher threads alive for the
    /// duration of the live session. Dropped on shutdown.
    pub keepalive: Debouncer<notify::RecommendedWatcher, RecommendedCache>,
}

pub fn spawn(
    root: PathBuf,
    suppress: Suppression,
    ignores: Arc<IgnoreStack>,
) -> Result<WatcherHandle> {
    let (tx, rx) = mpsc::unbounded_channel::<Vec<FsEvent>>();
    let root_cb = root.clone();

    let mut debouncer = new_debouncer(
        Duration::from_millis(200),
        None,
        move |result: DebounceEventResult| match result {
            Ok(events) => {
                let mut out: Vec<FsEvent> = Vec::with_capacity(events.len());
                for ev in events {
                    let paths = &ev.event.paths;
                    let to_rel = |p: &PathBuf| -> Option<PathBuf> {
                        p.strip_prefix(&root_cb).ok().map(|r| r.to_path_buf())
                    };
                    use notify::event::{ModifyKind, RenameMode};
                    match &ev.event.kind {
                        EventKind::Modify(ModifyKind::Name(RenameMode::Both))
                            if paths.len() >= 2 =>
                        {
                            if let (Some(from), Some(to)) = (to_rel(&paths[0]), to_rel(&paths[1])) {
                                if from.as_os_str().is_empty() || to.as_os_str().is_empty() {
                                    continue;
                                }
                                let from_ig = ignores.is_ignored_abs(&paths[0], false);
                                let to_ig = ignores.is_ignored_abs(&paths[1], false);
                                match (from_ig, to_ig) {
                                    (false, false) => {
                                        // A rename means the source is gone from
                                        // its old location, conceptually deleted.
                                        suppress.mark_deleted(from.clone());
                                        out.push(FsEvent::Renamed { from, to });
                                    }
                                    (false, true) => {
                                        suppress.mark_deleted(from.clone());
                                        out.push(FsEvent::Removed(from));
                                    }
                                    (true, false) => out.push(FsEvent::Created(to)),
                                    (true, true) => {}
                                }
                            }
                        }
                        kind => {
                            for path in paths {
                                let Some(rel) = to_rel(path) else { continue };
                                if rel.as_os_str().is_empty() {
                                    continue;
                                }
                                // For Remove events the inode is already gone, so
                                // is_dir() may lie. Pass `false` and let the
                                // gitignore rules decide; receivers tolerate either.
                                let is_dir = matches!(kind, EventKind::Remove(_))
                                    .then_some(false)
                                    .unwrap_or_else(|| path.is_dir());
                                if ignores.is_ignored_abs(path, is_dir) {
                                    tracing::debug!(
                                        "watcher: IGNORED {:?} {}",
                                        kind,
                                        rel.display()
                                    );
                                    continue;
                                }
                                let fsev = match kind {
                                    EventKind::Create(_) => FsEvent::Created(rel),
                                    EventKind::Modify(_) => FsEvent::Modified(rel),
                                    EventKind::Remove(_) => FsEvent::Removed(rel),
                                    other => {
                                        tracing::debug!(
                                            "watcher: SKIPPED kind={:?} path={}",
                                            other,
                                            path.display()
                                        );
                                        continue;
                                    }
                                };
                                tracing::debug!("watcher: emit {:?}", fsev);
                                // Eagerly tell the suppression map about
                                // deletions. This closes the window where a
                                // stale `FileData` echo from the peer could
                                // arrive AFTER the user's `rm` but BEFORE our
                                // debouncer fires the Removed event — without
                                // this, the receiver-side stale-create guard
                                // hasn't been armed yet, and the file would
                                // get resurrected.
                                if let FsEvent::Removed(p) = &fsev {
                                    suppress.mark_deleted(p.clone());
                                }
                                out.push(fsev);
                            }
                        }
                    }
                }
                if !out.is_empty() {
                    let _ = tx.send(out);
                }
            }
            Err(errs) => {
                for e in errs {
                    tracing::warn!("watcher: {e}");
                }
            }
        },
    )?;

    // 0.7: direct .watch() on the Debouncer instead of .watcher().watch().
    debouncer.watch(&root, RecursiveMode::Recursive)?;

    Ok(WatcherHandle {
        events: rx,
        keepalive: debouncer,
    })
}
