use anyhow::{Context, Result};
use ignore::{WalkBuilder, WalkState};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::cache::HashCache;
use crate::protocol::{Entry, EntryKind};

/// Hash a single file with blake3 (streaming, ~1 GB/s on modern CPUs).
pub fn hash_file(path: &Path) -> std::io::Result<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    let mut file = fs::File::open(path)?;
    std::io::copy(&mut file, &mut hasher)?;
    Ok(*hasher.finalize().as_bytes())
}

/// Build a configured walker. Respects .gitignore at all levels, plus .synxignore.
/// `.git/` directories are always skipped.
pub fn build_walker(root: &Path) -> ignore::WalkBuilder {
    let mut b = WalkBuilder::new(root);
    b.standard_filters(true)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .ignore(true)
        .parents(true)
        .require_git(false)
        .follow_links(false);
    b.add_custom_ignore_filename(".synxignore");
    // We do NOT hardcode-skip `.git/`. Dotfiles & `.git` are synced unless the
    // user explicitly puts them in `.gitignore` or `.synxignore`. The ignore
    // crate respects user rules; we add none of our own.
    b
}

/// Compute an Entry for a path relative to `root`, consulting the cache for
/// the (size, mtime) → hash mapping. Returns Ok(None) if the path doesn't exist.
pub fn build_entry(
    root: &Path,
    rel: &Path,
    cache: Option<&Mutex<HashCache>>,
) -> std::io::Result<Option<Entry>> {
    let full = root.join(rel);
    let meta = match fs::symlink_metadata(&full) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
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
        .saturating_add(meta.mtime_nsec() as i64);
    let mode = meta.permissions().mode();
    let size = if matches!(kind, EntryKind::File) {
        meta.len()
    } else {
        0
    };

    let hash = if matches!(kind, EntryKind::File) {
        let cached = cache.and_then(|m| m.lock().ok().and_then(|g| g.lookup(rel, size, mtime)));
        match cached {
            Some(h) => h,
            None => {
                let h = hash_file(&full)?;
                if let Some(m) = cache {
                    if let Ok(mut g) = m.lock() {
                        g.insert(rel.to_path_buf(), size, mtime, h);
                    }
                }
                h
            }
        }
    } else {
        [0u8; 32]
    };

    let link_target = if matches!(kind, EntryKind::Symlink) {
        fs::read_link(&full).ok()
    } else {
        None
    };
    Ok(Some(Entry {
        path: rel.to_path_buf(),
        kind,
        size,
        mtime,
        mode,
        hash,
        link_target,
    }))
}

/// Walk `root` in parallel (multi-threaded via `ignore`), returning a
/// fully-hashed manifest sorted by path. The cache is updated in-place; the
/// caller should call `HashCache::save` afterwards.
pub fn walk_manifest(root: &Path, cache: &Arc<Mutex<HashCache>>) -> Result<Vec<Entry>> {
    let (tx, rx) = std::sync::mpsc::channel::<Entry>();
    let root_arc = Arc::new(root.to_path_buf());

    build_walker(root).build_parallel().run(|| {
        let tx = tx.clone();
        let root = root_arc.clone();
        let cache = cache.clone();
        Box::new(move |result| {
            let dent = match result {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!("walk: {e}");
                    return WalkState::Continue;
                }
            };
            let path = dent.path();
            if path == root.as_path() {
                return WalkState::Continue;
            }
            let rel = match path.strip_prefix(root.as_path()) {
                Ok(r) => r.to_path_buf(),
                Err(_) => return WalkState::Continue,
            };
            match build_entry(&root, &rel, Some(&*cache)) {
                Ok(Some(e)) => {
                    let _ = tx.send(e);
                }
                Ok(None) => {}
                Err(e) => tracing::warn!("entry {}: {e}", rel.display()),
            }
            WalkState::Continue
        })
    });

    drop(tx);
    let mut entries: Vec<Entry> = rx.iter().collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
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
