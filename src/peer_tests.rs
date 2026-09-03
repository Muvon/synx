use super::*;
use crate::ignores::IgnoreStack;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct TestDir(PathBuf);

impl TestDir {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("synx-{label}-test-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn file_entry(path: &str, content: &[u8]) -> Entry {
    Entry {
        path: PathBuf::from(path),
        kind: EntryKind::File,
        size: content.len() as u64,
        mtime: 1_700_000_000_123_456_789,
        mode: 0o640,
        hash: *blake3::hash(content).as_bytes(),
        link_target: None,
    }
}

fn kind_entry(path: &str, kind: EntryKind) -> Entry {
    Entry {
        path: PathBuf::from(path),
        kind,
        size: 0,
        mtime: 1_700_000_000_123_456_789,
        mode: 0o755,
        hash: [0; 32],
        link_target: None,
    }
}

fn session_ctx(root: &Path) -> SessionCtx {
    SessionCtx {
        root: root.to_path_buf(),
        mode: SyncMode::Both,
        compress: false,
        is_client: false,
        ignores: Arc::new(IgnoreStack::from_manifest(root, &[])),
        gate: GitGate::default(),
        baseline: LiveBaseline::disabled(),
    }
}

#[test]
fn identifies_precompressed_extensions_case_insensitively() {
    assert!(is_precompressed(Path::new("artifacts/release.TAR.GZ")));
    assert!(is_precompressed(Path::new("photo.JpG")));
    assert!(!is_precompressed(Path::new("src/data.json")));
    assert!(!is_precompressed(Path::new("archive.tar")));
}

#[test]
fn equality_fast_paths_require_matching_type_size_content_and_link_target() {
    let root = TestDir::new("equal");
    fs::write(root.path().join("file"), b"content").unwrap();
    fs::create_dir(root.path().join("dir")).unwrap();
    std::os::unix::fs::symlink("file", root.path().join("link")).unwrap();

    let mut file = file_entry("file", b"content");
    assert!(is_already_equal(root.path(), &file));
    file.size += 1;
    assert!(!is_already_equal(root.path(), &file));
    file.size -= 1;
    file.hash = [0; 32];
    file.mtime = 0;
    assert!(!is_already_equal(root.path(), &file));

    assert!(!is_already_equal(
        root.path(),
        &file_entry("missing", b"content")
    ));
    assert!(!is_already_equal(
        root.path(),
        &kind_entry("file", EntryKind::Dir)
    ));
    assert!(is_already_equal(
        root.path(),
        &kind_entry("dir", EntryKind::Dir)
    ));

    let mut link = kind_entry("link", EntryKind::Symlink);
    link.link_target = Some(PathBuf::from("file"));
    assert!(is_already_equal(root.path(), &link));
    link.link_target = Some(PathBuf::from("other"));
    assert!(!is_already_equal(root.path(), &link));
    assert!(!is_already_equal(
        root.path(),
        &kind_entry("../escape", EntryKind::Dir)
    ));
}

#[test]
fn send_boundary_filters_events_buffered_before_ignore_discovery() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "synx-event-filter-test-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join(".gitignore"), "/ignored\n").unwrap();
    let ignores = IgnoreStack::from_manifest(&root, &[]);

    assert!(
        filter_outgoing_event(&root, &ignores, FsEvent::Modified(PathBuf::from("ignored")),)
            .is_none()
    );
    assert!(matches!(
        filter_outgoing_event(
            &root,
            &ignores,
            FsEvent::Renamed {
                from: PathBuf::from("visible"),
                to: PathBuf::from("ignored"),
            },
        ),
        Some(FsEvent::Removed(path)) if path == Path::new("visible")
    ));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn applies_file_directory_symlink_rename_and_delete_safely() {
    let root = TestDir::new("apply");
    let content = b"verified content";

    let dir = kind_entry("nested", EntryKind::Dir);
    apply_mkdir(root.path(), &dir).unwrap();
    assert!(root.path().join("nested").is_dir());

    let file = file_entry("nested/file.txt", content);
    apply_file_data(root.path(), &file, content).unwrap();
    assert_eq!(
        fs::read(root.path().join("nested/file.txt")).unwrap(),
        content
    );
    assert_eq!(
        fs::metadata(root.path().join("nested/file.txt"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );

    apply_rename(
        root.path(),
        Path::new("nested/file.txt"),
        Path::new("moved/file.txt"),
    )
    .unwrap();
    assert_eq!(
        fs::read(root.path().join("moved/file.txt")).unwrap(),
        content
    );

    let mut link = kind_entry("link", EntryKind::Symlink);
    link.link_target = Some(PathBuf::from("moved/file.txt"));
    apply_symlink(root.path(), &link).unwrap();
    assert_eq!(
        fs::read_link(root.path().join("link")).unwrap(),
        Path::new("moved/file.txt")
    );

    apply_delete(root.path(), Path::new("link")).unwrap();
    apply_delete(root.path(), Path::new("moved")).unwrap();
    apply_delete(root.path(), Path::new("missing")).unwrap();
    assert!(!root.path().join("link").exists());
    assert!(!root.path().join("moved").exists());
}

#[test]
fn rejects_corrupt_wrong_kind_and_out_of_root_mutations() {
    let root = TestDir::new("confine");
    let outside = TestDir::new("outside");
    fs::write(outside.path().join("sentinel"), b"safe").unwrap();

    let valid = file_entry("file", b"good");
    assert!(apply_file_data(root.path(), &valid, b"evil").is_err());
    assert!(!root.path().join("file").exists());

    let mut wrong_kind = valid.clone();
    wrong_kind.kind = EntryKind::Dir;
    assert!(apply_file_data(root.path(), &wrong_kind, b"good").is_err());
    assert!(apply_mkdir(root.path(), &valid).is_err());
    assert!(apply_symlink(root.path(), &valid).is_err());

    let escape = file_entry("../outside", b"evil");
    assert!(apply_file_data(root.path(), &escape, b"evil").is_err());
    assert!(apply_delete(root.path(), Path::new("../outside")).is_err());

    std::os::unix::fs::symlink(outside.path(), root.path().join("through-link")).unwrap();
    let through_link = file_entry("through-link/sentinel", b"evil");
    assert!(apply_file_data(root.path(), &through_link, b"evil").is_err());
    assert_eq!(fs::read(outside.path().join("sentinel")).unwrap(), b"safe");

    let mut missing_target = kind_entry("bad-link", EntryKind::Symlink);
    assert!(apply_symlink(root.path(), &missing_target).is_err());
    missing_target.link_target = Some(PathBuf::from("target"));
    apply_symlink(root.path(), &missing_target).unwrap();
    let as_dir = kind_entry("bad-link", EntryKind::Dir);
    assert!(apply_mkdir(root.path(), &as_dir).is_err());

    fs::create_dir(root.path().join("keep-dir")).unwrap();
    fs::write(root.path().join("keep-dir/sentinel"), b"safe").unwrap();
    let mut over_dir = kind_entry("keep-dir", EntryKind::Symlink);
    over_dir.link_target = Some(PathBuf::from("target"));
    assert!(apply_symlink(root.path(), &over_dir).is_err());
    assert_eq!(
        fs::read(root.path().join("keep-dir/sentinel")).unwrap(),
        b"safe"
    );
}

#[test]
fn computes_and_applies_verified_deltas() {
    let root = TestDir::new("delta");
    let mut base = vec![b'a'; 32 * 1024];
    base.extend(vec![b'b'; 32 * 1024]);
    let mut updated = base.clone();
    updated.splice(1000..1010, b"0123456789".iter().copied());
    updated.extend_from_slice(b"tail");

    let signature = compute_signature(&base);
    let delta = compute_delta(&signature, &updated).unwrap();
    assert_eq!(apply_delta_mem(&base, &delta).unwrap(), updated);
    assert!(compute_delta(b"bad signature", &updated).is_err());
    assert!(apply_delta_mem(&base, b"bad delta").is_err());

    let path = root.path().join("data.bin");
    fs::write(&path, &base).unwrap();
    let entry = file_entry("data.bin", &updated);
    let base_hash = *blake3::hash(&base).as_bytes();
    apply_delta_to_file(root.path(), &entry, base_hash, &delta).unwrap();
    assert_eq!(fs::read(&path).unwrap(), updated);

    fs::write(&path, &base).unwrap();
    assert!(apply_delta_to_file(root.path(), &entry, [9; 32], &delta).is_err());
    assert_eq!(fs::read(&path).unwrap(), base);

    let mut wrong_result = entry.clone();
    wrong_result.hash = [8; 32];
    assert!(apply_delta_to_file(root.path(), &wrong_result, base_hash, &delta).is_err());
    assert_eq!(fs::read(&path).unwrap(), base);
}

#[tokio::test]
async fn chunked_receive_verifies_integrity_and_replaces_duplicate_starts() {
    let root = TestDir::new("chunks");
    let pending = Pending::default();
    let content = b"chunk one + chunk two";
    let entry = file_entry("nested/file", content);

    pending.start(root.path(), entry.clone()).await.unwrap();
    let first_tmp = pending
        .inner
        .lock()
        .await
        .get(Path::new("nested/file"))
        .unwrap()
        .tmp
        .clone();
    pending.start(root.path(), entry.clone()).await.unwrap();
    assert!(!first_tmp.exists());
    pending.chunk(&entry.path, &content[..8]).await.unwrap();
    pending.chunk(&entry.path, &content[8..]).await.unwrap();
    assert_eq!(
        pending.end(root.path(), &entry.path).await.unwrap(),
        Some(entry.clone())
    );
    assert_eq!(fs::read(root.path().join("nested/file")).unwrap(), content);
    assert!(pending
        .end(root.path(), Path::new("unknown"))
        .await
        .unwrap()
        .is_none());

    let original = b"keep me";
    fs::write(root.path().join("nested/file"), original).unwrap();
    pending.start(root.path(), entry.clone()).await.unwrap();
    pending.chunk(&entry.path, b"corrupt").await.unwrap();
    assert!(pending.end(root.path(), &entry.path).await.is_err());
    assert_eq!(fs::read(root.path().join("nested/file")).unwrap(), original);
    assert!(pending
        .chunk(Path::new("unknown"), b"ignored")
        .await
        .is_ok());
}

#[tokio::test]
async fn sends_small_precompressed_large_and_vanished_files() {
    let root = TestDir::new("send");
    let small_content = b"small payload";
    let small = file_entry("small.txt", small_content);
    fs::write(root.path().join(&small.path), small_content).unwrap();
    let writer = Arc::new(Mutex::new(Vec::new()));
    assert_eq!(
        send_file(&writer, root.path(), &small, false)
            .await
            .unwrap(),
        small_content.len() as u64
    );
    let wire = writer.lock().await.clone();
    match read_message(&mut wire.as_slice()).await.unwrap() {
        Message::FileData { entry, content } => {
            assert_eq!(entry, small);
            assert_eq!(content, small_content);
        }
        other => panic!("unexpected small-file message: {other:?}"),
    }

    let compressed_content = vec![b'x'; 4096];
    let compressed = file_entry("archive.gz", &compressed_content);
    fs::write(root.path().join(&compressed.path), &compressed_content).unwrap();
    let writer = Arc::new(Mutex::new(Vec::new()));
    send_file(&writer, root.path(), &compressed, true)
        .await
        .unwrap();
    assert_eq!(
        writer.lock().await[4],
        0,
        "pre-compressed files bypass zstd"
    );

    let large_content = vec![0x5a; CHUNK_THRESHOLD];
    let large = file_entry("large.bin", &large_content);
    fs::write(root.path().join(&large.path), &large_content).unwrap();
    let writer = Arc::new(Mutex::new(Vec::new()));
    assert_eq!(
        send_file(&writer, root.path(), &large, false)
            .await
            .unwrap(),
        CHUNK_THRESHOLD as u64
    );
    let wire = writer.lock().await.clone();
    let mut reader = wire.as_slice();
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::FileStart { total_size, .. } if total_size == CHUNK_THRESHOLD as u64
    ));
    let mut rebuilt = Vec::new();
    loop {
        match read_message(&mut reader).await.unwrap() {
            Message::FileChunk { path, data } => {
                assert_eq!(path, large.path);
                rebuilt.extend(data);
            }
            Message::FileEnd { path } => {
                assert_eq!(path, large.path);
                break;
            }
            other => panic!("unexpected chunked message: {other:?}"),
        }
    }
    assert_eq!(rebuilt, large_content);

    let missing = file_entry("missing", b"gone");
    let writer = Arc::new(Mutex::new(Vec::new()));
    assert_eq!(
        send_file(&writer, root.path(), &missing, false)
            .await
            .unwrap(),
        0
    );
    assert!(writer.lock().await.is_empty());
    let directory = kind_entry("dir", EntryKind::Dir);
    assert!(send_file(&writer, root.path(), &directory, false)
        .await
        .is_err());
}

#[test]
fn suppression_is_state_based_and_tracks_hashes_and_deletes() {
    let root = TestDir::new("suppress");
    let path = PathBuf::from("file");
    fs::write(root.path().join(&path), b"one").unwrap();
    let mtime = lstat_mtime_ns(&root.path().join(&path));
    let hash = *blake3::hash(b"one").as_bytes();
    let suppression = Suppression::default();

    suppression.mark_set(path.clone(), mtime, hash);
    assert_eq!(suppression.prior_hash(&path), Some(hash));
    assert!(suppression.is_echo(root.path(), &FsEvent::Modified(path.clone())));

    let changed_time = filetime::FileTime::from_unix_time(
        mtime.div_euclid(1_000_000_000) + 1,
        mtime.rem_euclid(1_000_000_000) as u32,
    );
    filetime::set_file_mtime(root.path().join(&path), changed_time).unwrap();
    assert!(!suppression.is_echo(root.path(), &FsEvent::Modified(path.clone())));

    fs::remove_file(root.path().join(&path)).unwrap();
    suppression.mark_deleted(path.clone());
    assert!(suppression.is_recently_deleted(&path));
    assert!(suppression.is_echo(root.path(), &FsEvent::Removed(path.clone())));
    fs::write(root.path().join(&path), b"recreated").unwrap();
    assert!(!suppression.is_echo(root.path(), &FsEvent::Removed(path.clone())));

    suppression.mark_mtime(PathBuf::from("dir"), 1);
    assert_eq!(suppression.prior_hash(Path::new("dir")), None);
}

#[test]
fn observed_delete_guards_stale_creates_but_is_never_an_echo() {
    let root = TestDir::new("observed-del");
    let path = PathBuf::from("file");
    let suppression = Suppression::default();

    // Watcher-observed delete: stale-create guard engages, but the event
    // must NOT look like an echo — it still has to reach the peer.
    suppression.mark_observed_deleted(path.clone());
    assert!(suppression.is_recently_deleted(&path));
    assert!(!suppression.is_echo(root.path(), &FsEvent::Removed(path.clone())));

    // An applied delete stays echo-suppressible even when the watcher then
    // observes the unlink we caused.
    suppression.mark_deleted(path.clone());
    suppression.mark_observed_deleted(path.clone());
    assert!(suppression.is_echo(root.path(), &FsEvent::Removed(path)));
}

#[tokio::test]
async fn watcher_observed_delete_is_forwarded_to_peer() {
    // Regression: the watcher eagerly marks every Removed it emits (stale-
    // create guard). With a plain `mark_deleted` that mark made the very
    // same event pass `is_echo`, so real local deletes were silently
    // swallowed — on the agent (no reconcile sweep) they never reached the
    // client at all.
    let root = TestDir::new("observed-fwd");
    let suppress = Suppression::default();
    suppress.mark_observed_deleted(PathBuf::from("gone"));
    let writer = Arc::new(Mutex::new(Vec::new()));

    forward_local_events(
        root.path(),
        vec![FsEvent::Removed(PathBuf::from("gone"))],
        &writer,
        false,
        &suppress,
        false,
        &IgnoreStack::from_manifest(root.path(), &[]),
        &GitGate::default(),
        &LiveBaseline::disabled(),
    )
    .await
    .unwrap();

    let wire = writer.lock().await.clone();
    let mut reader = wire.as_slice();
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::Delete { path } if path == Path::new("gone")
    ));
}

#[test]
fn coalesces_event_storms_and_maps_sync_directions() {
    let dir = TestDir::new("coalesce");
    // Disk state is ground truth for the Created…Removed pattern:
    // "ephemeral" is gone → dropped; "rewritten" exists (unlink+recreate
    // coalesced by FSEvents) → kept as Modified, never as a delete.
    fs::write(dir.0.join("kept"), b"x").unwrap();
    fs::write(dir.0.join("rewritten"), b"y").unwrap();
    let events = vec![
        FsEvent::Created(PathBuf::from("ephemeral")),
        FsEvent::Modified(PathBuf::from("ephemeral")),
        FsEvent::Removed(PathBuf::from("ephemeral")),
        FsEvent::Created(PathBuf::from("rewritten")),
        FsEvent::Removed(PathBuf::from("rewritten")),
        FsEvent::Created(PathBuf::from("kept")),
        FsEvent::Modified(PathBuf::from("kept")),
        FsEvent::Renamed {
            from: PathBuf::from("old"),
            to: PathBuf::from("new"),
        },
    ];
    let result = coalesce(&dir.0, events);
    assert_eq!(result.len(), 3);
    assert!(matches!(&result[0], FsEvent::Modified(path) if path == Path::new("rewritten")));
    assert!(matches!(&result[1], FsEvent::Modified(path) if path == Path::new("kept")));
    assert!(
        matches!(&result[2], FsEvent::Renamed { from, to } if from == Path::new("old") && to == Path::new("new"))
    );

    assert_eq!(directions(SyncMode::Both, true), (true, true));
    assert_eq!(directions(SyncMode::Both, false), (true, true));
    assert_eq!(directions(SyncMode::Push, true), (true, false));
    assert_eq!(directions(SyncMode::Push, false), (false, true));
    assert_eq!(directions(SyncMode::Pull, true), (false, true));
    assert_eq!(directions(SyncMode::Pull, false), (true, false));
    assert!(is_under_git(Path::new(".git")));
    assert!(is_under_git(Path::new(".git/objects/aa")));
    assert!(!is_under_git(Path::new("src/.git")));
    assert!(!is_under_git(Path::new(".github/workflows")));
    assert!(!is_under_git(Path::new("")));
}

#[test]
fn git_gate_detects_live_stale_and_deferred_work() {
    let root = TestDir::new("git-gate");
    fs::create_dir(root.path().join(".git")).unwrap();
    assert!(!git_busy(root.path()));
    let marker = root.path().join(".git/MERGE_HEAD");
    fs::write(&marker, b"merge").unwrap();
    assert!(git_busy(root.path()));

    let gate = GitGate::default();
    assert!(gate.busy(root.path()));
    fs::remove_file(&marker).unwrap();
    assert!(gate.busy(root.path()));
    gate.inner.lock().unwrap().last_busy = Some(Instant::now() - GIT_SETTLE);
    assert!(!gate.busy(root.path()));

    gate.defer_out(PathBuf::from(".git/index"));
    gate.defer_out(PathBuf::from(".git/index"));
    gate.defer_in(Message::Delete {
        path: PathBuf::from(".git/lock"),
    });
    assert!(gate.has_deferred());
    let (out, incoming) = gate.take_deferred();
    assert_eq!(out, vec![PathBuf::from(".git/index")]);
    assert!(
        matches!(incoming.as_slice(), [Message::Delete { path }] if path == Path::new(".git/lock"))
    );
    assert!(!gate.has_deferred());

    fs::write(&marker, b"stale").unwrap();
    filetime::set_file_mtime(
        &marker,
        filetime::FileTime::from_system_time(SystemTime::now() - Duration::from_secs(601)),
    )
    .unwrap();
    assert!(!git_busy(root.path()));
}

#[tokio::test]
async fn handles_live_incoming_mutations_requests_and_guards() {
    let root = TestDir::new("incoming");
    fs::write(root.path().join(".gitignore"), "/ignored\n").unwrap();
    let ctx = session_ctx(root.path());
    let suppress = Suppression::default();
    let pending = Pending::default();
    let writer = Arc::new(Mutex::new(Vec::new()));

    let skipped = file_entry("disabled", b"no");
    handle_incoming(
        &ctx,
        Message::FileData {
            entry: skipped,
            content: b"no".to_vec(),
        },
        &suppress,
        &pending,
        &writer,
        false,
    )
    .await
    .unwrap();
    assert!(!root.path().join("disabled").exists());

    let file = file_entry("file", b"content");
    for _ in 0..2 {
        handle_incoming(
            &ctx,
            Message::FileData {
                entry: file.clone(),
                content: b"content".to_vec(),
            },
            &suppress,
            &pending,
            &writer,
            true,
        )
        .await
        .unwrap();
    }
    assert_eq!(fs::read(root.path().join("file")).unwrap(), b"content");

    let ignored = file_entry("ignored", b"secret");
    handle_incoming(
        &ctx,
        Message::FileData {
            entry: ignored,
            content: b"secret".to_vec(),
        },
        &suppress,
        &pending,
        &writer,
        true,
    )
    .await
    .unwrap();
    assert!(!root.path().join("ignored").exists());

    suppress.mark_deleted(PathBuf::from("stale"));
    let stale = file_entry("stale", b"old");
    handle_incoming(
        &ctx,
        Message::FileData {
            entry: stale,
            content: b"old".to_vec(),
        },
        &suppress,
        &pending,
        &writer,
        true,
    )
    .await
    .unwrap();
    assert!(!root.path().join("stale").exists());

    let directory = kind_entry("dir", EntryKind::Dir);
    for _ in 0..2 {
        handle_incoming(
            &ctx,
            Message::MkDir {
                entry: directory.clone(),
            },
            &suppress,
            &pending,
            &writer,
            true,
        )
        .await
        .unwrap();
    }
    let mut link = kind_entry("link", EntryKind::Symlink);
    link.link_target = Some(PathBuf::from("file"));
    for _ in 0..2 {
        handle_incoming(
            &ctx,
            Message::MkSymlink {
                entry: link.clone(),
            },
            &suppress,
            &pending,
            &writer,
            true,
        )
        .await
        .unwrap();
    }

    handle_incoming(
        &ctx,
        Message::Touch {
            path: PathBuf::from("file"),
            mtime: 1_710_000_000_000_000_000,
            mode: 0o600,
        },
        &suppress,
        &pending,
        &writer,
        true,
    )
    .await
    .unwrap();
    assert_eq!(
        fs::metadata(root.path().join("file"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    handle_incoming(
        &ctx,
        Message::Touch {
            path: PathBuf::from("missing"),
            mtime: 0,
            mode: 0o600,
        },
        &suppress,
        &pending,
        &writer,
        true,
    )
    .await
    .unwrap();
    assert!(handle_incoming(
        &ctx,
        Message::Touch {
            path: PathBuf::from("dir"),
            mtime: 0,
            mode: 0o600,
        },
        &suppress,
        &pending,
        &writer,
        true,
    )
    .await
    .is_err());

    let chunked = file_entry("chunked", b"two chunks");
    handle_incoming(
        &ctx,
        Message::FileStart {
            entry: chunked.clone(),
            total_size: chunked.size,
        },
        &suppress,
        &pending,
        &writer,
        true,
    )
    .await
    .unwrap();
    for data in [b"two ".as_slice(), b"chunks".as_slice()] {
        handle_incoming(
            &ctx,
            Message::FileChunk {
                path: chunked.path.clone(),
                data: data.to_vec(),
            },
            &suppress,
            &pending,
            &writer,
            true,
        )
        .await
        .unwrap();
    }
    handle_incoming(
        &ctx,
        Message::FileEnd {
            path: chunked.path.clone(),
        },
        &suppress,
        &pending,
        &writer,
        true,
    )
    .await
    .unwrap();
    assert_eq!(
        fs::read(root.path().join("chunked")).unwrap(),
        b"two chunks"
    );

    handle_incoming(
        &ctx,
        Message::Rename {
            from: PathBuf::from("chunked"),
            to: PathBuf::from("renamed"),
        },
        &suppress,
        &pending,
        &writer,
        true,
    )
    .await
    .unwrap();
    handle_incoming(
        &ctx,
        Message::Delete {
            path: PathBuf::from("renamed"),
        },
        &suppress,
        &pending,
        &writer,
        true,
    )
    .await
    .unwrap();
    assert!(!root.path().join("renamed").exists());

    for path in ["file", "dir", "link", "missing", "ignored"] {
        handle_incoming(
            &ctx,
            Message::FileGet {
                path: PathBuf::from(path),
            },
            &suppress,
            &pending,
            &writer,
            true,
        )
        .await
        .unwrap();
    }
    handle_incoming(&ctx, Message::Ping, &suppress, &pending, &writer, true)
        .await
        .unwrap();
    handle_incoming(&ctx, Message::Pong, &suppress, &pending, &writer, true)
        .await
        .unwrap();
    handle_incoming(
        &ctx,
        Message::Error("remote failure".into()),
        &suppress,
        &pending,
        &writer,
        true,
    )
    .await
    .unwrap();

    let wire = writer.lock().await.clone();
    let mut reader = wire.as_slice();
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::FileData { entry, content }
            if entry.path == Path::new("file") && content == b"content"
    ));
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::MkDir { entry } if entry.path == Path::new("dir")
    ));
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::MkSymlink { entry } if entry.path == Path::new("link")
    ));
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::Pong
    ));
    assert!(reader.is_empty());

    fs::create_dir(root.path().join(".git")).unwrap();
    fs::write(root.path().join(".git/MERGE_HEAD"), b"busy").unwrap();
    handle_incoming(
        &ctx,
        Message::Delete {
            path: PathBuf::from(".git/index"),
        },
        &suppress,
        &pending,
        &writer,
        true,
    )
    .await
    .unwrap();
    assert!(ctx.gate.has_deferred());
}

#[tokio::test]
async fn forwards_local_events_with_dedup_filtering_and_type_messages() {
    let root = TestDir::new("forward");
    fs::write(root.path().join(".gitignore"), "/ignored\n").unwrap();
    fs::write(root.path().join("file"), b"body").unwrap();
    fs::create_dir(root.path().join("dir")).unwrap();
    std::os::unix::fs::symlink("file", root.path().join("link")).unwrap();
    fs::write(root.path().join("renamed"), b"moved").unwrap();
    fs::write(root.path().join("touch"), b"same").unwrap();
    fs::write(root.path().join("echo"), b"echo").unwrap();
    fs::write(root.path().join("ignored"), b"hidden").unwrap();
    // Exists on disk: a `Removed` for it must be grounded to Modified,
    // never forwarded as a delete (the unlink+recreate FSEvents case).
    fs::write(root.path().join("zombie"), b"alive").unwrap();

    let ignores = IgnoreStack::from_manifest(root.path(), &[]);
    let suppress = Suppression::default();
    let touch_entry = build_entry(root.path(), Path::new("touch"))
        .unwrap()
        .unwrap();
    suppress.mark_set(PathBuf::from("touch"), 0, touch_entry.hash);
    let echo_entry = build_entry(root.path(), Path::new("echo"))
        .unwrap()
        .unwrap();
    suppress.mark_set(PathBuf::from("echo"), echo_entry.mtime, echo_entry.hash);
    let writer = Arc::new(Mutex::new(Vec::new()));

    forward_local_events(
        root.path(),
        vec![
            FsEvent::Created(PathBuf::from("file")),
            FsEvent::Modified(PathBuf::from("missing")),
            FsEvent::Created(PathBuf::from("dir")),
            FsEvent::Created(PathBuf::from("link")),
            FsEvent::Removed(PathBuf::from("gone")),
            FsEvent::Removed(PathBuf::from("zombie")),
            FsEvent::Renamed {
                from: PathBuf::from("old"),
                to: PathBuf::from("renamed"),
            },
            FsEvent::Modified(PathBuf::from("touch")),
            FsEvent::Modified(PathBuf::from("echo")),
            FsEvent::Modified(PathBuf::from("ignored")),
        ],
        &writer,
        false,
        &suppress,
        false,
        &ignores,
        &GitGate::default(),
        &LiveBaseline::disabled(),
    )
    .await
    .unwrap();

    let wire = writer.lock().await.clone();
    let mut reader = wire.as_slice();
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::FileData { entry, content }
            if entry.path == Path::new("file") && content == b"body"
    ));
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::Delete { path } if path == Path::new("missing")
    ));
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::MkDir { entry } if entry.path == Path::new("dir")
    ));
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::MkSymlink { entry } if entry.path == Path::new("link")
    ));
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::Delete { path } if path == Path::new("gone")
    ));
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::FileData { entry, content }
            if entry.path == Path::new("zombie") && content == b"alive"
    ));
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::Rename { from, to }
            if from == Path::new("old") && to == Path::new("renamed")
    ));
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::FileData { entry, content }
            if entry.path == Path::new("renamed") && content == b"moved"
    ));
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::Touch { path, .. } if path == Path::new("touch")
    ));
    assert!(reader.is_empty());
}

#[tokio::test]
async fn reconcile_sweep_catches_watcher_misses() {
    let root = TestDir::new("reconcile");
    fs::write(root.path().join("same"), b"same").unwrap();
    fs::write(root.path().join("changed"), b"v1").unwrap();
    fs::write(root.path().join("deleted"), b"x").unwrap();
    let mut converged = HashMap::new();
    for name in ["same", "changed", "deleted"] {
        let e = build_entry(root.path(), Path::new(name)).unwrap().unwrap();
        converged.insert(e.path.clone(), e);
    }
    let baseline = LiveBaseline::seed(
        root.path().to_path_buf(),
        converged,
        &crate::baseline::Baseline::default(),
    );

    // Diverge exactly like a git checkout would: unlink+recreate one
    // file, delete one, add one — with no watcher events at all.
    fs::remove_file(root.path().join("changed")).unwrap();
    fs::write(root.path().join("changed"), b"v2").unwrap();
    fs::remove_file(root.path().join("deleted")).unwrap();
    fs::write(root.path().join("added"), b"new").unwrap();

    let cache = Arc::new(StdMutex::new(HashCache::load(root.path())));
    let events = reconcile_sweep(root.path(), &baseline, &cache, &GitGate::default())
        .await
        .unwrap();

    let mut paths: Vec<&Path> = events
        .iter()
        .map(|e| match e {
            FsEvent::Created(p) | FsEvent::Modified(p) | FsEvent::Removed(p) => p.as_path(),
            _ => panic!("unexpected rename in sweep"),
        })
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        vec![
            Path::new("added"),
            Path::new("changed"),
            Path::new("deleted")
        ]
    );
    // The rewritten file must surface as Modified (alive on disk),
    // never as a delete — that inversion was the remote-data-loss bug.
    assert!(events
        .iter()
        .find(|e| matches!(e, FsEvent::Modified(p) if p == Path::new("changed")))
        .is_some());
    assert!(events
        .iter()
        .find(|e| matches!(e, FsEvent::Removed(p) if p == Path::new("deleted")))
        .is_some());

    // No-op while git is mid-operation: the walk excludes `.git/` and
    // would misreport those paths as deleted.
    fs::create_dir(root.path().join(".git")).unwrap();
    fs::write(root.path().join(".git/MERGE_HEAD"), b"busy").unwrap();
    let busy = reconcile_sweep(root.path(), &baseline, &cache, &GitGate::default())
        .await
        .unwrap();
    assert!(busy.is_empty());

    // No-op without a baseline (first run — nothing to diff against).
    let empty = reconcile_sweep(
        root.path(),
        &LiveBaseline::disabled(),
        &cache,
        &GitGate::default(),
    )
    .await
    .unwrap();
    assert!(empty.is_empty());
}
#[tokio::test]
async fn defers_only_git_events_while_repository_is_busy() {
    let root = TestDir::new("forward-git");
    fs::create_dir(root.path().join(".git")).unwrap();
    fs::write(root.path().join(".git/MERGE_HEAD"), b"busy").unwrap();
    fs::write(root.path().join("normal"), b"send").unwrap();
    let ignores = IgnoreStack::from_manifest(root.path(), &[]);
    let gate = GitGate::default();
    let writer = Arc::new(Mutex::new(Vec::new()));

    forward_local_events(
        root.path(),
        vec![
            FsEvent::Modified(PathBuf::from(".git/index")),
            FsEvent::Modified(PathBuf::from("normal")),
        ],
        &writer,
        false,
        &Suppression::default(),
        false,
        &ignores,
        &gate,
        &LiveBaseline::disabled(),
    )
    .await
    .unwrap();

    let wire = writer.lock().await.clone();
    let mut reader = wire.as_slice();
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::FileData { entry, content }
            if entry.path == Path::new("normal") && content == b"send"
    ));
    assert!(reader.is_empty());
    let (deferred, _) = gate.take_deferred();
    assert_eq!(deferred, vec![PathBuf::from(".git/index")]);
}

#[tokio::test]
async fn live_loop_replies_and_keeps_running_after_per_operation_errors() {
    let root = TestDir::new("live-loop");
    let ignores = Arc::new(IgnoreStack::from_manifest(root.path(), &[]));
    let ignore_state = Arc::new(std::sync::OnceLock::new());
    assert!(ignore_state.set(ignores.clone()).is_ok());
    let watcher = watcher::spawn(
        root.path().to_path_buf(),
        Suppression::default(),
        ignore_state,
    )
    .unwrap();
    let bad = file_entry("bad", b"expected");
    let mut input = Vec::new();
    for message in [
        Message::Ping,
        Message::FileData {
            entry: bad,
            content: b"wrong".to_vec(),
        },
        Message::Bye,
    ] {
        write_message(&mut input, &message, false).await.unwrap();
    }
    let writer = Arc::new(Mutex::new(Vec::new()));
    let ctx = SessionCtx {
        root: root.path().to_path_buf(),
        mode: SyncMode::Both,
        compress: false,
        is_client: false,
        ignores,
        gate: GitGate::default(),
        baseline: LiveBaseline::disabled(),
    };
    live_loop(
        ctx,
        std::io::Cursor::new(input),
        writer.clone(),
        Suppression::default(),
        Pending::default(),
        watcher,
        None,
    )
    .await
    .unwrap();

    let wire = writer.lock().await.clone();
    let mut reader = wire.as_slice();
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::Pong
    ));
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::Error(error) if error.contains("content mismatch")
    ));
    assert!(reader.is_empty());
    assert!(!root.path().join("bad").exists());
}

#[test]
fn reads_remotes_from_git_config_including_gitdir_files() {
    let root = TestDir::new("git-remotes");
    fs::create_dir_all(root.path().join(".git")).unwrap();
    fs::write(
        root.path().join(".git/config"),
        "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = git@github.com:Muvon/synx.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n[remote \"mirror\"]\n\turl = https://github.com/Muvon/synx\n[branch \"main\"]\n\tremote = origin\n\turl = https://example.com/ignored\n",
    )
    .unwrap();
    assert_eq!(
        git_remotes(root.path()),
        vec!["github.com/muvon/synx".to_string()]
    );

    // `.git` as a file (submodule / linked worktree) with commondir.
    let wt = TestDir::new("git-remotes-worktree");
    fs::create_dir_all(wt.path().join(".git-common")).unwrap();
    fs::write(wt.path().join(".git"), "gitdir: .git-common\n").unwrap();
    fs::write(wt.path().join(".git-common/commondir"), ".\n").unwrap();
    fs::write(
        wt.path().join(".git-common/config"),
        "[remote \"origin\"]\n\turl = ssh://git@github.com:22/Muvon/synx.git\n",
    )
    .unwrap();
    assert_eq!(
        git_remotes(wt.path()),
        vec!["github.com/muvon/synx".to_string()]
    );

    // Not a repo → empty, never an error.
    let plain = TestDir::new("git-remotes-plain");
    assert!(git_remotes(plain.path()).is_empty());
}

#[test]
fn normalizes_git_urls_across_schemes_and_syntaxes() {
    let canon = "github.com/muvon/synx";
    for url in [
        "git@github.com:Muvon/synx.git",
        "ssh://git@github.com:22/Muvon/synx.git",
        "ssh://github.com/Muvon/synx",
        "https://github.com/Muvon/synx.git",
        "http://user:pass@github.com:80/Muvon/synx.git/",
        "HTTPS://GitHub.Com/Muvon/Synx",
    ] {
        assert_eq!(normalize_git_url(url), canon, "{url}");
    }
    assert_eq!(
        normalize_git_url("/local/path/repo.git"),
        "/local/path/repo"
    );
    assert_eq!(normalize_git_url("github.com"), "github.com");
}

#[test]
fn conflicts_only_when_both_sides_have_disjoint_remotes() {
    let a = vec!["github.com/a".to_string()];
    let b = vec!["github.com/b".to_string()];
    assert!(git_remotes_conflict(&a, &b));
    assert!(!git_remotes_conflict(
        &a,
        &["gitlab.com/x".to_string(), "github.com/a".to_string()]
    ));
    // Unidentifiable sides never conflict.
    assert!(!git_remotes_conflict(&a, &[]));
    assert!(!git_remotes_conflict(&[], &b));
    assert!(!git_remotes_conflict(&[], &[]));
}

#[tokio::test]
async fn rename_forwarding_touches_unchanged_files_and_rekeys_the_baseline() {
    let root = TestDir::new("rename-fwd");
    fs::create_dir(root.path().join("dir")).unwrap();
    fs::write(root.path().join("dir/child"), b"kept").unwrap();
    fs::write(root.path().join("same"), b"same").unwrap();
    fs::write(root.path().join("edited"), b"v1").unwrap();
    let mut converged = HashMap::new();
    for rel in ["dir", "dir/child", "same", "edited"] {
        let e = build_entry(root.path(), Path::new(rel)).unwrap().unwrap();
        converged.insert(e.path.clone(), e);
    }
    let baseline = LiveBaseline::seed(
        root.path().to_path_buf(),
        converged,
        &crate::baseline::Baseline::default(),
    );

    fs::rename(root.path().join("dir"), root.path().join("moved")).unwrap();
    fs::rename(root.path().join("same"), root.path().join("same2")).unwrap();
    fs::rename(root.path().join("edited"), root.path().join("edited2")).unwrap();
    fs::write(root.path().join("edited2"), b"v2").unwrap();

    let writer = Arc::new(Mutex::new(Vec::new()));
    let renamed = |from: &str, to: &str| FsEvent::Renamed {
        from: PathBuf::from(from),
        to: PathBuf::from(to),
    };
    forward_local_events(
        root.path(),
        vec![
            renamed("dir", "moved"),
            renamed("same", "same2"),
            renamed("edited", "edited2"),
        ],
        &writer,
        false,
        &Suppression::default(),
        false,
        &IgnoreStack::from_manifest(root.path(), &[]),
        &GitGate::default(),
        &baseline,
    )
    .await
    .unwrap();

    let wire = writer.lock().await.clone();
    let mut reader = wire.as_slice();
    // A moved directory is one rename; its children never cross the wire.
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::Rename { from, to } if from == Path::new("dir") && to == Path::new("moved")
    ));
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::MkDir { entry } if entry.path == Path::new("moved")
    ));
    // Unchanged content: metadata only, no body.
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::Rename { from, to } if from == Path::new("same") && to == Path::new("same2")
    ));
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::Touch { path, .. } if path == Path::new("same2")
    ));
    // Moved and edited: the new body follows the rename.
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::Rename { from, to } if from == Path::new("edited") && to == Path::new("edited2")
    ));
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::FileData { entry, content }
            if entry.path == Path::new("edited2") && content == b"v2"
    ));
    assert!(reader.is_empty());

    // The baseline followed the moves, child included, so a sweep has
    // nothing left to re-send or delete.
    let keys = baseline.with_entries(|e| {
        let mut keys: Vec<PathBuf> = e.keys().cloned().collect();
        keys.sort();
        keys
    });
    assert_eq!(
        keys,
        ["edited2", "moved", "moved/child", "same2"].map(PathBuf::from)
    );
    assert_eq!(
        baseline.with_entries(|e| e[Path::new("moved/child")].hash),
        *blake3::hash(b"kept").as_bytes()
    );
    let cache = Arc::new(StdMutex::new(HashCache::default()));
    let events = reconcile_sweep(root.path(), &baseline, &cache, &GitGate::default())
        .await
        .unwrap();
    assert!(events.is_empty(), "{events:?}");
}

#[tokio::test]
async fn incoming_rename_rekeys_the_baseline_subtree() {
    let root = TestDir::new("rename-in");
    fs::create_dir(root.path().join("dir")).unwrap();
    fs::write(root.path().join("dir/child"), b"kept").unwrap();
    let mut converged = HashMap::new();
    for rel in ["dir", "dir/child"] {
        let e = build_entry(root.path(), Path::new(rel)).unwrap().unwrap();
        converged.insert(e.path.clone(), e);
    }
    let ctx = SessionCtx {
        baseline: LiveBaseline::seed(
            root.path().to_path_buf(),
            converged,
            &crate::baseline::Baseline::default(),
        ),
        ..session_ctx(root.path())
    };
    let writer = Arc::new(Mutex::new(Vec::new()));

    handle_incoming(
        &ctx,
        Message::Rename {
            from: PathBuf::from("dir"),
            to: PathBuf::from("moved"),
        },
        &Suppression::default(),
        &Pending::default(),
        &writer,
        true,
    )
    .await
    .unwrap();

    assert!(root.path().join("moved/child").is_file());
    let keys = ctx.baseline.with_entries(|e| {
        let mut keys: Vec<PathBuf> = e.keys().cloned().collect();
        keys.sort();
        keys
    });
    assert_eq!(keys, ["moved", "moved/child"].map(PathBuf::from));
    assert!(writer.lock().await.is_empty());
}
