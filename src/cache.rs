//! Persistent (size, mtime) → blake3 hash cache.
//!
//! Walking a repo and hashing every file is the slowest part of a sync.
//! On re-runs, almost every file is unchanged, so we cache the hash keyed
//! by (size, mtime). Cache lives in the platform's user-cache directory.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default, Serialize, Deserialize)]
pub struct HashCache {
    entries: HashMap<PathBuf, CacheEntry>,
    /// Runtime-only: an unchanged walk should not rewrite the whole cache.
    #[serde(skip)]
    dirty: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheEntry {
    pub size: u64,
    pub mtime: i64,
    pub hash: [u8; 32],
}

impl HashCache {
    pub fn load(root: &Path) -> Self {
        let Some(path) = cache_path_for(root) else {
            return Self::default();
        };
        match fs::read(&path) {
            Ok(bytes) => postcard::from_bytes(&bytes).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&mut self, root: &Path) {
        if !self.dirty {
            return;
        }
        let Some(path) = cache_path_for(root) else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(bytes) = postcard::to_allocvec(self) {
            if fs::write(&path, bytes).is_ok() {
                self.dirty = false;
            }
        }
    }

    pub fn lookup(&self, rel: &Path, size: u64, mtime: i64) -> Option<[u8; 32]> {
        self.entries
            .get(rel)
            .filter(|e| e.size == size && e.mtime == mtime)
            .map(|e| e.hash)
    }

    pub fn record(&mut self, rel: &Path, size: u64, mtime: i64, hash: [u8; 32]) {
        let next = CacheEntry { size, mtime, hash };
        if self.entries.get(rel) == Some(&next) {
            return;
        }
        self.entries.insert(rel.to_path_buf(), next);
        self.dirty = true;
    }
}

fn cache_path_for(root: &Path) -> Option<PathBuf> {
    let base = dirs::cache_dir()?.join("synx");
    let mut h = blake3::Hasher::new();
    h.update(root.as_os_str().as_encoded_bytes());
    let id = h.finalize().to_hex();
    Some(base.join(format!("{}.cache", &id.as_str()[..16])))
}

#[cfg(test)]
mod tests {
    use super::HashCache;
    use std::path::Path;

    #[test]
    fn unchanged_records_do_not_dirty_cache() {
        let mut cache = HashCache::default();
        let hash = [7; 32];
        cache.record(Path::new("src/main.rs"), 42, 123, hash);
        assert!(cache.dirty);

        // dirty is runtime-only and therefore resets after a persisted cache
        // is loaded; recording the same manifest entry must keep it clean.
        let bytes = postcard::to_allocvec(&cache).unwrap();
        let mut loaded: HashCache = postcard::from_bytes(&bytes).unwrap();
        assert!(!loaded.dirty);
        loaded.record(Path::new("src/main.rs"), 42, 123, hash);
        assert!(!loaded.dirty);

        loaded.record(Path::new("src/main.rs"), 43, 124, [8; 32]);
        assert!(loaded.dirty);
    }
}
