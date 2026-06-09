//! Persistent per-root baseline: the converged manifest from the last
//! successful sync.
//!
//! Used as the common ancestor in the three-way diff. From the two live
//! manifests alone, "the user deleted this file here" and "the peer created
//! this file there" are byte-for-byte indistinguishable — a stateless diff
//! can only ever pull the file back, which silently resurrects deletions.
//! The baseline records what both sides agreed on last time, so a path that
//! is now absent on one side can be classified: gone-and-unchanged-elsewhere
//! is a genuine deletion (propagate), anything else is kept (never lose data).
//!
//! Lives next to the hash cache in the user-cache dir, keyed by root path.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::protocol::Entry;

#[derive(Default, Serialize, Deserialize)]
pub struct Baseline {
    entries: HashMap<PathBuf, Entry>,
}

impl Baseline {
    /// Load the baseline for `root`. Any failure (missing, corrupt, older
    /// format) yields an empty baseline, which makes the three-way diff fall
    /// back to the conservative pull-back behavior — never a mass delete.
    pub fn load(root: &Path) -> Self {
        let Some(path) = baseline_path_for(root) else {
            return Self::default();
        };
        match fs::read(&path) {
            Ok(bytes) => postcard::from_bytes(&bytes).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, root: &Path) {
        let Some(path) = baseline_path_for(root) else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(bytes) = postcard::to_allocvec(self) {
            let _ = fs::write(&path, bytes);
        }
    }

    pub fn from_map(entries: HashMap<PathBuf, Entry>) -> Self {
        Self { entries }
    }

    pub fn get(&self, path: &Path) -> Option<&Entry> {
        self.entries.get(path)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn baseline_path_for(root: &Path) -> Option<PathBuf> {
    let base = dirs::cache_dir()?.join("synx");
    let mut h = blake3::Hasher::new();
    h.update(root.as_os_str().as_encoded_bytes());
    let id = h.finalize().to_hex();
    Some(base.join(format!("{}.baseline", &id.as_str()[..16])))
}
