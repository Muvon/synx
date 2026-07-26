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
        Self::load_from_path(&path)
    }

    fn load_from_path(path: &Path) -> Self {
        match fs::read(path) {
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
        self.save_to_path(&path);
    }

    fn save_to_path(&mut self, path: &Path) {
        if !self.dirty {
            return;
        }
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(bytes) = postcard::to_allocvec(self) {
            if fs::write(path, bytes).is_ok() {
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
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("synx-cache-{label}-{}-{nonce}", std::process::id()))
    }

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

    #[test]
    fn persists_loads_and_recovers_from_missing_or_corrupt_cache() {
        let path = temp_path("roundtrip");
        let hash = [5; 32];
        let mut cache = HashCache::default();
        cache.record(Path::new("file"), 7, 11, hash);
        cache.save_to_path(&path);
        assert!(!cache.dirty);
        assert!(path.is_file());

        let loaded = HashCache::load_from_path(&path);
        assert_eq!(loaded.lookup(Path::new("file"), 7, 11), Some(hash));
        assert_eq!(loaded.lookup(Path::new("file"), 8, 11), None);
        assert_eq!(loaded.lookup(Path::new("missing"), 7, 11), None);

        fs::write(&path, b"corrupt").unwrap();
        assert!(HashCache::load_from_path(&path).entries.is_empty());
        fs::remove_file(&path).unwrap();
        assert!(HashCache::load_from_path(&path).entries.is_empty());
    }

    #[test]
    fn failed_save_remains_dirty_for_retry() {
        let path = temp_path("failure");
        fs::create_dir(&path).unwrap();
        let mut cache = HashCache::default();
        cache.record(Path::new("file"), 1, 1, [1; 32]);
        cache.save_to_path(&path);
        assert!(cache.dirty);
        fs::remove_dir(path).unwrap();
    }
}
