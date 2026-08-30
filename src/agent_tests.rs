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
        let path =
            std::env::temp_dir().join(format!("synx-agent-{label}-{}-{nonce}", std::process::id()));
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
            root_existed: true,
            ..
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

#[tokio::test]
async fn reports_git_remotes_in_hello_ack() {
    let parent = TestDir::new("hello-remotes");
    let root = parent.0.join("repo");
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(
        root.join(".git/config"),
        "[remote \"origin\"]\n\turl = git@github.com:Muvon/synx.git\n",
    )
    .unwrap();
    let input = encode(vec![
        Message::Hello {
            version: PROTOCOL_VERSION,
            root: PathBuf::from("client"),
            mode: SyncMode::Both,
            compress: false,
        },
        Message::ManifestBegin,
        Message::ManifestEnd,
        Message::Bye,
    ])
    .await;
    let writer = Arc::new(Mutex::new(Vec::new()));
    run_io(root, std::io::Cursor::new(input), writer.clone())
        .await
        .unwrap();
    let output = writer.lock().await;
    let mut reader = output.as_slice();
    assert!(matches!(
        read_message(&mut reader).await.unwrap(),
        Message::HelloAck { ref git_remotes, .. }
            if git_remotes == &vec!["github.com/muvon/synx".to_string()]
    ));
}
