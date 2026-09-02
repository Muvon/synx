use super::*;
use crate::paths::INTERNAL_TMP_PREFIX;
use std::time::{SystemTime, UNIX_EPOCH};

struct TestDir(PathBuf);

impl TestDir {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "synx-walker-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn hashes_streamed_and_mmap_files_and_reports_missing_files() {
    let root = TestDir::new("hash");
    let small = root.0.join("small");
    fs::write(&small, b"small").unwrap();
    assert_eq!(
        hash_file(&small).unwrap(),
        *blake3::hash(b"small").as_bytes()
    );

    let large_content = vec![0x42; MMAP_HASH_THRESHOLD as usize];
    let large = root.0.join("large");
    fs::write(&large, &large_content).unwrap();
    assert_eq!(
        hash_file(&large).unwrap(),
        *blake3::hash(&large_content).as_bytes()
    );
    assert!(hash_file(&root.0.join("missing")).is_err());
}

#[test]
fn builds_entries_for_files_directories_and_symlinks() {
    let root = TestDir::new("entries");
    fs::create_dir(root.0.join("dir")).unwrap();
    fs::write(root.0.join("file"), b"content").unwrap();
    std::os::unix::fs::symlink("file", root.0.join("link")).unwrap();

    let dir = build_entry(&root.0, Path::new("dir")).unwrap().unwrap();
    assert_eq!(dir.kind, EntryKind::Dir);
    let file = build_entry(&root.0, Path::new("file")).unwrap().unwrap();
    assert_eq!(file.kind, EntryKind::File);
    assert_eq!(file.hash, *blake3::hash(b"content").as_bytes());
    let link = build_entry(&root.0, Path::new("link")).unwrap().unwrap();
    assert_eq!(link.kind, EntryKind::Symlink);
    assert_eq!(link.link_target, Some(PathBuf::from("file")));
    assert!(build_entry(&root.0, Path::new("missing"))
        .unwrap()
        .is_none());
    assert!(build_entry(&root.0, Path::new("../escape")).is_err());
}

#[test]
fn walks_once_with_ignores_internal_temps_and_reusable_cache() {
    let root = TestDir::new("manifest");
    fs::create_dir(root.0.join("nested")).unwrap();
    fs::write(root.0.join(".synxignore"), "*.ignored\n").unwrap();
    fs::write(root.0.join("kept"), b"one").unwrap();
    fs::write(root.0.join("nested/drop.ignored"), b"two").unwrap();
    fs::write(
        root.0.join(format!("{INTERNAL_TMP_PREFIX}orphan")),
        b"temporary",
    )
    .unwrap();

    let mut cache = HashCache::default();
    let (first, first_excluded) = walk_manifest(&root.0, &mut cache).unwrap();
    assert!(
        first_excluded.is_empty(),
        "no git activity — nothing excluded"
    );
    let listed: Vec<&Path> = first.iter().map(|entry| entry.path.as_path()).collect();
    assert!(listed.contains(&Path::new("kept")));
    assert!(listed.contains(&Path::new("nested")));
    assert!(!listed.contains(&Path::new("nested/drop.ignored")));
    assert!(!listed.iter().any(|path| is_internal_temp(path)));

    let encoded = postcard::to_allocvec(&cache).unwrap();
    let mut loaded: HashCache = postcard::from_bytes(&encoded).unwrap();
    let (second, _) = walk_manifest(&root.0, &mut loaded).unwrap();
    assert_eq!(first, second);
}

#[test]
fn walk_includes_git_when_idle_and_excludes_it_declared_when_busy() {
    let root = TestDir::new("gitbusy");
    fs::create_dir(root.0.join(".git")).unwrap();
    fs::write(root.0.join(".git/HEAD"), b"ref: refs/heads/master\n").unwrap();
    fs::write(root.0.join("src.rs"), b"code").unwrap();

    let (idle, idle_excluded) = walk_manifest(&root.0, &mut HashCache::default()).unwrap();
    assert!(idle_excluded.is_empty(), "no busy markers — .git/ syncs");
    assert!(idle.iter().any(|e| e.path == Path::new(".git/HEAD")));

    // A fresh index.lock marks git as mid-operation (see peer::git_busy).
    fs::write(root.0.join(".git/index.lock"), b"").unwrap();
    let (busy, busy_excluded) = walk_manifest(&root.0, &mut HashCache::default()).unwrap();
    assert_eq!(
        busy_excluded,
        [PathBuf::from(".git")],
        "fresh index.lock must pause .git/ and declare the exclusion"
    );
    assert!(
        !busy.iter().any(|e| crate::peer::is_under_git(&e.path)),
        "no .git/ entries while paused"
    );
    assert!(
        busy.iter().any(|e| e.path == Path::new("src.rs")),
        "working tree keeps syncing while .git/ is paused"
    );
}

#[test]
fn walk_never_honors_ignore_rules_outside_the_sync_root() {
    // Machine-local ignore state — a parent repo's .gitignore, the user's
    // global gitignore, .git/info/exclude — is invisible to the peer and to
    // IgnoreStack. If the walker honors it, a file that exists on disk
    // silently vanishes from the manifest with no ManifestExcluded marker,
    // and the peer reads that omission as baseline-proven deletion evidence
    // (see sync_tests::outside_ignore_rules_never_become_deletion_evidence).
    let parent = TestDir::new("outside-rules");
    fs::create_dir(parent.0.join(".git")).unwrap();
    fs::write(parent.0.join(".gitignore"), "*.log\n").unwrap();
    let root = parent.0.join("repo");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("app.log"), b"precious").unwrap();

    let (manifest, excluded) = walk_manifest(&root, &mut HashCache::default()).unwrap();
    assert!(excluded.is_empty(), "nothing deliberately paused");
    assert!(
        manifest.iter().any(|e| e.path == Path::new("app.log")),
        "existing file dropped from the manifest by a rule outside the sync root"
    );
}

#[test]
fn ensure_root_creates_and_rejects_invalid_roots() {
    let parent = TestDir::new("ensure");
    let missing = parent.0.join("new/root");
    let ensured = ensure_root(&missing).unwrap();
    assert!(ensured.is_absolute());
    assert!(ensured.is_dir());

    let file = parent.0.join("file");
    fs::write(&file, b"x").unwrap();
    assert!(ensure_root(&file).is_err());
}
