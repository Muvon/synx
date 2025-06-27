// Copyright 2025 Muvon Un Limited
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Gitignore-aware glob pattern expansion utilities

use anyhow::{anyhow, Result};
use ignore::WalkBuilder;
use std::path::Path;

/// Maximum number of files allowed after glob expansion to prevent command line overflow
const MAX_EXPANDED_FILES: usize = 1000;

/// Expand glob patterns to actual file paths with gitignore and dotfile filtering
///
/// This function provides intelligent file expansion that:
/// - Respects .gitignore rules using the ignore crate
/// - Automatically excludes dotfiles (files/directories starting with '.')
/// - Applies glob pattern matching to filtered results
/// - Enforces file count limits to prevent system overload
///
/// # Arguments
/// * `patterns` - Array of glob patterns to expand
/// * `base_dir` - Base directory to search from (defaults to current directory)
///
/// # Returns
/// * `Ok(Vec<String>)` - List of expanded file paths
/// * `Err(anyhow::Error)` - If expansion fails or too many files found
pub fn expand_glob_patterns_filtered(
	patterns: &[String],
	base_dir: Option<&str>,
) -> Result<Vec<String>> {
	let mut expanded_paths = Vec::new();
	let search_dir = base_dir.unwrap_or(".");

	crate::log_debug!(
		"Expanding {} glob patterns from directory '{}': {:?}",
		patterns.len(),
		search_dir,
		patterns
	);

	// Build ignore walker that respects .gitignore and excludes dotfiles
	let mut builder = WalkBuilder::new(search_dir);
	builder
		.hidden(false) // Don't automatically skip hidden files (we'll filter manually)
		.git_ignore(true) // Respect .gitignore files
		.git_global(true) // Respect global git ignore
		.git_exclude(true) // Respect .git/info/exclude
		.require_git(false) // Don't require git repository
		.follow_links(false) // Don't follow symlinks
		.max_depth(None); // No depth limit

	// Collect all files first, then apply glob filtering
	let walker = builder.build();
	let mut all_files = Vec::new();

	for result in walker {
		match result {
			Ok(entry) => {
				let path = entry.path();

				// Skip directories
				if !path.is_file() {
					continue;
				}

				// Skip dotfiles and files in dot directories
				let path_str = path.to_string_lossy();
				if is_dotfile_or_in_dot_directory(&path_str) {
					continue;
				}

				all_files.push(path_str.to_string());
			}
			Err(err) => {
				crate::log_debug!("Walker error: {}", err);
				// Continue walking even if some paths fail
			}
		}
	}

	crate::log_debug!(
		"Found {} files after gitignore and dotfile filtering",
		all_files.len()
	);

	// Now apply glob pattern matching
	for pattern in patterns {
		let mut pattern_matches = 0;

		// Check if this looks like a glob pattern
		if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
			// Compile glob pattern
			let glob_pattern = match glob::Pattern::new(pattern) {
				Ok(p) => p,
				Err(e) => return Err(anyhow!("Invalid glob pattern '{}': {}", pattern, e)),
			};

			// Apply pattern to all files
			for file_path in &all_files {
				if glob_pattern.matches(file_path) {
					expanded_paths.push(file_path.clone());
					pattern_matches += 1;
				}
			}
		} else {
			// Not a glob pattern, add as-is if it exists and passes filters
			let path_obj = Path::new(pattern);
			if path_obj.exists() && path_obj.is_file() {
				let path_str = pattern;
				if !is_dotfile_or_in_dot_directory(path_str) {
					expanded_paths.push(pattern.clone());
					pattern_matches += 1;
				}
			}
		}

		crate::log_debug!(
			"Glob pattern '{}' matched {} files",
			pattern,
			pattern_matches
		);
	}

	// Deduplicate files in case multiple patterns match the same file
	expanded_paths.sort();
	expanded_paths.dedup();

	crate::log_debug!(
		"Total expanded files after deduplication: {}",
		expanded_paths.len()
	);

	// Check if we have too many files
	if expanded_paths.len() > MAX_EXPANDED_FILES {
		return Err(anyhow!(
            "Too many files expanded from glob patterns: {} files (max allowed: {}). Consider using more specific patterns to reduce the file count.",
            expanded_paths.len(),
            MAX_EXPANDED_FILES
        ));
	}

	Ok(expanded_paths)
}

/// Check if a file path is a dotfile or is inside a dot directory
///
/// This function identifies files that should be excluded:
/// - Files starting with '.' (e.g., .env, .gitignore)
/// - Files inside directories starting with '.' (e.g., .git/config, .vscode/settings.json)
///
/// # Arguments
/// * `path` - File path to check
///
/// # Returns
/// * `true` if the file should be excluded, `false` otherwise
fn is_dotfile_or_in_dot_directory(path: &str) -> bool {
	// Split path into components and check each one
	for component in Path::new(path).components() {
		if let Some(name) = component.as_os_str().to_str() {
			if name.starts_with('.') && name != "." && name != ".." {
				return true;
			}
		}
	}
	false
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_is_dotfile_or_in_dot_directory() {
		// Regular files should not be filtered
		assert!(!is_dotfile_or_in_dot_directory("src/main.rs"));
		assert!(!is_dotfile_or_in_dot_directory(
			"ui/components/Button.svelte"
		));
		assert!(!is_dotfile_or_in_dot_directory("README.md"));

		// Dotfiles should be filtered
		assert!(is_dotfile_or_in_dot_directory(".env"));
		assert!(is_dotfile_or_in_dot_directory(".gitignore"));
		assert!(is_dotfile_or_in_dot_directory(".eslintrc.json"));

		// Files in dot directories should be filtered
		assert!(is_dotfile_or_in_dot_directory(".git/config"));
		assert!(is_dotfile_or_in_dot_directory(".vscode/settings.json"));
		assert!(is_dotfile_or_in_dot_directory("src/.hidden/file.rs"));
		assert!(is_dotfile_or_in_dot_directory(".github/workflows/ci.yml"));

		// Current and parent directory references should not be filtered
		assert!(!is_dotfile_or_in_dot_directory("."));
		assert!(!is_dotfile_or_in_dot_directory(".."));
		assert!(!is_dotfile_or_in_dot_directory("./src/main.rs"));
		assert!(!is_dotfile_or_in_dot_directory("../other/file.rs"));
	}
}
