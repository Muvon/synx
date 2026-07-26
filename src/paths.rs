//! Filesystem path confinement for operations requested by the peer.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

pub const INTERNAL_TMP_PREFIX: &str = ".synx-tmp-";

pub fn is_internal_temp(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(INTERNAL_TMP_PREFIX))
}

/// Resolve a non-empty relative path beneath `root`, rejecting lexical
/// traversal and existing symlink ancestors. The final component may itself
/// be a symlink because deleting or replacing a symlink is a valid sync op.
pub fn resolve_beneath(root: &Path, relative: &Path) -> io::Result<PathBuf> {
    let mut names = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(name) => names.push(name),
            _ => return Err(unsafe_path(relative)),
        }
    }
    if names.is_empty() {
        return Err(unsafe_path(relative));
    }

    let mut resolved = root.to_path_buf();
    for (index, name) in names.iter().enumerate() {
        resolved.push(name);
        if index + 1 == names.len() {
            continue;
        }
        match fs::symlink_metadata(&resolved) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!(
                        "path escapes sync root through symlink ancestor: {}",
                        relative.display()
                    ),
                ));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(io::Error::new(
                    io::ErrorKind::NotADirectory,
                    format!("path ancestor is not a directory: {}", relative.display()),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(resolved)
}

fn unsafe_path(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("path must stay beneath sync root: {}", path.display()),
    )
}

#[cfg(test)]
mod tests {
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
}
