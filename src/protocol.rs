use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_MESSAGE_SIZE: usize = 64 * 1024 * 1024; // 64 MiB per-message
pub const COMPRESS_THRESHOLD: usize = 512;
pub const COMPRESS_LEVEL: i32 = 3;

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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

// ── Wire framing ──
// [u32 BE length][u8 flags][payload]
// flag bit 0 (FLAG_COMPRESSED): payload is zstd-compressed bincode

const FLAG_COMPRESSED: u8 = 0x01;

pub async fn read_message<R>(reader: &mut R) -> io::Result<Message>
where
    R: AsyncReadExt + Unpin,
{
    let mut hdr = [0u8; 5];
    reader.read_exact(&mut hdr).await?;
    let len = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as usize;
    let flags = hdr[4];
    if len > MAX_MESSAGE_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message too large: {} bytes", len),
        ));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    let bytes = if flags & FLAG_COMPRESSED != 0 {
        zstd::decode_all(&buf[..]).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
    } else {
        buf
    };
    bincode::deserialize(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub async fn write_message<W>(writer: &mut W, msg: &Message, compress: bool) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let bytes =
        bincode::serialize(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let (payload, flags) = if compress && bytes.len() > COMPRESS_THRESHOLD {
        let c = zstd::encode_all(&bytes[..], COMPRESS_LEVEL)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
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
    writer.flush().await?;
    Ok(())
}
