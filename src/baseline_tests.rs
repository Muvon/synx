use super::{Baseline, LiveBaseline};
use crate::protocol::{Entry, EntryKind};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_path(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "synx-baseline-{label}-{}-{nonce}",
        std::process::id()
    ))
}

fn entry(mtime: i64, hash: [u8; 32]) -> Entry {
    Entry {
        path: PathBuf::from("file.txt"),
        kind: EntryKind::File,
        size: 4,
        mtime,
        mode: 0o644,
        hash,
        link_target: None,
    }
}

#[test]
fn has_git_requires_git_entries() {
    assert!(!Baseline::default().has_git());
    assert!(!Baseline::from_entries([entry(1, [1; 32])]).has_git());
    let mut git = entry(1, [1; 32]);
    git.path = PathBuf::from(".git/HEAD");
    assert!(Baseline::from_entries([git]).has_git());
}

#[test]
fn baseline_equality_tracks_content_not_metadata() {
    let previous_entry = entry(1, [3; 32]);
    let baseline = Baseline {
        entries: HashMap::from([(previous_entry.path.clone(), previous_entry)]),
    };

    let metadata_only = entry(2, [3; 32]);
    assert!(baseline.matches(&HashMap::from([(
        metadata_only.path.clone(),
        metadata_only,
    )])));

    let changed = entry(2, [4; 32]);
    assert!(!baseline.matches(&HashMap::from([(changed.path.clone(), changed)])));
}

#[test]
fn persists_mutations_and_skips_semantically_unchanged_state() {
    let path = temp_path("roundtrip");
    let original = entry(1, [3; 32]);
    let previous = Baseline::default();
    let live = LiveBaseline::seed_to_path(
        Some(path.clone()),
        HashMap::from([(original.path.clone(), original.clone())]),
        &previous,
    );
    assert!(path.is_file());
    assert!(Baseline::load_from_path(&path)
        .get(&original.path)
        .is_some());

    let metadata_only = entry(2, [3; 32]);
    live.set(metadata_only);
    assert!(!live.inner.lock().unwrap().dirty);

    let changed = entry(3, [4; 32]);
    live.set(changed.clone());
    assert!(live.inner.lock().unwrap().dirty);
    assert!(live.begin_barrier());
    live.commit_barrier();
    assert!(!live.inner.lock().unwrap().dirty);
    assert!(Baseline::load_from_path(&path)
        .get(&changed.path)
        .unwrap()
        .same_content(&changed));

    live.remove(&changed.path);
    assert!(live.begin_barrier());
    live.commit_barrier();
    assert!(Baseline::load_from_path(&path).is_empty());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn live_updates_reach_disk_only_once_the_peer_confirms_them() {
    // A send reaches disk only after the peer confirms it. Recorded earlier,
    // a dropped link turns it into deletion evidence: the file is here,
    // absent there, and matches the baseline — which the next session reads
    // as a peer delete and applies to our only copy.
    let path = temp_path("barrier");
    let seeded = entry(1, [1; 32]);
    let live = LiveBaseline::seed_to_path(
        Some(path.clone()),
        HashMap::from([(seeded.path.clone(), seeded)]),
        &Baseline::default(),
    );

    let mut sent = entry(2, [7; 32]);
    sent.path = PathBuf::from(".git/objects/ee/769543");
    live.set(sent.clone());
    assert!(Baseline::load_from_path(&path).get(&sent.path).is_none());

    assert!(live.begin_barrier());
    // One barrier in flight at a time, and nothing on disk until it returns.
    assert!(!live.begin_barrier());
    assert!(Baseline::load_from_path(&path).get(&sent.path).is_none());

    // Sent after the Ping, so the Pong says nothing about it: it must not
    // ride along into the file the barrier authorizes.
    let mut later = entry(3, [9; 32]);
    later.path = PathBuf::from(".git/objects/c2/518b51");
    live.set(later.clone());

    live.commit_barrier();
    let stored = Baseline::load_from_path(&path);
    assert!(stored.get(&sent.path).is_some());
    assert!(stored.get(&later.path).is_none());

    // The next barrier picks up what the last one left behind.
    assert!(live.begin_barrier());
    live.commit_barrier();
    assert!(Baseline::load_from_path(&path).get(&later.path).is_some());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn a_failed_peer_apply_voids_the_claim_it_appears_in() {
    // The peer reported it could not write this path. It doesn't hold our
    // version, so neither the entry nor the in-flight snapshot claiming it
    // may survive — that claim is exactly what deletes our copy later.
    let path = temp_path("forget");
    let converged = entry(1, [1; 32]);
    let live = LiveBaseline::seed_to_path(
        Some(path.clone()),
        HashMap::from([(converged.path.clone(), converged.clone())]),
        &Baseline::default(),
    );

    let mut rejected = entry(2, [5; 32]);
    rejected.path = PathBuf::from("read-only.txt");
    live.set(rejected.clone());
    assert!(live.begin_barrier());
    live.forget(&rejected.path);
    // The outstanding reply still belongs to the voided claim: starting a new
    // barrier here would let that older reply commit a newer snapshot.
    assert!(!live.begin_barrier());
    live.commit_barrier();

    // The voided snapshot wrote nothing, so the file still holds the seed.
    let stored = Baseline::load_from_path(&path);
    assert!(stored
        .get(&converged.path)
        .is_some_and(|e| e.same_content(&converged)));

    // And the next barrier ships the corrected state, not the rejected one.
    assert!(live.begin_barrier());
    live.commit_barrier();
    assert!(Baseline::load_from_path(&path)
        .get(&rejected.path)
        .is_none());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn disabled_unchanged_corrupt_and_failed_storage_are_safe() {
    let disabled = LiveBaseline::disabled();
    disabled.set(entry(1, [1; 32]));
    disabled.remove(PathBuf::from("file.txt").as_path());
    disabled.persist_now();

    let previous_entry = entry(1, [2; 32]);
    let previous = Baseline::from_entries([previous_entry.clone()]);
    let untouched = temp_path("unchanged");
    let _live = LiveBaseline::seed_to_path(
        Some(untouched.clone()),
        HashMap::from([(previous_entry.path.clone(), previous_entry)]),
        &previous,
    );
    assert!(!untouched.exists());

    let corrupt = temp_path("corrupt");
    std::fs::write(&corrupt, b"not postcard").unwrap();
    assert!(Baseline::load_from_path(&corrupt).is_empty());
    std::fs::remove_file(corrupt).unwrap();

    let failure = temp_path("failure");
    std::fs::create_dir(&failure).unwrap();
    let live = LiveBaseline::seed_to_path(
        Some(failure.clone()),
        HashMap::from([(PathBuf::from("file.txt"), entry(1, [9; 32]))]),
        &Baseline::default(),
    );
    assert!(live.inner.lock().unwrap().dirty);
    std::fs::remove_dir(failure).unwrap();
}

#[test]
fn rename_rekeys_the_subtree_and_leaves_siblings() {
    let path = temp_path("rename");
    let mut dir = entry(1, [0; 32]);
    dir.path = PathBuf::from("dir");
    dir.kind = EntryKind::Dir;
    let mut child = entry(1, [2; 32]);
    child.path = PathBuf::from("dir/child");
    let mut sibling = entry(1, [3; 32]);
    sibling.path = PathBuf::from("dir.txt");
    let entries = HashMap::from([
        (dir.path.clone(), dir),
        (child.path.clone(), child),
        (sibling.path.clone(), sibling),
    ]);
    let live = LiveBaseline::seed_to_path(Some(path.clone()), entries, &Baseline::default());
    let keys = |live: &LiveBaseline| {
        live.with_entries(|e| {
            let mut keys: Vec<PathBuf> = e.keys().cloned().collect();
            keys.sort();
            keys
        })
    };

    live.rename(Path::new("dir"), Path::new("moved"));
    assert_eq!(
        keys(&live),
        ["dir.txt", "moved", "moved/child"].map(PathBuf::from)
    );
    let child = live
        .with_entries(|e| e.get(Path::new("moved/child")).cloned())
        .unwrap();
    assert_eq!(child.path, Path::new("moved/child"));
    assert_eq!(child.hash, [2; 32]);

    // Unknown source: no-op. Then the re-keyed state is what persists.
    live.rename(Path::new("nope"), Path::new("x"));
    assert_eq!(keys(&live).len(), 3);
    live.persist_now();
    let reloaded = Baseline::load_from_path(&path);
    assert!(reloaded.get(Path::new("moved/child")).is_some());
    assert!(reloaded.get(Path::new("dir/child")).is_none());
    let _ = std::fs::remove_file(&path);

    let off = LiveBaseline::disabled();
    off.rename(Path::new("dir"), Path::new("moved"));
    assert!(off.is_empty());
    assert!(!live.is_empty());
}
