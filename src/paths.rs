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
#[path = "paths_tests.rs"]
mod tests;
