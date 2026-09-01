use anyhow::Result;
use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::mpsc;

use crate::ignores::IgnoreStack;
use crate::paths::is_internal_temp;
use crate::peer::Suppression;

/// What our higher layers care about, regardless of platform quirks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Removed(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
}

fn normalize_event(
    root: &Path,
    suppress: &Suppression,
    ignores: Option<&IgnoreStack>,
    event: &notify::Event,
    out: &mut Vec<FsEvent>,
) {
    let paths = &event.paths;
    let to_rel =
        |p: &PathBuf| -> Option<PathBuf> { p.strip_prefix(root).ok().map(|r| r.to_path_buf()) };
    use notify::event::{ModifyKind, RenameMode};
    match &event.kind {
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
            if paths.len() < 2 {
                return;
            }
            if paths.iter().any(|path| is_internal_temp(path)) {
                return;
            }
            if let (Some(from), Some(to)) = (to_rel(&paths[0]), to_rel(&paths[1])) {
                if from.as_os_str().is_empty() || to.as_os_str().is_empty() {
                    return;
                }
                let from_ig = ignores.is_some_and(|i| i.is_ignored_abs(&paths[0], false));
                let to_ig = ignores.is_some_and(|i| i.is_ignored_abs(&paths[1], false));
                match (from_ig, to_ig) {
                    (false, false) => {
                        suppress.mark_observed_deleted(from.clone());
                        out.push(FsEvent::Renamed { from, to });
                    }
                    (false, true) => {
                        suppress.mark_observed_deleted(from.clone());
                        out.push(FsEvent::Removed(from));
                    }
                    (true, false) => out.push(FsEvent::Created(to)),
                    (true, true) => {}
                }
            }
        }
        kind => {
            for path in paths {
                if is_internal_temp(path) {
                    continue;
                }
                let Some(rel) = to_rel(path) else { continue };
                if rel.as_os_str().is_empty() {
                    continue;
                }
                let is_dir = matches!(kind, EventKind::Remove(_))
                    .then_some(false)
                    .unwrap_or_else(|| path.is_dir());
                if ignores.is_some_and(|i| i.is_ignored_abs(path, is_dir)) {
                    tracing::debug!("watcher: IGNORED {:?} {}", kind, rel.display());
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
                if let FsEvent::Removed(p) = &fsev {
                    suppress.mark_observed_deleted(p.clone());
                }
                out.push(fsev);
            }
        }
    }
}

pub struct WatcherHandle {
    pub events: mpsc::UnboundedReceiver<Vec<FsEvent>>,
    /// Held to keep the debouncer + watcher threads alive for the
    /// duration of the live session. Dropped on shutdown.
    pub keepalive: Debouncer<notify::RecommendedWatcher, RecommendedCache>,
}

fn recoverable_watch_error(error: &notify::Error) -> bool {
    match &error.kind {
        notify::ErrorKind::Io(error) => matches!(
            error.kind(),
            std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::NotFound
        ),
        notify::ErrorKind::PathNotFound => true,
        _ => false,
    }
}

fn warn_watch_error(error: &notify::Error, warned: &mut HashSet<PathBuf>) {
    if error.paths.is_empty() {
        tracing::warn!("watcher: {error} — skipping inaccessible resource");
        return;
    }
    for path in &error.paths {
        if warned.insert(path.clone()) {
            tracing::warn!(
                "watcher: cannot watch {}: {} — skipping inaccessible resource",
                path.display(),
                error
            );
        }
    }
}

/// Register a recursive native watch while isolating unreadable descendants.
///
/// Linux inotify implements a recursive watch by adding one watch per
/// directory. A single inaccessible descendant makes `watch(root,
/// Recursive)` return `PermissionDenied`, even though it has already covered
/// part of the tree. Retry the immediate children independently: readable
/// subtrees retain native recursive watching, while only the inaccessible
/// branch is omitted.
fn watch_subtree_tolerant<F>(
    dir: &Path,
    is_root: bool,
    watch: &mut F,
    warned: &mut HashSet<PathBuf>,
) -> notify::Result<()>
where
    F: FnMut(&Path, RecursiveMode) -> notify::Result<()>,
{
    match watch(dir, RecursiveMode::Recursive) {
        Ok(()) => return Ok(()),
        Err(error) if recoverable_watch_error(&error) => {
            // If the sync root itself is inaccessible, there is no useful
            // watcher coverage to preserve. Only descendants are skippable.
            if is_root && error.paths.iter().any(|path| path == dir) {
                return Err(error);
            }
            warn_watch_error(&error, warned);
        }
        Err(error) => return Err(error),
    }

    let children = match fs::read_dir(dir) {
        Ok(children) => children,
        Err(error)
            if !is_root
                && matches!(
                    error.kind(),
                    std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::NotFound
                ) =>
        {
            let error = notify::Error::io(error).add_path(dir.to_path_buf());
            warn_watch_error(&error, warned);
            return Ok(());
        }
        Err(error) => return Err(notify::Error::io(error).add_path(dir.to_path_buf())),
    };

    for child in children {
        let child = match child {
            Ok(child) => child,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::NotFound
                ) =>
            {
                warn_watch_error(&notify::Error::io(error), warned);
                continue;
            }
            Err(error) => return Err(notify::Error::io(error)),
        };
        let path = child.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::NotFound
                ) =>
            {
                let error = notify::Error::io(error).add_path(path);
                warn_watch_error(&error, warned);
                continue;
            }
            Err(error) => return Err(notify::Error::io(error).add_path(path)),
        };
        if metadata.is_dir() {
            watch_subtree_tolerant(&path, false, watch, warned)?;
        }
    }

    Ok(())
}

pub fn spawn(
    root: PathBuf,
    suppress: Suppression,
    ignores: Arc<OnceLock<Arc<IgnoreStack>>>,
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
                    normalize_event(
                        &root_cb,
                        &suppress,
                        ignores.get().map(Arc::as_ref),
                        &ev.event,
                        &mut out,
                    );
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
    // A permission-denied descendant must not tear down the whole session.
    let mut warned = HashSet::new();
    watch_subtree_tolerant(
        &root,
        true,
        &mut |path, mode| debouncer.watch(path, mode),
        &mut warned,
    )?;

    Ok(WatcherHandle {
        events: rx,
        keepalive: debouncer,
    })
}

#[cfg(test)]
#[path = "watcher_tests.rs"]
mod tests;
