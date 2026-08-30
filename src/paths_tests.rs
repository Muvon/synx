use super::resolve_beneath;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

struct TestDir(std::path::PathBuf);

impl TestDir {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("synx-path-test-{}-{nonce}", std::process::id()));
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
fn confines_paths_to_root() {
    let root = TestDir::new();
    fs::create_dir(root.0.join("dir")).unwrap();
    assert_eq!(
        resolve_beneath(&root.0, Path::new("dir/file")).unwrap(),
        root.0.join("dir/file")
    );

    for unsafe_path in [
        "",
        ".",
        "..",
        "../outside",
        "/absolute",
        "dir/../../outside",
    ] {
        assert!(resolve_beneath(&root.0, Path::new(unsafe_path)).is_err());
    }
}

#[test]
fn rejects_symlink_and_file_ancestors_but_allows_final_symlink() {
    let root = TestDir::new();
    let outside = TestDir::new();
    std::os::unix::fs::symlink(&outside.0, root.0.join("link")).unwrap();
    assert!(resolve_beneath(&root.0, Path::new("link/escape")).is_err());
    assert!(resolve_beneath(&root.0, Path::new("link")).is_ok());

    fs::write(root.0.join("file"), b"x").unwrap();
    assert!(resolve_beneath(&root.0, Path::new("file/child")).is_err());
}
