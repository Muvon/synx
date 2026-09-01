use super::IgnoreStack;
use crate::cache::HashCache;
use crate::walker::walk_manifest;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn builds_nested_matchers_from_the_manifest_without_another_walk() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("synx-ignore-test-{}-{nonce}", std::process::id()));
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::create_dir_all(root.join("other")).unwrap();
    fs::write(root.join(".gitignore"), "/root-only\n").unwrap();
    fs::write(root.join("nested/.gitignore"), "*.tmp\n").unwrap();
    fs::write(root.join("root-only"), "ignored").unwrap();
    fs::write(root.join("nested/cache.tmp"), "ignored").unwrap();
    fs::write(root.join("nested/keep.txt"), "kept").unwrap();
    fs::write(root.join("other/cache.tmp"), "kept").unwrap();

    let (manifest, _) = walk_manifest(&root, &mut HashCache::default()).unwrap();
    assert!(manifest
        .iter()
        .any(|entry| entry.path == Path::new("nested/.gitignore")));
    assert!(!manifest
        .iter()
        .any(|entry| entry.path == Path::new("nested/cache.tmp")));
    let ignores = IgnoreStack::from_manifest(&root, &manifest);

    assert!(ignores.is_ignored_rel(Path::new("root-only"), false));
    assert!(ignores.is_ignored_rel(Path::new("nested/cache.tmp"), false));
    assert!(!ignores.is_ignored_rel(Path::new("other/cache.tmp"), false));

    fs::remove_dir_all(root).unwrap();
}
