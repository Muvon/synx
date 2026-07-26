use anyhow::Result;
use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
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
                    suppress.mark_deleted(p.clone());
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
    debouncer.watch(&root, RecursiveMode::Recursive)?;

    Ok(WatcherHandle {
        events: rx,
        keepalive: debouncer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDir(PathBuf);
    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

    impl TestDir {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "synx-watcher-test-{}-{nonce}-{}",
                std::process::id(),
                NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn event(kind: EventKind, paths: &[PathBuf]) -> notify::Event {
        paths
            .iter()
            .cloned()
            .fold(notify::Event::new(kind), notify::Event::add_path)
    }

    #[test]
    fn normalizes_basic_events_and_filters_unsafe_noise() {
        let root = TestDir::new();
        fs::write(root.0.join(".gitignore"), "/ignored\n").unwrap();
        fs::write(root.0.join("file"), b"x").unwrap();
        let ignores = IgnoreStack::from_manifest(&root.0, &[]);
        let suppress = Suppression::default();
        let mut out = Vec::new();

        normalize_event(
            &root.0,
            &suppress,
            Some(&ignores),
            &event(EventKind::Create(CreateKind::File), &[root.0.join("new")]),
            &mut out,
        );
        normalize_event(
            &root.0,
            &suppress,
            Some(&ignores),
            &event(EventKind::Modify(ModifyKind::Any), &[root.0.join("file")]),
            &mut out,
        );
        normalize_event(
            &root.0,
            &suppress,
            Some(&ignores),
            &event(EventKind::Remove(RemoveKind::File), &[root.0.join("gone")]),
            &mut out,
        );

        assert_eq!(
            out,
            vec![
                FsEvent::Created(PathBuf::from("new")),
                FsEvent::Modified(PathBuf::from("file")),
                FsEvent::Removed(PathBuf::from("gone")),
            ]
        );
        assert!(suppress.is_recently_deleted(Path::new("gone")));

        let before = out.len();
        for path in [
            root.0.join("ignored"),
            root.0.join(".synx-tmp-123"),
            root.0.clone(),
            root.0.parent().unwrap().join("outside"),
        ] {
            normalize_event(
                &root.0,
                &suppress,
                Some(&ignores),
                &event(EventKind::Create(CreateKind::File), &[path]),
                &mut out,
            );
        }
        normalize_event(
            &root.0,
            &suppress,
            Some(&ignores),
            &event(
                EventKind::Access(notify::event::AccessKind::Any),
                &[root.0.join("file")],
            ),
            &mut out,
        );
        assert_eq!(out.len(), before);
    }

    #[test]
    fn normalizes_renames_across_ignore_boundaries() {
        let root = TestDir::new();
        fs::write(
            root.0.join(".gitignore"),
            "/ignored-to\n/ignored-from\n/ignored-a\n/ignored-b\n",
        )
        .unwrap();
        let ignores = IgnoreStack::from_manifest(&root.0, &[]);
        let suppress = Suppression::default();
        let rename = EventKind::Modify(ModifyKind::Name(RenameMode::Both));
        let mut out = Vec::new();

        for (from, to) in [
            ("old", "new"),
            ("visible", "ignored-to"),
            ("ignored-from", "created"),
            ("ignored-a", "ignored-b"),
        ] {
            normalize_event(
                &root.0,
                &suppress,
                Some(&ignores),
                &event(rename, &[root.0.join(from), root.0.join(to)]),
                &mut out,
            );
        }

        assert_eq!(
            out,
            vec![
                FsEvent::Renamed {
                    from: PathBuf::from("old"),
                    to: PathBuf::from("new"),
                },
                FsEvent::Removed(PathBuf::from("visible")),
                FsEvent::Created(PathBuf::from("created")),
            ]
        );
        assert!(suppress.is_recently_deleted(Path::new("old")));
        assert!(suppress.is_recently_deleted(Path::new("visible")));

        let before = out.len();
        normalize_event(
            &root.0,
            &suppress,
            Some(&ignores),
            &event(rename, &[root.0.join(".synx-tmp-a"), root.0.join("final")]),
            &mut out,
        );
        normalize_event(
            &root.0,
            &suppress,
            Some(&ignores),
            &event(rename, &[root.0.join("only-one")]),
            &mut out,
        );
        assert_eq!(out.len(), before);
    }

    #[test]
    fn starts_a_recursive_watcher() {
        let root = TestDir::new();
        let ignores = Arc::new(OnceLock::new());
        assert!(ignores
            .set(Arc::new(IgnoreStack::from_manifest(&root.0, &[])))
            .is_ok());
        let handle = spawn(root.0.clone(), Suppression::default(), ignores).unwrap();
        drop(handle);
    }
}
