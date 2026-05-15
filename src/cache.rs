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
}

#[derive(Clone, Copy, Serialize, Deserialize)]
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
            // bincode 2 returns (value, bytes_consumed); we only want the value.
            // `config::legacy()` keeps the on-disk format compatible with what
            // bincode 1 wrote, so existing caches still decode.
            Ok(bytes) => bincode::serde::decode_from_slice(&bytes, bincode::config::legacy())
                .map(|(v, _)| v)
                .unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, root: &Path) {
        let Some(path) = cache_path_for(root) else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(bytes) = bincode::serde::encode_to_vec(self, bincode::config::legacy()) {
            let _ = fs::write(&path, bytes);
        }
    }

    pub fn lookup(&self, rel: &Path, size: u64, mtime: i64) -> Option<[u8; 32]> {
        self.entries
            .get(rel)
            .filter(|e| e.size == size && e.mtime == mtime)
            .map(|e| e.hash)
    }

    pub fn insert(&mut self, rel: PathBuf, size: u64, mtime: i64, hash: [u8; 32]) {
        self.entries.insert(rel, CacheEntry { size, mtime, hash });
    }
}

fn cache_path_for(root: &Path) -> Option<PathBuf> {
    let base = dirs::cache_dir()?.join("synx");
    let mut h = blake3::Hasher::new();
    h.update(root.as_os_str().as_encoded_bytes());
    let id = h.finalize().to_hex();
    Some(base.join(format!("{}.cache", &id.as_str()[..16])))
}
