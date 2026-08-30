use super::{Baseline, LiveBaseline};
use crate::protocol::{Entry, EntryKind};
use std::collections::HashMap;
use std::path::PathBuf;
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
    live.persist_now();
    assert!(!live.inner.lock().unwrap().dirty);
    assert!(Baseline::load_from_path(&path)
        .get(&changed.path)
        .unwrap()
        .same_content(&changed));

    live.remove(&changed.path);
    live.persist_now();
    assert!(Baseline::load_from_path(&path).is_empty());
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
