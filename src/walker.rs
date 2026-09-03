use anyhow::{Context, Result};
use ignore::{WalkBuilder, WalkState};
use notify_debouncer_full::file_id::FileId;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::cache::HashCache;
use crate::paths::{is_internal_temp, resolve_beneath};
use crate::protocol::{Entry, EntryKind};
use crate::watcher::IdCache;

/// Files above this size are hashed via mmap + rayon (parallel across cores).
/// Below it, the mmap setup cost outweighs the parallelism win.
const MMAP_HASH_THRESHOLD: u64 = 1024 * 1024; // 1 MiB

/// Hash a single file with blake3.
///
/// - Small files (<1 MiB): streaming `io::copy` — minimal overhead.
/// - Larger files: memory-mapped + rayon-parallel — saturates multiple cores
///   on a single file. blake3 hits ~1 GB/s/core; an 8-core box hashes a
///   1 GiB file in ~125 ms with this path vs ~1 s sequential.
pub fn hash_file(path: &Path) -> std::io::Result<[u8; 32]> {
    let len = fs::metadata(path)?.len();
    hash_file_with_len(path, len)
}

/// Hash with metadata the caller already obtained. The manifest walker has
/// just stat'd every entry, so asking the kernel for the size again adds one
/// syscall per cache miss for no new information.
fn hash_file_with_len(path: &Path, len: u64) -> std::io::Result<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    if len >= MMAP_HASH_THRESHOLD {
        hasher.update_mmap_rayon(path)?;
    } else {
        let mut file = fs::File::open(path)?;
        std::io::copy(&mut file, &mut hasher)?;
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Identity the watcher uses to pair the two halves of a rename.
pub fn file_id(meta: &fs::Metadata) -> FileId {
    FileId::new_inode(meta.dev(), meta.ino())
}

/// Build a configured walker. Respects in-tree .gitignore at all levels,
/// plus .synxignore — and nothing else.
///
/// Machine-local rules (global gitignore, .git/info/exclude, `.ignore`
/// files, ignore files in directories above the root) must NOT shape the
/// manifest: the peer can't see them and `IgnoreStack` doesn't model them,
/// so a file they match vanishes from this side's manifest with no
/// ManifestExcluded marker — which the three-way diff reads as
/// baseline-proven deletion evidence and destroys the peer's live copy.
pub fn build_walker(root: &Path) -> ignore::WalkBuilder {
    let mut b = WalkBuilder::new(root);
    b.standard_filters(false)
        .hidden(false)
        .git_ignore(true)
        .require_git(false)
        .follow_links(false);
    b.add_custom_ignore_filename(".synxignore");
    // We do NOT hardcode-skip anything (including `.git/`). Dotfiles are
    // synced unless the user explicitly puts them in `.gitignore` or
    // `.synxignore`. Our own atomic-write tmps live in `$TMPDIR/synx/`
    // and never appear under the sync root.
    b
}

/// Compute an Entry for a path relative to `root`.
/// Returns Ok(None) if the path doesn't exist.
pub fn build_entry(root: &Path, rel: &Path) -> std::io::Result<Option<Entry>> {
    let full = resolve_beneath(root, rel)?;
    let meta = match fs::symlink_metadata(&full) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    Ok(build_entry_from_metadata(rel, &full, meta, None)?.map(|built| built.entry))
}

struct BuiltEntry {
    entry: Entry,
    cache_miss: bool,
    /// For the watcher's rename pairing — free here, the walk already holds
    /// the metadata.
    id: FileId,
}

/// Build an entry from metadata already returned by the directory walker.
fn build_entry_from_metadata(
    rel: &Path,
    full: &Path,
    meta: fs::Metadata,
    cache: Option<&HashCache>,
) -> std::io::Result<Option<BuiltEntry>> {
    let id = file_id(&meta);
    let ft = meta.file_type();
    let kind = if ft.is_symlink() {
        EntryKind::Symlink
    } else if ft.is_dir() {
        EntryKind::Dir
    } else if ft.is_file() {
        EntryKind::File
    } else {
        return Ok(None); // sockets, FIFOs, etc.
    };
    let mtime = meta
        .mtime()
        .saturating_mul(1_000_000_000)
        .saturating_add(meta.mtime_nsec());
    let mode = meta.permissions().mode();
    let size = if matches!(kind, EntryKind::File) {
        meta.len()
    } else {
        0
    };

    let (hash, cache_miss) = if matches!(kind, EntryKind::File) {
        let cached = cache.and_then(|c| c.lookup(rel, size, mtime));
        match cached {
            Some(h) => (h, false),
            None => (hash_file_with_len(full, size)?, cache.is_some()),
        }
    } else {
        ([0u8; 32], false)
    };

    let link_target = if matches!(kind, EntryKind::Symlink) {
        fs::read_link(full).ok()
    } else {
        None
    };
    Ok(Some(BuiltEntry {
        entry: Entry {
            path: rel.to_path_buf(),
            kind,
            size,
            mtime,
            mode,
            hash,
            link_target,
        },
        cache_miss,
        id,
    }))
}

/// Walk `root` in parallel (multi-threaded via `ignore`), returning a
/// fully-hashed manifest sorted by path plus the subtree prefixes the walk
/// deliberately excluded. The cache is updated in-place; the caller should
/// call `HashCache::save` afterwards.
///
/// If git is mid-operation (rebase / merge / cherry-pick / pending ref
/// lock — see `peer::git_busy`), `.git/` is excluded from the walk and
/// returned as an excluded prefix. The peer MUST be told
/// (Message::ManifestExcluded) so it treats the missing entries as
/// "paused", never as deletions — otherwise the plan deletes the peer's
/// live repo files.
///
/// `ids`, when given, is seeded with every walked path's file id so the
/// watcher can pair renames without a walk of its own.
pub fn walk_manifest(
    root: &Path,
    cache: &mut HashCache,
    ids: Option<&IdCache>,
) -> Result<(Vec<Entry>, Vec<PathBuf>)> {
    // Each visitor accumulates locally and sends one batch when its worker
    // exits. This avoids both a cache mutex and one channel synchronization
    // per path in the hot parallel walk.
    let (tx, rx) = mpsc::channel::<Vec<BuiltEntry>>();
    let skip_git = crate::peer::git_busy(root);
    if skip_git {
        tracing::info!("git operation in progress — excluding .git/ from this walk");
    }
    let cache_read: &HashCache = cache;

    build_walker(root).build_parallel().run(|| {
        struct Batch {
            entries: Vec<BuiltEntry>,
            tx: mpsc::Sender<Vec<BuiltEntry>>,
        }
        impl Drop for Batch {
            fn drop(&mut self) {
                if !self.entries.is_empty() {
                    let _ = self.tx.send(std::mem::take(&mut self.entries));
                }
            }
        }

        let mut batch = Batch {
            entries: Vec::with_capacity(1024),
            tx: tx.clone(),
        };
        Box::new(move |result| {
            let dent = match result {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!("walk: {e}");
                    return WalkState::Continue;
                }
            };
            let path = dent.path();
            if path == root {
                return WalkState::Continue;
            }
            if is_internal_temp(path) {
                return WalkState::Continue;
            }
            let rel = match path.strip_prefix(root) {
                Ok(r) => r,
                Err(_) => return WalkState::Continue,
            };
            if skip_git && crate::peer::is_under_git(rel) {
                // Skip the .git entry itself AND all descendants.
                return WalkState::Skip;
            }
            // `DirEntry::metadata` is symlink-aware when follow_links=false,
            // matching symlink_metadata without a second stat in build_entry.
            let meta = match dent.metadata() {
                Ok(meta) => meta,
                Err(e) => {
                    tracing::warn!("entry {}: {e}", rel.display());
                    return WalkState::Continue;
                }
            };
            match build_entry_from_metadata(rel, path, meta, Some(cache_read)) {
                Ok(Some(e)) => {
                    batch.entries.push(e);
                }
                Ok(None) => {}
                Err(e) => tracing::warn!("entry {}: {e}", rel.display()),
            }
            WalkState::Continue
        })
    });

    drop(tx);
    let mut built: Vec<BuiltEntry> = rx.into_iter().flatten().collect();
    built.sort_by(|a, b| a.entry.path.cmp(&b.entry.path));
    // Update misses after the parallel read-only phase. Cache hits remain a
    // shared immutable HashMap lookup and never contend on a global lock.
    for result in &built {
        if result.cache_miss {
            let entry = &result.entry;
            cache.record(&entry.path, entry.size, entry.mtime, entry.hash);
        }
    }
    if let Some(ids) = ids {
        ids.seed(built.iter().map(|b| (root.join(&b.entry.path), b.id)));
    }
    let excluded = if skip_git {
        vec![PathBuf::from(".git")]
    } else {
        Vec::new()
    };
    Ok((
        built.into_iter().map(|result| result.entry).collect(),
        excluded,
    ))
}

/// Ensure root is (or becomes) a directory we can walk.
pub fn ensure_root(path: &Path) -> Result<PathBuf> {
    if !path.exists() {
        fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))?;
    }
    let canon =
        fs::canonicalize(path).with_context(|| format!("canonicalize {}", path.display()))?;
    if !canon.is_dir() {
        anyhow::bail!("{} is not a directory", canon.display());
    }
    Ok(canon)
}

#[cfg(test)]
#[path = "walker_tests.rs"]
mod tests;
