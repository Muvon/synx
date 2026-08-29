use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_MESSAGE_SIZE: usize = 64 * 1024 * 1024; // 64 MiB per-message
pub const COMPRESS_THRESHOLD: usize = 512;
pub const COMPRESS_LEVEL: i32 = 3;
/// Buffer size for the SSH stdin/stdout streams. The default 8 KiB flushes
/// far too often when streaming a large manifest (one tiny write per entry);
/// 64 KiB amortizes syscalls without a meaningful memory cost (one link).
pub const IO_BUF_SIZE: usize = 64 * 1024;

/// Files smaller than this are sent as a single `FileData` message;
/// anything larger is streamed via `FileStart` / `FileChunk` / `FileEnd`.
pub const CHUNK_THRESHOLD: usize = 16 * 1024 * 1024; // 16 MiB
/// Size of each `FileChunk` payload during chunked transfer.
pub const CHUNK_SIZE: usize = 4 * 1024 * 1024; // 4 MiB

#[derive(clap::ValueEnum, Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SyncMode {
    /// local → remote only
    Push,
    /// remote → local only
    Pull,
    /// bidirectional (default)
    Both,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
}

/// A single filesystem entry, relative to the sync root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub path: PathBuf,
    pub kind: EntryKind,
    pub size: u64,
    /// nanoseconds since Unix epoch
    pub mtime: i64,
    pub mode: u32,
    /// blake3 hash; zeroed for non-files
    pub hash: [u8; 32],
    pub link_target: Option<PathBuf>,
}

impl Entry {
    pub fn same_content(&self, other: &Entry) -> bool {
        if self.kind != other.kind {
            return false;
        }
        match self.kind {
            EntryKind::File => self.hash == other.hash,
            EntryKind::Dir => true,
            EntryKind::Symlink => self.link_target == other.link_target,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    /// First message from client.
    Hello {
        version: u32,
        root: PathBuf,
        mode: SyncMode,
        compress: bool,
    },
    /// Server's response.
    HelloAck {
        version: u32,
        root_existed: bool,
    },

    // ── Manifest exchange (streaming) ──
    ManifestBegin,
    ManifestEntry(Entry),
    ManifestEnd,

    // ── Filesystem operations ──
    /// Request peer to send us this file.
    FileGet {
        path: PathBuf,
    },
    /// Whole-file payload (small/medium files).
    FileData {
        entry: Entry,
        content: Vec<u8>,
    },
    /// Begin a chunked file transfer; receiver opens a tmp file.
    FileStart {
        entry: Entry,
        total_size: u64,
    },
    /// One chunk of a large file (multiple per file).
    FileChunk {
        path: PathBuf,
        data: Vec<u8>,
    },
    /// Finish a chunked file transfer; receiver renames tmp into place.
    FileEnd {
        path: PathBuf,
    },
    /// Sync metadata only (mtime + mode) when the file's content is
    /// unchanged. Avoids re-transferring the body on a `touch`-like change.
    Touch {
        path: PathBuf,
        mtime: i64,
        mode: u32,
    },
    /// Ask peer to compute a rsync-style signature of the file it has at
    /// `path` (its old version). `base_hash` lets the peer verify it still
    /// has the version the sender expects.
    SignatureRequest {
        path: PathBuf,
        base_hash: [u8; 32],
    },
    /// Response to `SignatureRequest`. `sig = None` means the peer can't
    /// (or won't) produce a signature — the sender should fall back to a
    /// full transfer.
    Signature {
        path: PathBuf,
        sig: Option<Vec<u8>>,
    },
    /// Patch the peer's existing file using the given delta.
    /// `base_hash` is the hash of the version the delta was computed against;
    /// receiver MUST verify the result (blake3) matches `entry.hash`.
    Delta {
        entry: Entry,
        base_hash: [u8; 32],
        delta: Vec<u8>,
    },
    /// Client-initiated pull with a signature of the version we already
    /// have. The server responds with a `Delta` (preferred) or `FileData` /
    /// chunked transfer fallback.
    PullDelta {
        path: PathBuf,
        base_hash: [u8; 32],
        sig: Vec<u8>,
    },
    /// Create or update a directory's metadata.
    MkDir {
        entry: Entry,
    },
    /// Create or replace a symlink.
    MkSymlink {
        entry: Entry,
    },
    /// Remove a path (files or dirs).
    Delete {
        path: PathBuf,
    },
    /// Rename / move within the sync root.
    Rename {
        from: PathBuf,
        to: PathBuf,
    },

    /// Sender has nothing more for the initial-sync phase.
    SyncDone,

    Ping,
    Pong,
    Bye,
    Error(String),
}

/// Protocol paths are always relative to the negotiated synchronization root.
/// Rejecting traversal at decode time protects every dispatcher, including
/// future message handlers that might otherwise forget a local check.
fn validate_relative_path(path: &Path) -> io::Result<()> {
    let mut has_component = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => has_component = true,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unsafe protocol path: {}", path.display()),
                ));
            }
        }
    }
    if !has_component {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "protocol path must not be empty",
        ));
    }
    Ok(())
}

impl Message {
    fn validate_paths(&self) -> io::Result<()> {
        match self {
            Message::ManifestEntry(entry) => validate_relative_path(&entry.path),
            Message::FileData { entry, .. } | Message::Delta { entry, .. } => {
                validate_entry_kind(entry, EntryKind::File)
            }
            Message::FileStart { entry, total_size } => {
                validate_entry_kind(entry, EntryKind::File)?;
                if *total_size != entry.size {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "FileStart size does not match entry metadata",
                    ));
                }
                Ok(())
            }
            Message::MkDir { entry } => validate_entry_kind(entry, EntryKind::Dir),
            Message::MkSymlink { entry } => validate_entry_kind(entry, EntryKind::Symlink),
            Message::FileGet { path }
            | Message::FileChunk { path, .. }
            | Message::FileEnd { path }
            | Message::Touch { path, .. }
            | Message::SignatureRequest { path, .. }
            | Message::Signature { path, .. }
            | Message::PullDelta { path, .. }
            | Message::Delete { path } => validate_relative_path(path),
            Message::Rename { from, to } => {
                validate_relative_path(from)?;
                validate_relative_path(to)
            }
            Message::Hello { .. }
            | Message::HelloAck { .. }
            | Message::ManifestBegin
            | Message::ManifestEnd
            | Message::SyncDone
            | Message::Ping
            | Message::Pong
            | Message::Bye
            | Message::Error(_) => Ok(()),
        }
    }
}

fn validate_entry_kind(entry: &Entry, expected: EntryKind) -> io::Result<()> {
    validate_relative_path(&entry.path)?;
    if entry.kind != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "message entry kind mismatch for {}: expected {expected:?}, got {:?}",
                entry.path.display(),
                entry.kind
            ),
        ));
    }
    Ok(())
}

// ── Wire framing ──
// [u32 BE length][u8 flags][payload]
// flag bit 0 (FLAG_COMPRESSED): payload is zstd-compressed postcard

const FLAG_COMPRESSED: u8 = 0x01;

pub async fn read_message<R>(reader: &mut R) -> io::Result<Message>
where
    R: AsyncReadExt + Unpin,
{
    let mut hdr = [0u8; 5];
    reader.read_exact(&mut hdr).await?;
    let len = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as usize;
    let flags = hdr[4];
    if flags & !FLAG_COMPRESSED != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unknown frame flags: {flags:#04x}"),
        ));
    }
    if len > MAX_MESSAGE_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message too large: {} bytes", len),
        ));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    let bytes = if flags & FLAG_COMPRESSED != 0 {
        decode_compressed_limited(&buf, MAX_MESSAGE_SIZE)?
    } else {
        buf
    };
    let message: Message =
        postcard::from_bytes(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    message.validate_paths()?;
    Ok(message)
}

/// Decode without allowing a small compressed frame to expand beyond the
/// protocol's memory bound. The on-wire limit alone does not stop a zstd bomb.
fn decode_compressed_limited(compressed: &[u8], max_size: usize) -> io::Result<Vec<u8>> {
    let decoder = zstd::stream::read::Decoder::new(compressed)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut decoded = Vec::new();
    decoder
        .take(max_size.saturating_add(1) as u64)
        .read_to_end(&mut decoded)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if decoded.len() > max_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("decompressed message too large: {} bytes", decoded.len()),
        ));
    }
    Ok(decoded)
}

/// Encode + frame a message into `writer` WITHOUT flushing. Use for bulk
/// streaming (the manifest) where the caller flushes once at the end; the
/// BufWriter still auto-flushes when full, so back-pressure is preserved.
pub async fn write_frame<W>(writer: &mut W, msg: &Message, compress: bool) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let bytes =
        postcard::to_allocvec(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let (payload, flags) = if compress && bytes.len() > COMPRESS_THRESHOLD {
        let c = zstd::encode_all(&bytes[..], COMPRESS_LEVEL).map_err(io::Error::other)?;
        // Only use the compressed form if it actually saves space.
        if c.len() + 5 < bytes.len() {
            (c, FLAG_COMPRESSED)
        } else {
            (bytes, 0)
        }
    } else {
        (bytes, 0)
    };
    if payload.len() > MAX_MESSAGE_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("outgoing message too large: {} bytes", payload.len()),
        ));
    }
    let len = (payload.len() as u32).to_be_bytes();
    writer.write_all(&len).await?;
    writer.write_all(&[flags]).await?;
    writer.write_all(&payload).await?;
    Ok(())
}

/// Frame + flush a single message. The default for live traffic, where each
/// message should hit the wire immediately.
pub async fn write_message<W>(writer: &mut W, msg: &Message, compress: bool) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    write_frame(writer, msg, compress).await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
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
}
