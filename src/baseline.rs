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
//! `Baseline` is the read side, loaded at session start for the plan.
//! `LiveBaseline` is the write side: seeded with the converged manifest after
//! init sync, then kept current as the live loop applies/forwards changes, so
//! even a file created and deleted within one session is recorded correctly
//! and never resurrects. Both share one on-disk file (a bare path → Entry
//! map), keyed by root, living next to the hash cache in the user-cache dir.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::protocol::Entry;

/// At most one disk write per this interval while the live loop is churning.
/// A slightly stale baseline is safe — it only makes the next diff fall back
/// to the conservative pull-back for the un-persisted paths, never a wrong
/// delete — so debouncing trades a tiny resurrection window for far less IO.
const PERSIST_DEBOUNCE: Duration = Duration::from_secs(3);

/// Read side: the baseline as it was at the last sync. Empty on first run.
#[derive(Default)]
pub struct Baseline {
    entries: HashMap<PathBuf, Entry>,
}

impl Baseline {
    #[cfg(test)]
    pub(crate) fn from_entries(entries: impl IntoIterator<Item = Entry>) -> Self {
        Self {
            entries: entries
                .into_iter()
                .map(|entry| (entry.path.clone(), entry))
                .collect(),
        }
    }

    /// Any failure (missing, corrupt, older format) yields an empty baseline,
    /// which makes the three-way diff fall back to the conservative pull-back
    /// behavior — never a mass delete.
    pub fn load(root: &Path) -> Self {
        let Some(path) = baseline_path_for(root) else {
            return Self::default();
        };
        Self::load_from_path(&path)
    }

    fn load_from_path(path: &Path) -> Self {
        match fs::read(path) {
            Ok(bytes) => Self {
                entries: postcard::from_bytes(&bytes).unwrap_or_default(),
            },
            Err(_) => Self::default(),
        }
    }

    pub fn get(&self, path: &Path) -> Option<&Entry> {
        self.entries.get(path)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// True iff `.git/` was part of the last converged state — the evidence
    /// that lets a `.git`-less remote manifest be read as a deliberate wipe
    /// rather than "never synced".
    pub fn has_git(&self) -> bool {
        self.entries.keys().any(|p| crate::peer::is_under_git(p))
    }

    /// Entries under any of the given subtree prefixes from the last
    /// converged state. Carried forward into the reseeded baseline when a
    /// session paused those subtrees (see `run_session`), so deletion
    /// evidence survives the pause.
    pub fn entries_under<'a>(&'a self, prefixes: &'a [PathBuf]) -> impl Iterator<Item = &'a Entry> {
        self.entries
            .values()
            .filter(move |e| prefixes.iter().any(|prefix| e.path.starts_with(prefix)))
    }

    pub fn matches(&self, entries: &HashMap<PathBuf, Entry>) -> bool {
        self.entries.len() == entries.len()
            && self.entries.iter().all(|(path, previous)| {
                entries
                    .get(path)
                    .is_some_and(|current| previous.same_content(current))
            })
    }
}

/// Write side: a shared, mutable baseline kept current during a live session
/// and persisted (debounced) to the same file `Baseline::load` reads.
#[derive(Clone, Default)]
pub struct LiveBaseline {
    inner: Arc<Mutex<Inner>>,
    storage_path: Option<PathBuf>,
    /// Only the client owns a persistent baseline (it builds the plan). The
    /// agent gets a disabled one whose mutations and writes are no-ops.
    enabled: bool,
}

#[derive(Default)]
struct Inner {
    entries: HashMap<PathBuf, Entry>,
    dirty: bool,
    last_save: Option<Instant>,
    generation: u64,
}

impl LiveBaseline {
    /// Seed with the converged manifest and persist immediately, so even a
    /// `--once` run or an instant disconnect leaves a correct baseline.
    pub fn seed(root: PathBuf, entries: HashMap<PathBuf, Entry>, previous: &Baseline) -> Self {
        Self::seed_to_path(baseline_path_for(&root), entries, previous)
    }

    fn seed_to_path(
        storage_path: Option<PathBuf>,
        entries: HashMap<PathBuf, Entry>,
        previous: &Baseline,
    ) -> Self {
        let changed = !previous.matches(&entries);
        let lb = Self {
            inner: Arc::new(Mutex::new(Inner {
                entries,
                dirty: changed,
                last_save: None,
                generation: u64::from(changed),
            })),
            storage_path,
            enabled: true,
        };
        lb.persist_now();
        lb
    }

    /// A no-op baseline for the agent side (no planning, nothing to persist).
    pub fn disabled() -> Self {
        Self::default()
    }

    /// Record that `path` now holds `entry`'s content on both sides.
    pub fn set(&self, entry: Entry) {
        if !self.enabled {
            return;
        }
        if let Ok(mut g) = self.inner.lock() {
            if !g
                .entries
                .get(&entry.path)
                .is_some_and(|previous| previous.same_content(&entry))
            {
                g.entries.insert(entry.path.clone(), entry);
                g.dirty = true;
                g.generation = g.generation.wrapping_add(1);
            }
        }
        self.persist_due();
    }

    /// Record that `path` is now gone on both sides.
    pub fn remove(&self, path: &Path) {
        if !self.enabled {
            return;
        }
        if let Ok(mut g) = self.inner.lock() {
            if g.entries.remove(path).is_some() {
                g.dirty = true;
                g.generation = g.generation.wrapping_add(1);
            }
        }
        self.persist_due();
    }
    /// True when there is nothing to diff against: disabled (agent side) or
    /// no converged entries yet.
    pub fn is_empty(&self) -> bool {
        !self.enabled || self.inner.lock().map_or(true, |g| g.entries.is_empty())
    }

    /// Run `f` over the converged entries under the lock, so the
    /// reconciliation sweep can diff against them without cloning the map
    /// (see `peer::reconcile_sweep`). `f` must not call back into this
    /// baseline. Yields `T::default()` when disabled.
    pub fn with_entries<T: Default>(&self, f: impl FnOnce(&HashMap<PathBuf, Entry>) -> T) -> T {
        if !self.enabled {
            return T::default();
        }
        self.inner.lock().map(|g| f(&g.entries)).unwrap_or_default()
    }

    /// Persist if dirty and the debounce interval has elapsed.
    fn persist_due(&self) {
        self.write(false);
    }

    /// Persist unconditionally if dirty (called on clean live-loop exit).
    pub fn persist_now(&self) {
        self.write(true);
    }

    fn write(&self, force: bool) {
        if !self.enabled {
            return;
        }
        // Serialize under the lock, write outside it — keep the critical
        // section to a single in-memory pass, never an IO syscall.
        let (bytes, generation) = {
            let Ok(g) = self.inner.lock() else {
                return;
            };
            if !g.dirty {
                return;
            }
            let due = force
                || g.last_save
                    .map(|t| t.elapsed() >= PERSIST_DEBOUNCE)
                    .unwrap_or(true);
            if !due {
                return;
            }
            let Ok(bytes) = postcard::to_allocvec(&g.entries) else {
                return;
            };
            (bytes, g.generation)
        };
        let Some(path) = &self.storage_path else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if fs::write(path, &bytes).is_ok() {
            if let Ok(mut g) = self.inner.lock() {
                if g.generation == generation {
                    g.dirty = false;
                }
                g.last_save = Some(Instant::now());
            }
        }
    }
}

fn baseline_path_for(root: &Path) -> Option<PathBuf> {
    let base = dirs::cache_dir()?.join("synx");
    let mut h = blake3::Hasher::new();
    h.update(root.as_os_str().as_encoded_bytes());
    let id = h.finalize().to_hex();
    Some(base.join(format!("{}.baseline", &id.as_str()[..16])))
}

#[cfg(test)]
#[path = "baseline_tests.rs"]
mod tests;
