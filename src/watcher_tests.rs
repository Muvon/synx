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

fn recv_events(rx: &mut mpsc::UnboundedReceiver<Vec<FsEvent>>, secs: f64) -> Vec<FsEvent> {
    let mut got = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs_f64(secs);
    loop {
        let now = std::time::Instant::now();
        if now >= deadline {
            break;
        }
        match rx.try_recv() {
            Ok(batch) => got.extend(batch),
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    got
}

// Exploratory: what does the real watcher report for git-style churn?
// Run with `cargo test exploratory -- --nocapture --ignored`.
#[test]
#[ignore]
fn exploratory_git_style_churn() {
    let dir = TestDir::new();
    fs::write(dir.0.join("f.txt"), b"v1").unwrap();
    let root = dir.0.canonicalize().unwrap();
    let ignores = Arc::new(OnceLock::new());
    let mut handle = spawn(root.clone(), Suppression::default(), ignores).unwrap();
    // Let the initial-create events drain.
    let _ = recv_events(&mut handle.events, 1.0);

    println!("── unlink + create + write (git checkout_entry style)");
    fs::remove_file(root.join("f.txt")).unwrap();
    fs::write(root.join("f.txt"), b"v2").unwrap();
    println!("{:?}", recv_events(&mut handle.events, 1.0));

    println!("── in-place write (truncate)");
    fs::write(root.join("f.txt"), b"v3").unwrap();
    println!("{:?}", recv_events(&mut handle.events, 1.0));

    println!("── rapid double rewrite (rebase replay style)");
    fs::remove_file(root.join("f.txt")).unwrap();
    fs::write(root.join("f.txt"), b"a").unwrap();
    fs::remove_file(root.join("f.txt")).unwrap();
    fs::write(root.join("f.txt"), b"b").unwrap();
    println!("{:?}", recv_events(&mut handle.events, 1.0));

    println!("── delete only");
    fs::remove_file(root.join("f.txt")).unwrap();
    println!("{:?}", recv_events(&mut handle.events, 1.0));

    println!("── recreate after delete");
    fs::write(root.join("f.txt"), b"back").unwrap();
    println!("{:?}", recv_events(&mut handle.events, 1.0));

    drop(handle);
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

#[test]
fn isolates_a_permission_denied_subtree_and_watches_readable_siblings() {
    let root = TestDir::new();
    let readable_a = root.0.join("a");
    let affected = root.0.join("affected");
    let blocked = affected.join("blocked");
    let readable_b = affected.join("readable-b");
    let readable_c = root.0.join("c");
    for dir in [&readable_a, &blocked, &readable_b, &readable_c] {
        fs::create_dir_all(dir).unwrap();
    }

    let mut watched = Vec::new();
    let mut fake_watch = |path: &Path, mode: RecursiveMode| {
        assert_eq!(mode, RecursiveMode::Recursive);
        if blocked.starts_with(path) {
            Err(
                notify::Error::io(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
                    .add_path(blocked.clone()),
            )
        } else {
            watched.push(path.to_path_buf());
            Ok(())
        }
    };

    watch_subtree_tolerant(&root.0, true, &mut fake_watch, &mut HashSet::new()).unwrap();

    assert!(watched.contains(&readable_a));
    assert!(watched.contains(&readable_b));
    assert!(watched.contains(&readable_c));
    assert!(!watched.contains(&blocked));
}

#[test]
fn does_not_hide_fatal_watcher_errors() {
    let root = TestDir::new();
    let mut fake_watch = |_path: &Path, _mode: RecursiveMode| {
        Err(notify::Error::new(notify::ErrorKind::MaxFilesWatch))
    };

    let error =
        watch_subtree_tolerant(&root.0, true, &mut fake_watch, &mut HashSet::new()).unwrap_err();
    assert!(matches!(error.kind, notify::ErrorKind::MaxFilesWatch));
}

fn id_of(path: &Path) -> FileId {
    file_id(&fs::symlink_metadata(path).unwrap())
}

fn cached(cache: &IdCache, path: &Path) -> Option<FileId> {
    cache.cached_file_id(path).map(|id| *id.as_ref())
}

#[test]
fn id_cache_records_walks_after_seed_and_honors_ignores() {
    let root = TestDir::new();
    fs::write(root.0.join(".gitignore"), "/ignored\n/build/\n*.log\n").unwrap();
    let ignores = Arc::new(OnceLock::new());
    assert!(ignores
        .set(Arc::new(IgnoreStack::from_manifest(&root.0, &[])))
        .is_ok());
    let mut cache = IdCache::new(ignores);

    fs::create_dir_all(root.0.join("dir/sub")).unwrap();
    fs::write(root.0.join("dir/file"), b"x").unwrap();
    fs::write(root.0.join("dir/sub/deep"), b"x").unwrap();
    fs::write(root.0.join("dir/noise.log"), b"x").unwrap();
    fs::write(root.0.join("dir.txt"), b"x").unwrap();
    fs::write(root.0.join("ignored"), b"x").unwrap();
    fs::create_dir(root.0.join("build")).unwrap();
    fs::write(root.0.join(".synx-tmp-1"), b"x").unwrap();

    // Before the seed a recursive add records only the directory itself —
    // the root registration must not trigger a walk of its own.
    cache.add_path(&root.0.join("dir"), RecursiveMode::Recursive);
    assert_eq!(
        cached(&cache, &root.0.join("dir")),
        Some(id_of(&root.0.join("dir")))
    );
    assert!(cached(&cache, &root.0.join("dir/file")).is_none());

    cache.seed([(root.0.join("dir.txt"), id_of(&root.0.join("dir.txt")))]);
    assert_eq!(
        cached(&cache, &root.0.join("dir.txt")),
        Some(id_of(&root.0.join("dir.txt")))
    );

    // After the seed a new directory is walked, minus ignored entries that
    // only the root ignore file knows about.
    cache.add_path(&root.0.join("dir"), RecursiveMode::Recursive);
    assert_eq!(
        cached(&cache, &root.0.join("dir/sub/deep")),
        Some(id_of(&root.0.join("dir/sub/deep")))
    );
    assert!(cached(&cache, &root.0.join("dir/file")).is_some());
    assert!(cached(&cache, &root.0.join("dir/noise.log")).is_none());

    // Ignored files, dir-only ignored directories, our own tmp files and
    // missing paths are never recorded.
    for rel in ["ignored", "build", ".synx-tmp-1", "missing"] {
        cache.add_path(&root.0.join(rel), RecursiveMode::Recursive);
        assert!(cached(&cache, &root.0.join(rel)).is_none(), "{rel}");
    }

    // A non-recursive add of a plain file records it.
    fs::write(root.0.join("late"), b"x").unwrap();
    cache.add_path(&root.0.join("late"), RecursiveMode::NonRecursive);
    assert_eq!(
        cached(&cache, &root.0.join("late")),
        Some(id_of(&root.0.join("late")))
    );

    // Removing a directory forgets its descendants but not a sibling that
    // merely shares the name as a prefix.
    cache.remove_path(&root.0.join("dir"));
    for rel in ["dir", "dir/file", "dir/sub", "dir/sub/deep"] {
        assert!(cached(&cache, &root.0.join(rel)).is_none(), "{rel}");
    }
    assert!(cached(&cache, &root.0.join("dir.txt")).is_some());
    assert!(cached(&cache, &root.0.join("late")).is_some());

    // A backend rescan (queue overflow) re-walks the root, ignore-aware.
    cache.rescan(&[(root.0.clone(), RecursiveMode::Recursive)]);
    assert!(cached(&cache, &root.0.join("dir/sub/deep")).is_some());
    assert!(cached(&cache, &root.0.join("ignored")).is_none());
    assert!(cached(&cache, &root.0.join("build")).is_none());
}

#[test]
fn live_watcher_pairs_a_rename_via_the_seeded_id_cache() {
    let dir = TestDir::new();
    let root = dir.0.canonicalize().unwrap();
    fs::write(root.join("old"), b"v1").unwrap();
    fs::create_dir(root.join("olddir")).unwrap();
    fs::write(root.join("olddir/child"), b"c").unwrap();
    let ignores = Arc::new(OnceLock::new());
    assert!(ignores
        .set(Arc::new(IgnoreStack::from_manifest(&root, &[])))
        .is_ok());
    let mut handle = spawn(root.clone(), Suppression::default(), ignores).unwrap();
    handle.ids.seed(
        ["old", "olddir", "olddir/child"].map(|rel| (root.join(rel), id_of(&root.join(rel)))),
    );
    // Let any startup noise drain, then start the activity clock fresh.
    let _ = recv_events(&mut handle.events, 0.5);
    handle.activity.store(false, Ordering::Relaxed);

    let wait_for_rename = |events: &mut mpsc::UnboundedReceiver<Vec<FsEvent>>, from, to| {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut got = Vec::new();
        while std::time::Instant::now() < deadline {
            got.extend(recv_events(events, 0.2));
            if got.iter().any(|e| matches!(e, FsEvent::Renamed { .. })) {
                break;
            }
        }
        assert!(
            got.iter().any(|e| matches!(
                e,
                FsEvent::Renamed { from: f, to: t } if f == Path::new(from) && t == Path::new(to)
            )),
            "expected {from} → {to}, got {got:?}"
        );
    };

    fs::rename(root.join("old"), root.join("new")).unwrap();
    wait_for_rename(&mut handle.events, "old", "new");
    assert!(handle.activity.load(Ordering::Relaxed));
    assert!(cached(&handle.ids, &root.join("old")).is_none());
    assert_eq!(
        cached(&handle.ids, &root.join("new")),
        Some(id_of(&root.join("new")))
    );

    // A directory move: FSEvents reports only the directory, so pairing is
    // the only way the peer learns it's a move and keeps the children.
    fs::rename(root.join("olddir"), root.join("newdir")).unwrap();
    wait_for_rename(&mut handle.events, "olddir", "newdir");
    assert!(cached(&handle.ids, &root.join("olddir")).is_none());
    assert_eq!(
        cached(&handle.ids, &root.join("newdir/child")),
        Some(id_of(&root.join("newdir/child")))
    );
    drop(handle);
}

#[test]
fn id_cache_resolves_a_collapsed_rename_by_id() {
    let root = TestDir::new();
    fs::write(root.0.join("old"), b"x").unwrap();
    fs::write(root.0.join("other"), b"y").unwrap();
    let mut cache = IdCache::default();
    cache.seed(["old", "other"].map(|rel| (root.0.join(rel), id_of(&root.0.join(rel)))));
    let create = |name: &str| event(EventKind::Create(CreateKind::File), &[root.0.join(name)]);

    // The backend removes the old path (rename-from) and then reports the
    // new path as a plain create.
    fs::rename(root.0.join("old"), root.0.join("new")).unwrap();
    cache.remove_path(&root.0.join("old"));
    let paired = cache.resolve_rename(&create("new")).expect("paired");
    assert!(matches!(
        paired.kind,
        EventKind::Modify(ModifyKind::Name(RenameMode::Both))
    ));
    assert_eq!(paired.paths, vec![root.0.join("old"), root.0.join("new")]);
    // Consumed: the same create again is just a create.
    assert!(cache.resolve_rename(&create("new")).is_none());

    // Unrelated creates, other kinds and missing paths never pair.
    fs::write(root.0.join("fresh"), b"z").unwrap();
    assert!(cache.resolve_rename(&create("fresh")).is_none());
    assert!(cache.resolve_rename(&create("missing")).is_none());
    cache.remove_path(&root.0.join("other"));
    assert!(cache
        .resolve_rename(&event(
            EventKind::Modify(ModifyKind::Any),
            &[root.0.join("other")]
        ))
        .is_none());
    // A hard link shares the id, but the source still exists: not a move.
    fs::hard_link(root.0.join("other"), root.0.join("link")).unwrap();
    assert!(cache.resolve_rename(&create("link")).is_none());
}
