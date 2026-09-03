use anyhow::Result;
use notify::event::{ModifyKind, RenameMode};
use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::file_id::FileId;
use notify_debouncer_full::{new_debouncer_opt, DebounceEventResult, Debouncer, FileIdCache};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::ops::Bound;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::mpsc;

use crate::ignores::IgnoreStack;
use crate::paths::is_internal_temp;
use crate::peer::Suppression;
use crate::walker::{build_walker, file_id};

/// Editor save storms coalesce inside this window.
const DEBOUNCE: Duration = Duration::from_millis(200);
/// How often the debouncer thread wakes to flush. The default is a quarter
/// of the window — 20 wakeups a second forever; this halves that at the
/// cost of at most 100 ms extra delivery latency.
const DEBOUNCE_TICK: Duration = Duration::from_millis(100);

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

/// Rename pairing for backends without rename cookies (macOS FSEvents): the
/// two halves of a rename arrive as separate events and the old path is
/// already gone, so they can only be matched by a (device, inode) remembered
/// from before.
///
/// The crate's own cache does this by walking the whole tree at watch time —
/// following symlinks, ignoring `.gitignore` — and holding every path under
/// `target/` or `node_modules/` for the session. This one honors the ignore
/// stack, never follows links, and is seeded from the manifest walk, which
/// already has the metadata, instead of a second walk.
#[derive(Clone, Default)]
pub struct IdCache {
    inner: Arc<Mutex<IdCacheInner>>,
    ignores: Arc<OnceLock<Arc<IgnoreStack>>>,
}

#[derive(Default)]
struct IdCacheInner {
    ids: BTreeMap<PathBuf, Known>,
    /// Ids of reported paths the backend just removed, waiting for the
    /// create that the other half of a rename arrives as (see
    /// `resolve_rename`). Consumed on match; cleared once a mass delete
    /// grows it past `MOVED_CAP`.
    moved: HashMap<FileId, PathBuf>,
    /// Until the manifest walk has seeded us, a recursive `add_path` (the
    /// root registration, or a directory created mid-walk) records only the
    /// directory itself; the seed covers the contents.
    seeded: bool,
}

/// A watched path's identity, and whether synx has ever reported the path.
#[derive(Clone, Copy)]
struct Known {
    id: FileId,
    /// Seeded from the manifest, or delivered by the debouncer since. Only
    /// such a path can exist on the peer, so only its disappearance can be
    /// one half of a move. A path created and renamed inside one debounce
    /// window never left the queue under its own name: the debouncer
    /// collapses it into a create at the new path, which is also all the
    /// peer must see — pairing it would ask the peer to rename a path it
    /// never had (git's `index.lock`, every editor's atomic save).
    reported: bool,
}

/// Bound on remembered removed ids; only a mass delete gets near it.
const MOVED_CAP: usize = 10_000;

impl IdCache {
    fn new(ignores: Arc<OnceLock<Arc<IgnoreStack>>>) -> Self {
        Self {
            inner: Arc::default(),
            ignores,
        }
    }

    /// Bulk-load ids the manifest walk already has; recursive `add_path`
    /// calls walk from now on.
    pub fn seed(&self, ids: impl IntoIterator<Item = (PathBuf, FileId)>) {
        if let Ok(mut g) = self.inner.lock() {
            g.ids.extend(
                ids.into_iter()
                    .map(|(path, id)| (path, Known { id, reported: true })),
            );
            g.seeded = true;
        }
    }

    /// Record that the debouncer delivered `event`, so its paths are now
    /// candidates for pairing. A rename covers the moved subtree: the peer
    /// moves it as a whole.
    fn mark_reported(&self, event: &notify::Event) {
        let Ok(mut g) = self.inner.lock() else {
            return;
        };
        let is_move = matches!(
            event.kind,
            EventKind::Modify(ModifyKind::Name(RenameMode::Both))
        );
        // For a move only the destination is still in `ids`.
        for path in event.paths.iter().skip(usize::from(is_move)) {
            if let Some(known) = g.ids.get_mut(path) {
                known.reported = true;
            }
            if is_move {
                g.ids
                    .range_mut::<Path, _>((Bound::Excluded(path.as_path()), Bound::Unbounded))
                    .take_while(|(p, _)| p.starts_with(path))
                    .for_each(|(_, known)| known.reported = true);
            }
        }
    }

    fn ignored(&self, abs: &Path, is_dir: bool) -> bool {
        is_internal_temp(abs)
            || self
                .ignores
                .get()
                .is_some_and(|i| i.is_ignored_abs(abs, is_dir))
    }

    /// Pair a rename the debouncer could not. On macOS the two halves arrive
    /// as separate events, and even a pair it matched by id is collapsed
    /// into a plain create when FSEvents replays the old path's `Created`
    /// flag alongside the rename — which it does for any path it has a
    /// record of. A created path whose id belonged to a reported path that
    /// just vanished is that path, moved.
    pub fn resolve_rename(&self, event: &notify::Event) -> Option<notify::Event> {
        if !matches!(event.kind, EventKind::Create(_)) {
            return None;
        }
        let to = event.paths.first()?;
        let id = file_id(&fs::symlink_metadata(to).ok()?);
        let mut g = self.inner.lock().ok()?;
        // A source still on disk is a hard link, not a move.
        let from = g
            .moved
            .get(&id)
            .filter(|from| *from != to && fs::symlink_metadata(from).is_err())
            .cloned()?;
        g.moved.remove(&id);
        Some(
            notify::Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
                .add_path(from)
                .add_path(to.clone()),
        )
    }
}

impl FileIdCache for IdCache {
    fn cached_file_id(&self, path: &Path) -> Option<impl AsRef<FileId>> {
        self.inner.lock().ok()?.ids.get(path).map(|known| known.id)
    }

    fn add_path(&mut self, path: &Path, recursive_mode: RecursiveMode) {
        if self.ignored(path, false) {
            return;
        }
        let Ok(meta) = fs::symlink_metadata(path) else {
            return;
        };
        if meta.is_dir() && self.ignored(path, true) {
            return;
        }
        let Ok(mut g) = self.inner.lock() else {
            return;
        };
        record(&mut g.ids, path, file_id(&meta));
        if !(meta.is_dir() && recursive_mode == RecursiveMode::Recursive && g.seeded) {
            return;
        }
        // A walker rooted below the sync root misses the ignore files above
        // it, so every entry is also checked against the full stack.
        for dent in build_walker(path).build().flatten() {
            if dent.depth() == 0 {
                continue;
            }
            let Ok(meta) = dent.metadata() else {
                continue;
            };
            if self.ignored(dent.path(), meta.is_dir()) {
                continue;
            }
            record(&mut g.ids, dent.path(), file_id(&meta));
        }
    }

    fn remove_path(&mut self, path: &Path) {
        let Ok(mut g) = self.inner.lock() else {
            return;
        };
        if let Some(known) = g.ids.remove(path) {
            if known.reported {
                if g.moved.len() >= MOVED_CAP {
                    g.moved.clear();
                }
                g.moved.insert(known.id, path.to_path_buf());
            }
        }
        // Descendants sort directly after their parent, so a removed
        // directory costs one range scan rather than a pass over the map.
        // They get no tombstones: no event ever arrives for them.
        let under: Vec<PathBuf> = g
            .ids
            .range::<Path, _>((Bound::Excluded(path), Bound::Unbounded))
            .map(|(p, _)| p)
            .take_while(|p| p.starts_with(path))
            .cloned()
            .collect();
        for p in under {
            g.ids.remove(&p);
        }
    }
}

/// Refresh a path's id, keeping what we know about its delivery: a backend
/// re-add (FSEvents replaying a create, a rescan) is not a new path.
fn record(ids: &mut BTreeMap<PathBuf, Known>, path: &Path, id: FileId) {
    ids.entry(path.to_path_buf())
        .and_modify(|known| known.id = id)
        .or_insert(Known {
            id,
            reported: false,
        });
}

pub struct WatcherHandle {
    pub events: mpsc::UnboundedReceiver<Vec<FsEvent>>,
    /// Set whenever the backend reported anything at all — including events
    /// the ignore filter dropped or a queue-overflow rescan. The live loop's
    /// reconciliation sweep runs only after activity, so an idle tree is
    /// never re-walked.
    pub activity: Arc<AtomicBool>,
    /// Rename-pairing ids; the caller seeds it from the manifest walk.
    pub ids: IdCache,
    /// Held to keep the debouncer + watcher threads alive for the
    /// duration of the live session. Dropped on shutdown.
    pub keepalive: Debouncer<notify::RecommendedWatcher, IdCache>,
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
    let activity = Arc::new(AtomicBool::new(false));
    let activity_cb = Arc::clone(&activity);
    let ids = IdCache::new(Arc::clone(&ignores));
    let ids_cb = ids.clone();

    let mut debouncer = new_debouncer_opt::<_, notify::RecommendedWatcher, IdCache>(
        DEBOUNCE,
        Some(DEBOUNCE_TICK),
        move |result: DebounceEventResult| {
            activity_cb.store(true, Ordering::Relaxed);
            match result {
                Ok(events) => {
                    let mut out: Vec<FsEvent> = Vec::with_capacity(events.len());
                    for ev in events {
                        let raw = match ids_cb.resolve_rename(&ev.event) {
                            Some(paired) => paired,
                            None => ev.event,
                        };
                        ids_cb.mark_reported(&raw);
                        normalize_event(
                            &root_cb,
                            &suppress,
                            ignores.get().map(Arc::as_ref),
                            &raw,
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
            }
        },
        ids.clone(),
        notify::Config::default(),
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
        activity,
        ids,
        keepalive: debouncer,
    })
}

#[cfg(test)]
#[path = "watcher_tests.rs"]
mod tests;
