//! A stack of per-directory `.gitignore` / `.synxignore` matchers, loaded
//! once at the start of a session and used to test arbitrary paths.
//!
//! The walker (`walker.rs`) already respects gitignore via `ignore::WalkBuilder`
//! when *building* the manifest. This module exists for the cases where we
//! receive a path from outside (the remote manifest, or a watcher event) and
//! need to ask "would we have walked this path?".

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::Match;
use std::path::{Path, PathBuf};

use crate::protocol::Entry;

pub struct IgnoreStack {
    root: PathBuf,
    /// Sorted by directory depth (shallow → deep). Deeper matchers override.
    matchers: Vec<(PathBuf, Gitignore)>,
}

impl IgnoreStack {
    /// Build the arbitrary-path matchers from ignore files already discovered
    /// by the manifest walk. This avoids a separate full-tree discovery pass.
    pub fn from_manifest(root: &Path, manifest: &[Entry]) -> Self {
        let mut matchers: Vec<(PathBuf, Gitignore)> = Vec::new();

        // Root-level: combine .gitignore + .synxignore.
        let mut b = GitignoreBuilder::new(root);
        let _ = b.add(root.join(".gitignore"));
        let _ = b.add(root.join(".synxignore"));
        if let Ok(gi) = b.build() {
            matchers.push((root.to_path_buf(), gi));
        }

        // The configured manifest walker has already applied outer ignore
        // rules, so the ignore files it reports are exactly the relevant
        // nested matchers. Root files were combined above.
        for entry in manifest {
            let Some(name) = entry.path.file_name() else {
                continue;
            };
            if name != ".gitignore" && name != ".synxignore" {
                continue;
            }
            let full = root.join(&entry.path);
            let dir = match full.parent() {
                Some(p) => p.to_path_buf(),
                None => continue,
            };
            if dir == root {
                continue;
            }
            let mut b = GitignoreBuilder::new(&dir);
            // `GitignoreBuilder::add` returns Option<Error>; None means OK.
            if b.add(&full).is_some() {
                continue;
            }
            if let Ok(gi) = b.build() {
                matchers.push((dir, gi));
            }
        }

        matchers.sort_by_key(|(p, _)| p.components().count());
        Self {
            root: root.to_path_buf(),
            matchers,
        }
    }

    /// Test an absolute path. Honors only user-provided rules; dotfiles
    /// (including `.git/`) are NOT special-cased.
    ///
    /// Uses `matched_path_or_any_parents` so that a pattern like `/target`
    /// correctly ignores not just `target` itself but also every path
    /// underneath it (`target/debug/build/foo`, etc.). Plain `matched()`
    /// only checks the exact path string, which leaks descendants.
    pub fn is_ignored_abs(&self, abs: &Path, is_dir: bool) -> bool {
        let mut ignored = false;
        for (dir, gi) in &self.matchers {
            if let Ok(rel) = abs.strip_prefix(dir) {
                if rel.as_os_str().is_empty() {
                    continue;
                }
                match gi.matched_path_or_any_parents(rel, is_dir) {
                    Match::Ignore(_) => ignored = true,
                    Match::Whitelist(_) => ignored = false,
                    Match::None => {}
                }
            }
        }
        ignored
    }

    /// Test a path relative to the configured root.
    pub fn is_ignored_rel(&self, rel: &Path, is_dir: bool) -> bool {
        // Translate into absolute form for prefix-stripping consistency.
        let abs = self.root.join(rel);
        self.is_ignored_abs(&abs, is_dir)
    }
}

#[cfg(test)]
#[path = "ignores_tests.rs"]
mod tests;
