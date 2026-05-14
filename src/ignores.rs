//! A stack of per-directory `.gitignore` / `.synxignore` matchers, loaded
//! once at the start of a session and used to test arbitrary paths.
//!
//! The walker (`walker.rs`) already respects gitignore via `ignore::WalkBuilder`
//! when *building* the manifest. This module exists for the cases where we
//! receive a path from outside (the remote manifest, or a watcher event) and
//! need to ask "would we have walked this path?".

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::{Match, WalkBuilder};
use std::path::{Path, PathBuf};

pub struct IgnoreStack {
    root: PathBuf,
    /// Sorted by directory depth (shallow → deep). Deeper matchers override.
    matchers: Vec<(PathBuf, Gitignore)>,
}

impl IgnoreStack {
    pub fn load(root: &Path) -> Self {
        let mut matchers: Vec<(PathBuf, Gitignore)> = Vec::new();

        // Root-level: combine .gitignore + .synxignore.
        let mut b = GitignoreBuilder::new(root);
        let _ = b.add(root.join(".gitignore"));
        let _ = b.add(root.join(".synxignore"));
        if let Ok(gi) = b.build() {
            matchers.push((root.to_path_buf(), gi));
        }

        // Nested .gitignore / .synxignore files (use a walker that respects
        // the outer ignore rules so we don't descend into already-ignored dirs).
        let walker = WalkBuilder::new(root)
            .standard_filters(true)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .require_git(false)
            .build();
        for dent in walker.flatten() {
            let name = dent.file_name();
            if name != ".gitignore" && name != ".synxignore" {
                continue;
            }
            let dir = match dent.path().parent() {
                Some(p) => p.to_path_buf(),
                None => continue,
            };
            if dir == root {
                continue;
            }
            let mut b = GitignoreBuilder::new(&dir);
            // `GitignoreBuilder::add` returns Option<Error>; None means OK.
            if b.add(dent.path()).is_some() {
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
    pub fn is_ignored_abs(&self, abs: &Path, is_dir: bool) -> bool {
        let mut ignored = false;
        for (dir, gi) in &self.matchers {
            if let Ok(rel) = abs.strip_prefix(dir) {
                match gi.matched(rel, is_dir) {
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
