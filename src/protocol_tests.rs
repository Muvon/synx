use super::*;

fn file_entry(path: &str, hash: u8) -> Entry {
    Entry {
        path: PathBuf::from(path),
        kind: EntryKind::File,
        size: 3,
        mtime: 123,
        mode: 0o644,
        hash: [hash; 32],
        link_target: None,
    }
}

#[test]
fn same_content_uses_kind_specific_identity() {
    let file = file_entry("file", 1);
    let mut metadata_changed = file.clone();
    metadata_changed.mtime += 1;
    metadata_changed.mode = 0o600;
    assert!(file.same_content(&metadata_changed));
    assert!(!file.same_content(&file_entry("file", 2)));

    let dir_a = Entry {
        kind: EntryKind::Dir,
        hash: [1; 32],
        ..file.clone()
    };
    let dir_b = Entry {
        mtime: 999,
        ..dir_a.clone()
    };
    assert!(dir_a.same_content(&dir_b));
    assert!(!dir_a.same_content(&file));

    let link_a = Entry {
        kind: EntryKind::Symlink,
        link_target: Some(PathBuf::from("target-a")),
        ..file.clone()
    };
    let link_b = Entry {
        link_target: Some(PathBuf::from("target-b")),
        ..link_a.clone()
    };
    assert!(!link_a.same_content(&link_b));
}

#[tokio::test]
async fn frames_round_trip_with_and_without_compression() {
    for compress in [false, true] {
        let message = Message::Error("compressible payload ".repeat(512));
        let mut wire = Vec::new();
        write_frame(&mut wire, &message, compress).await.unwrap();
        assert_eq!(wire[4] & FLAG_COMPRESSED != 0, compress);

        let decoded = read_message(&mut wire.as_slice()).await.unwrap();
        match decoded {
            Message::Error(value) => assert_eq!(value, "compressible payload ".repeat(512)),
            other => panic!("unexpected message: {other:?}"),
        }
    }
}

#[tokio::test]
async fn write_message_produces_a_readable_frame() {
    let mut wire = Vec::new();
    write_message(&mut wire, &Message::Ping, false)
        .await
        .unwrap();
    assert!(matches!(
        read_message(&mut wire.as_slice()).await.unwrap(),
        Message::Ping
    ));
}

#[tokio::test]
async fn rejects_oversized_unknown_and_invalid_frames() {
    let mut oversized = Vec::from(((MAX_MESSAGE_SIZE + 1) as u32).to_be_bytes());
    oversized.push(0);
    let error = read_message(&mut oversized.as_slice()).await.unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("message too large"));

    let unknown = vec![0, 0, 0, 0, 0x80];
    let error = read_message(&mut unknown.as_slice()).await.unwrap_err();
    assert!(error.to_string().contains("unknown frame flags"));

    let invalid = vec![0, 0, 0, 1, 0, 0xff];
    assert_eq!(
        read_message(&mut invalid.as_slice())
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
}

#[tokio::test]
async fn hello_ack_round_trips_git_remotes() {
    let mut wire = Vec::new();
    write_frame(
        &mut wire,
        &Message::HelloAck {
            version: PROTOCOL_VERSION,
            root_existed: true,
            git_remotes: vec!["github.com/muvon/synx".into()],
        },
        false,
    )
    .await
    .unwrap();
    assert!(matches!(
        read_message(&mut wire.as_slice()).await.unwrap(),
        Message::HelloAck { ref git_remotes, .. }
            if git_remotes == &vec!["github.com/muvon/synx".to_string()]
    ));
}

#[tokio::test]
async fn rejects_absolute_parent_and_empty_protocol_paths() {
    let messages = [
        Message::Delete {
            path: PathBuf::from("../outside"),
        },
        Message::FileGet {
            path: PathBuf::from("/etc/passwd"),
        },
        Message::Rename {
            from: PathBuf::from("safe"),
            to: PathBuf::from("a/../../outside"),
        },
        Message::FileEnd {
            path: PathBuf::new(),
        },
    ];
    for message in messages {
        let mut wire = Vec::new();
        write_frame(&mut wire, &message, false).await.unwrap();
        let error = read_message(&mut wire.as_slice()).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(
            error.to_string().contains("unsafe protocol path")
                || error.to_string().contains("must not be empty")
        );
    }
}

#[tokio::test]
async fn rejects_message_entry_kind_and_size_mismatches() {
    let mut directory = file_entry("path", 1);
    directory.kind = EntryKind::Dir;
    let messages = [
        Message::FileData {
            entry: directory.clone(),
            content: Vec::new(),
        },
        Message::MkDir {
            entry: file_entry("path", 1),
        },
        Message::FileStart {
            entry: file_entry("path", 1),
            total_size: 99,
        },
    ];
    for message in messages {
        let mut wire = Vec::new();
        write_frame(&mut wire, &message, false).await.unwrap();
        assert_eq!(
            read_message(&mut wire.as_slice()).await.unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}

#[test]
fn compressed_decode_is_bounded() {
    let compressed = zstd::encode_all(&vec![b'x'; 4096][..], 1).unwrap();
    let error = decode_compressed_limited(&compressed, 1024).unwrap_err();
    assert!(error.to_string().contains("decompressed message too large"));

    let decoded = decode_compressed_limited(&compressed, 4096).unwrap();
    assert_eq!(decoded, vec![b'x'; 4096]);
    assert!(decode_compressed_limited(b"not zstd", 4096).is_err());
}
