use super::*;
use crate::protocol::write_frame;
use crate::walker::build_entry;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

struct TestDir(PathBuf);

impl TestDir {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("synx-sync-{label}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn entry(path: &str, kind: EntryKind, hash: u8, mtime: i64) -> Entry {
    Entry {
        path: PathBuf::from(path),
        kind,
        size: if kind == EntryKind::File { 10 } else { 0 },
        mtime,
        mode: 0o644,
        hash: if kind == EntryKind::File {
            [hash; 32]
        } else {
            [0; 32]
        },
        link_target: (kind == EntryKind::Symlink).then(|| PathBuf::from(format!("target-{hash}"))),
    }
}

fn paths(entries: &[Entry]) -> Vec<&str> {
    entries
        .iter()
        .map(|entry| entry.path.to_str().unwrap())
        .collect()
}

fn file_entry(path: &str, content: &[u8], mtime: i64) -> Entry {
    Entry {
        path: PathBuf::from(path),
        kind: EntryKind::File,
        size: content.len() as u64,
        mtime,
        mode: 0o640,
        hash: *blake3::hash(content).as_bytes(),
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

fn once_args(root: &Path) -> ClientArgs {
    ClientArgs {
        local: root.display().to_string(),
        remote: "host:/remote".into(),
        mode: SyncMode::Both,
        ssh_opts: None,
        no_compress: true,
        once: true,
        dry_run: false,
        allow_repo_mismatch: false,
        remote_synx: "synx".into(),
    }
}

#[test]
fn first_sync_merges_without_inventing_deletions() {
    let local = vec![entry("local", EntryKind::File, 1, 1)];
    let remote = vec![entry("remote", EntryKind::File, 2, 1)];
    let baseline = Baseline::default();
    let plan = build_plan(&local, &remote, &baseline, SyncMode::Both);
    assert_eq!(paths(&plan.push), ["local"]);
    assert_eq!(plan.get, [PathBuf::from("remote")]);
    assert!(plan.del_local.is_empty());
    assert!(plan.del_remote.is_empty());
    assert!(plan.conflicts.is_empty());
}

#[test]
fn propagates_only_baseline_proven_deletions_for_each_mode() {
    let shared = entry("file", EntryKind::File, 1, 1);
    let baseline = Baseline::from_entries([shared.clone()]);

    let remote_deleted = build_plan(
        std::slice::from_ref(&shared),
        &[],
        &baseline,
        SyncMode::Both,
    );
    assert_eq!(remote_deleted.del_local, std::slice::from_ref(&shared.path));
    assert!(remote_deleted.push.is_empty());

    let push_wins = build_plan(
        std::slice::from_ref(&shared),
        &[],
        &baseline,
        SyncMode::Push,
    );
    assert_eq!(paths(&push_wins.push), ["file"]);
    assert!(push_wins.del_local.is_empty());

    let local_deleted = build_plan(
        &[],
        std::slice::from_ref(&shared),
        &baseline,
        SyncMode::Both,
    );
    assert_eq!(local_deleted.del_remote, std::slice::from_ref(&shared.path));
    assert!(local_deleted.get.is_empty());

    let pull_wins = build_plan(
        &[],
        std::slice::from_ref(&shared),
        &baseline,
        SyncMode::Pull,
    );
    assert_eq!(pull_wins.get, std::slice::from_ref(&shared.path));
    assert!(pull_wins.del_remote.is_empty());
}

#[test]
fn modify_vs_delete_keeps_changed_data() {
    let old = entry("file", EntryKind::File, 1, 1);
    let local_changed = entry("file", EntryKind::File, 2, 2);
    let baseline = Baseline::from_entries([old.clone()]);
    let plan = build_plan(
        std::slice::from_ref(&local_changed),
        &[],
        &baseline,
        SyncMode::Both,
    );
    assert_eq!(paths(&plan.push), ["file"]);
    assert!(plan.del_local.is_empty());

    let remote_changed = entry("file", EntryKind::File, 3, 2);
    let plan = build_plan(
        &[],
        std::slice::from_ref(&remote_changed),
        &baseline,
        SyncMode::Both,
    );
    assert_eq!(plan.get, [PathBuf::from("file")]);
    assert!(plan.del_remote.is_empty());
}

#[test]
fn resolves_content_by_mode_and_mtime_but_never_type_conflicts() {
    let local = entry("file", EntryKind::File, 1, 10);
    let remote = entry("file", EntryKind::File, 2, 20);
    let baseline = Baseline::default();

    assert_eq!(
        build_plan(
            std::slice::from_ref(&local),
            std::slice::from_ref(&remote),
            &baseline,
            SyncMode::Both,
        )
        .get,
        [PathBuf::from("file")]
    );
    assert_eq!(
        paths(
            &build_plan(
                std::slice::from_ref(&local),
                std::slice::from_ref(&remote),
                &baseline,
                SyncMode::Push,
            )
            .push
        ),
        ["file"]
    );
    let same_time_remote = Entry {
        mtime: 10,
        ..remote.clone()
    };
    assert_eq!(
        paths(
            &build_plan(
                std::slice::from_ref(&local),
                std::slice::from_ref(&same_time_remote),
                &baseline,
                SyncMode::Both,
            )
            .push
        ),
        ["file"]
    );

    let remote_dir = entry("file", EntryKind::Dir, 0, 20);
    let conflict = build_plan(&[local], &[remote_dir], &baseline, SyncMode::Both);
    assert!(conflict.push.is_empty());
    assert!(conflict.get.is_empty());
    assert_eq!(
        conflict.conflicts,
        [(PathBuf::from("file"), EntryKind::File, EntryKind::Dir)]
    );
}

#[test]
fn skips_equal_content_and_orders_pushes_by_kind_then_path() {
    let same_local = entry("same", EntryKind::File, 1, 1);
    let same_remote = Entry {
        mtime: 99,
        ..same_local.clone()
    };
    let local = vec![
        same_local,
        entry("z-file", EntryKind::File, 2, 1),
        entry("b-link", EntryKind::Symlink, 3, 1),
        entry("a-dir", EntryKind::Dir, 0, 1),
    ];
    let plan = build_plan(&local, &[same_remote], &Baseline::default(), SyncMode::Push);
    assert_eq!(paths(&plan.push), ["a-dir", "b-link", "z-file"]);
}

#[tokio::test]
async fn manifest_receiver_accepts_valid_stream_and_rejects_bad_sequences() {
    let first = entry("a", EntryKind::File, 1, 1);
    let second = entry("b", EntryKind::Dir, 0, 1);
    let mut wire = Vec::new();
    for message in [
        Message::ManifestBegin,
        Message::ManifestEntry(first.clone()),
        Message::ManifestEntry(second.clone()),
        Message::ManifestEnd,
    ] {
        write_frame(&mut wire, &message, false).await.unwrap();
    }
    assert_eq!(
        receive_manifest(&mut wire.as_slice()).await.unwrap(),
        [first.clone(), second]
    );

    let mut duplicate = Vec::new();
    for message in [
        Message::ManifestBegin,
        Message::ManifestEntry(first.clone()),
        Message::ManifestEntry(first),
        Message::ManifestEnd,
    ] {
        write_frame(&mut duplicate, &message, false).await.unwrap();
    }
    assert!(receive_manifest(&mut duplicate.as_slice())
        .await
        .unwrap_err()
        .to_string()
        .contains("duplicate manifest path"));

    let mut wrong_start = Vec::new();
    write_frame(&mut wrong_start, &Message::Ping, false)
        .await
        .unwrap();
    assert!(receive_manifest(&mut wrong_start.as_slice()).await.is_err());

    let mut error_stream = Vec::new();
    write_frame(
        &mut error_stream,
        &Message::Error("remote failure".into()),
        false,
    )
    .await
    .unwrap();
    assert!(receive_manifest(&mut error_stream.as_slice())
        .await
        .is_err());
}

#[tokio::test]
async fn executes_initial_sync_push_pull_delta_and_mutation_paths() {
    let root = TestDir::new("inner");
    fs::create_dir(root.0.join("local-dir")).unwrap();
    fs::write(root.0.join("local-small"), b"push me").unwrap();
    std::os::unix::fs::symlink("local-small", root.0.join("local-link")).unwrap();

    let push_old = vec![b'a'; 300 * 1024];
    let mut push_new = push_old.clone();
    push_new[4096..4106].copy_from_slice(b"0123456789");
    fs::write(root.0.join("push-delta"), &push_new).unwrap();

    let pull_old = vec![b'b'; 300 * 1024];
    let mut pull_new = pull_old.clone();
    pull_new[8192..8202].copy_from_slice(b"abcdefghij");
    fs::write(root.0.join("pull-delta"), &pull_old).unwrap();

    let push_remote = file_entry("push-delta", &push_old, 0);
    let pull_remote = file_entry("pull-delta", &pull_new, i64::MAX);
    let remote_only = file_entry("remote-only", b"pulled", i64::MAX);
    let remote_dir = entry("remote-dir", EntryKind::Dir, 0, i64::MAX);
    let mut remote_link = entry("remote-link", EntryKind::Symlink, 1, i64::MAX);
    remote_link.link_target = Some(PathBuf::from("remote-only"));

    let push_signature = compute_signature(&push_old);
    let pull_signature = compute_signature(&pull_old);
    let pull_delta = compute_delta(&pull_signature, &pull_new).unwrap();
    let pull_base_hash = *blake3::hash(&pull_old).as_bytes();
    let chunked = file_entry("chunked", b"chunked response", i64::MAX);
    let extra = file_entry("extra", b"temporary", i64::MAX);

    let input = encode([
        Message::ManifestBegin,
        Message::ManifestEntry(push_remote.clone()),
        Message::ManifestEntry(pull_remote.clone()),
        Message::ManifestEntry(remote_only.clone()),
        Message::ManifestEntry(remote_dir.clone()),
        Message::ManifestEntry(remote_link.clone()),
        Message::ManifestEnd,
        Message::Signature {
            path: PathBuf::from("push-delta"),
            sig: Some(push_signature),
        },
        Message::FileData {
            entry: remote_only.clone(),
            content: b"pulled".to_vec(),
        },
        Message::Delta {
            entry: pull_remote.clone(),
            base_hash: pull_base_hash,
            delta: pull_delta,
        },
        Message::MkDir {
            entry: remote_dir.clone(),
        },
        Message::MkSymlink {
            entry: remote_link.clone(),
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
            data: b"response".to_vec(),
        },
        Message::FileEnd {
            path: chunked.path.clone(),
        },
        Message::Touch {
            path: remote_only.path.clone(),
            mtime: 1_720_000_000_000_000_000,
            mode: 0o600,
        },
        Message::FileData {
            entry: extra.clone(),
            content: b"temporary".to_vec(),
        },
        Message::Rename {
            from: extra.path.clone(),
            to: PathBuf::from("extra-renamed"),
        },
        Message::Delete {
            path: PathBuf::from("extra-renamed"),
        },
        Message::Error("non-fatal remote apply error".into()),
        Message::Ping,
        Message::SyncDone,
    ])
    .await;
    let writer = Arc::new(Mutex::new(Vec::new()));
    run_inner(
        root.0.clone(),
        once_args(&root.0),
        false,
        std::io::Cursor::new(input),
        writer.clone(),
        None,
    )
    .await
    .unwrap();

    assert_eq!(fs::read(root.0.join("pull-delta")).unwrap(), pull_new);
    assert_eq!(fs::read(root.0.join("remote-only")).unwrap(), b"pulled");
    assert_eq!(
        fs::metadata(root.0.join("remote-only"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::read(root.0.join("chunked")).unwrap(),
        b"chunked response"
    );
    assert!(root.0.join("remote-dir").is_dir());
    assert_eq!(
        fs::read_link(root.0.join("remote-link")).unwrap(),
        Path::new("remote-only")
    );
    assert!(!root.0.join("extra-renamed").exists());

    let output = writer.lock().await.clone();
    let mut reader = output.as_slice();
    let mut messages = Vec::new();
    while !reader.is_empty() {
        messages.push(read_message(&mut reader).await.unwrap());
    }
    assert!(messages
        .iter()
        .any(|message| matches!(message, Message::ManifestBegin)));
    assert!(messages.iter().any(
        |message| matches!(message, Message::MkDir { entry } if entry.path == Path::new("local-dir"))
    ));
    assert!(messages.iter().any(
        |message| matches!(message, Message::MkSymlink { entry } if entry.path == Path::new("local-link"))
    ));
    assert!(messages.iter().any(
        |message| matches!(message, Message::FileData { entry, content } if entry.path == Path::new("local-small") && content == b"push me")
    ));
    assert!(messages.iter().any(
        |message| matches!(message, Message::SignatureRequest { path, .. } if path == Path::new("push-delta"))
    ));
    assert!(messages.iter().any(
        |message| matches!(message, Message::Delta { entry, .. } if entry.path == Path::new("push-delta"))
    ));
    assert!(messages.iter().any(
        |message| matches!(message, Message::PullDelta { path, .. } if path == Path::new("pull-delta"))
    ));
    assert!(messages.iter().any(
        |message| matches!(message, Message::FileGet { path } if path == Path::new("remote-only"))
    ));
    assert!(messages
        .iter()
        .any(|message| matches!(message, Message::SyncDone)));
    assert!(matches!(messages.last(), Some(Message::Bye)));
}

#[tokio::test]
async fn dry_run_stops_after_manifest_and_plan() {
    let root = TestDir::new("dry");
    fs::write(root.0.join("local"), b"unchanged").unwrap();
    let local = build_entry(&root.0, Path::new("local")).unwrap().unwrap();
    let input = encode([
        Message::ManifestBegin,
        Message::ManifestEntry(local),
        Message::ManifestEnd,
    ])
    .await;
    let writer = Arc::new(Mutex::new(Vec::new()));
    let mut args = once_args(&root.0);
    args.dry_run = true;
    run_inner(
        root.0.clone(),
        args,
        false,
        std::io::Cursor::new(input),
        writer.clone(),
        None,
    )
    .await
    .unwrap();

    let output = writer.lock().await.clone();
    let mut reader = output.as_slice();
    let mut saw_bye = false;
    while !reader.is_empty() {
        saw_bye |= matches!(read_message(&mut reader).await.unwrap(), Message::Bye);
    }
    assert!(saw_bye);
}

#[test]
fn classifies_fatal_errors_and_shortens_multiline_messages() {
    assert!(is_fatal(&anyhow::anyhow!("protocol mismatch")));
    assert!(is_fatal(&anyhow::anyhow!("invalid local path")));
    assert!(is_fatal(&anyhow::anyhow!("remote must be host:path")));
    assert!(is_fatal(&anyhow::anyhow!(
        "refusing to sync: different git repositories"
    )));
    assert!(!is_fatal(&anyhow::anyhow!("connection reset")));
    assert_eq!(short_err(&anyhow::anyhow!("first\nsecond")), "first");
}
