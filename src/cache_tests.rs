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
