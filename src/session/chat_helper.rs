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

// Implementation of a command completer for rustyline
use colored::*;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::Validator;
use rustyline::Helper;
use std::borrow::Cow::{self, Borrowed, Owned};
use std::fs;
use std::path::{Path, PathBuf};

struct CommandCompleter<'a> {
	commands: Vec<String>,
	config: &'a crate::config::Config,
	role: &'a str,
}

impl<'a> CommandCompleter<'a> {
	fn new(config: &'a crate::config::Config, role: &'a str) -> Self {
		let commands = crate::session::chat::COMMANDS
			.iter()
			.map(|&s| s.to_string())
			.collect();
		Self {
			commands,
			config,
			role,
		}
	}

	/// Check if the given file extension is a supported image format
	fn is_image_file(path: &str) -> bool {
		let supported_extensions = [
			".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp", ".tiff", ".tif", ".ico", ".svg",
			".avif", ".heic", ".heif",
		];

		let path_lower = path.to_lowercase();
		supported_extensions
			.iter()
			.any(|ext| path_lower.ends_with(ext))
	}

	/// Expand tilde (~) to home directory
	fn expand_tilde(path: &str) -> PathBuf {
		if let Some(stripped) = path.strip_prefix("~/") {
			if let Some(home) = dirs::home_dir() {
				home.join(stripped)
			} else {
				PathBuf::from(path)
			}
		} else if path == "~" {
			dirs::home_dir().unwrap_or_else(|| PathBuf::from(path))
		} else {
			PathBuf::from(path)
		}
	}

	/// Custom file completion that handles absolute paths and tilde expansion
	fn complete_file_path(file_part: &str) -> Vec<Pair> {
		if file_part.is_empty() {
			// Show current directory contents
			return Self::list_directory_contents(".");
		}

		// Expand tilde if present
		let expanded_path = Self::expand_tilde(file_part);
		let expanded_str = expanded_path.to_string_lossy();

		// If the path ends with a separator, list contents of that directory
		if file_part.ends_with('/') || file_part.ends_with('\\') {
			return Self::list_directory_contents(&expanded_str);
		}

		// Determine the parent directory and filename part
		let (parent_dir, filename_part) = if let Some(parent) = expanded_path.parent() {
			let parent_str = parent.to_str().unwrap_or(".");
			// If parent is empty string (for relative paths without ./), use current directory
			let actual_parent = if parent_str.is_empty() {
				"."
			} else {
				parent_str
			};

			let filename_part = expanded_path
				.file_name()
				.and_then(|n| n.to_str())
				.unwrap_or("");

			(actual_parent, filename_part)
		} else {
			// No parent found, treat as current directory search
			(".", file_part)
		};

		let mut candidates = Self::list_directory_contents(parent_dir);

		// Filter candidates that start with the filename part
		if !filename_part.is_empty() {
			let filename_lower = filename_part.to_lowercase();
			candidates.retain(|candidate| {
				let name = Path::new(&candidate.replacement)
					.file_name()
					.and_then(|n| n.to_str())
					.unwrap_or("")
					.to_lowercase();
				name.starts_with(&filename_lower)
			});
		}

		// Adjust the replacement paths to be relative to the original input
		for candidate in &mut candidates {
			if file_part.starts_with("~/") {
				// Convert back to tilde notation
				if let Some(home) = dirs::home_dir() {
					if let Ok(relative) = PathBuf::from(&candidate.replacement).strip_prefix(&home)
					{
						candidate.replacement = format!("~/{}", relative.to_string_lossy());
					}
				}
			} else if file_part.starts_with('/') {
				// Keep absolute path
				// candidate.replacement is already correct
			} else {
				// For relative paths, make sure we maintain the relative nature
				if let Ok(relative) = PathBuf::from(&candidate.replacement)
					.strip_prefix(std::env::current_dir().unwrap_or_default())
				{
					candidate.replacement = relative.to_string_lossy().to_string();
				}
			}
		}

		candidates
	}

	/// List contents of a directory, returning both directories and image files
	fn list_directory_contents(dir_path: &str) -> Vec<Pair> {
		let mut candidates = Vec::new();

		if let Ok(entries) = fs::read_dir(dir_path) {
			for entry in entries.flatten() {
				let path = entry.path();
				let path_str = path.to_string_lossy().to_string();

				if path.is_dir() {
					// Add directory with trailing slash
					let display = format!(
						"{}/",
						path.file_name().and_then(|n| n.to_str()).unwrap_or("")
					);
					candidates.push(Pair {
						display: display.clone(),
						replacement: format!("{}/", path_str),
					});
				} else if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
					if Self::is_image_file(filename) {
						candidates.push(Pair {
							display: filename.to_string(),
							replacement: path_str,
						});
					}
				}
			}
		}

		// Sort: directories first, then files
		candidates.sort_by(|a, b| {
			let a_is_dir = a.replacement.ends_with('/');
			let b_is_dir = b.replacement.ends_with('/');

			match (a_is_dir, b_is_dir) {
				(true, false) => std::cmp::Ordering::Less,
				(false, true) => std::cmp::Ordering::Greater,
				_ => a.replacement.cmp(&b.replacement),
			}
		});

		candidates
	}

	/// Filter and prepare completion candidates for better UX with bash-like completion
	fn filter_and_limit_candidates(candidates: Vec<Pair>, file_part: &str) -> Vec<Pair> {
		// For bash-like completion, find common prefix for partial completion
		if candidates.is_empty() {
			return candidates;
		}

		// If there's a common prefix longer than current input, add it as first candidate
		let common_prefix = Self::find_common_prefix(&candidates);
		let mut result = candidates;

		// If common prefix is longer than current input, add it for partial completion
		if common_prefix.len() > file_part.len() {
			let partial_completion = Pair {
				display: format!("{} (partial)", common_prefix),
				replacement: common_prefix,
			};
			result.insert(0, partial_completion);
		}

		// Limit total candidates for better UX
		const MAX_TOTAL: usize = 10; // More options for list mode
		result.truncate(MAX_TOTAL);
		result
	}

	/// Find common prefix among all candidates for partial completion
	fn find_common_prefix(candidates: &[Pair]) -> String {
		if candidates.is_empty() {
			return String::new();
		}

		let first = &candidates[0].replacement;
		let mut common_len = first.len();

		for candidate in candidates.iter().skip(1) {
			let replacement = &candidate.replacement;
			let mut len = 0;
			for (a, b) in first.chars().zip(replacement.chars()) {
				if a == b {
					len += a.len_utf8();
				} else {
					break;
				}
			}
			common_len = common_len.min(len);
		}

		first[..common_len].to_string()
	}
}

impl<'a> Completer for CommandCompleter<'a> {
	type Candidate = Pair;

	fn complete(
		&self,
		line: &str,
		pos: usize,
		_ctx: &rustyline::Context<'_>,
	) -> Result<(usize, Vec<Self::Candidate>), ReadlineError> {
		// Handle /image command with file completion
		if line.starts_with("/image ") {
			let image_prefix = "/image ";
			let prefix_len = image_prefix.len();

			// Extract the file path part up to cursor position
			let file_part = if pos > prefix_len {
				&line[prefix_len..pos]
			} else {
				""
			};

			// Use our custom file completion that handles absolute paths and tilde expansion
			let candidates = Self::complete_file_path(file_part);
			let filtered_candidates = Self::filter_and_limit_candidates(candidates, file_part);

			// For file completion, we want to replace from the start of the file part
			Ok((prefix_len, filtered_candidates))
		} else if line.starts_with("/prompt ") {
			// Handle /prompt command with template name completion
			let prompt_prefix = "/prompt ";
			let prefix_len = prompt_prefix.len();

			// Extract the template name part up to cursor position
			let template_part = if pos > prefix_len {
				&line[prefix_len..pos]
			} else {
				""
			};

			// Get available prompt templates from config
			let candidates: Vec<Pair> = self
				.config
				.prompts
				.iter()
				.filter(|prompt| prompt.name.starts_with(template_part))
				.map(|prompt| Pair {
					display: if let Some(ref description) = prompt.description {
						format!("{} - {}", prompt.name, description)
					} else {
						prompt.name.clone()
					},
					replacement: prompt.name.clone(),
				})
				.collect();

			Ok((prefix_len, candidates))
		} else if line.starts_with("/run ") {
			// Handle /run command with command name completion
			let run_prefix = "/run ";
			let prefix_len = run_prefix.len();

			// Extract the command name part up to cursor position
			let command_part = if pos > prefix_len {
				&line[prefix_len..pos]
			} else {
				""
			};

			// Get available commands for this role
			let available_commands =
				crate::session::chat::list_available_commands(self.config, self.role);
			let candidates: Vec<Pair> = available_commands
				.iter()
				.filter(|cmd| cmd.starts_with(command_part))
				.map(|cmd| Pair {
					display: cmd.clone(),
					replacement: cmd.clone(),
				})
				.collect();

			Ok((prefix_len, candidates))
		} else if !line.starts_with('/') {
			// No completion for non-commands
			Ok((0, vec![]))
		} else {
			// Handle regular command completion with cursor position awareness
			let command_part = &line[..pos.min(line.len())];
			let candidates: Vec<Pair> = self
				.commands
				.iter()
				.filter(|cmd| cmd.starts_with(command_part))
				.map(|cmd| Pair {
					display: cmd.clone(),
					replacement: cmd.clone(),
				})
				.collect();

			// Find common prefix for partial completion
			let common_prefix = Self::find_common_prefix(&candidates);
			let mut result = candidates;

			// If there's a longer common prefix, add it as first option
			if common_prefix.len() > command_part.len() {
				let partial = Pair {
					display: format!("{} (partial)", common_prefix),
					replacement: common_prefix,
				};
				result.insert(0, partial);
			}

			Ok((0, result))
		}
	}
}

// We need to implement these traits to make CommandHelper work with rustyline
impl<'a> Hinter for CommandCompleter<'a> {
	type Hint = String;

	fn hint(&self, line: &str, _pos: usize, _ctx: &rustyline::Context<'_>) -> Option<Self::Hint> {
		if line.is_empty() || !line.starts_with('/') {
			return None;
		}

		// Special hint for /image command
		if line == "/image" {
			return Some(" <path_to_image>".to_string());
		}

		// Special hint for /prompt command
		if line == "/prompt" {
			return Some(" <template_name>".to_string());
		}

		// Special hint for /run command
		if line == "/run" {
			return Some(" <command_name>".to_string());
		}

		if line.starts_with("/image ") && line.len() > 7 {
			let file_part = &line[7..]; // "/image ".len() = 7
			if file_part.is_empty() {
				return Some("Start typing image file path...".to_string());
			}
			return None; // Let filename completer handle this
		}

		if line.starts_with("/prompt ") && line.len() > 8 {
			let template_part = &line[8..]; // "/prompt ".len() = 8
			if template_part.is_empty() {
				return Some("Start typing prompt template name...".to_string());
			}
			return None; // Let template completer handle this
		}

		if line.starts_with("/run ") && line.len() > 5 {
			let command_part = &line[5..]; // "/run ".len() = 5
			if command_part.is_empty() {
				return Some("Start typing command name...".to_string());
			}
			return None; // Let command completer handle this
		}

		// Look for a command that starts with the current input
		self.commands
			.iter()
			.find(|cmd| cmd.starts_with(line))
			.map(|cmd| cmd[line.len()..].to_string())
	}
}

impl<'a> Highlighter for CommandCompleter<'a> {
	fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
		// Only apply highlighting to commands (lines starting with '/')
		if line.starts_with('/') {
			// Special handling for /image command with file path
			if line.starts_with("/image ") && line.len() > 7 {
				let image_cmd = "/image";
				let file_part = &line[7..]; // "/image ".len() = 7

				// Check if the file path points to a valid image
				if !file_part.is_empty()
					&& Path::new(file_part).exists()
					&& Self::is_image_file(file_part)
				{
					// Highlight valid image path in bright green
					return Owned(format!(
						"{} {}",
						image_cmd.green(),
						file_part.bright_green()
					));
				} else if !file_part.is_empty() {
					// Highlight invalid/non-existent path in yellow
					return Owned(format!("{} {}", image_cmd.green(), file_part.yellow()));
				} else {
					// Just the command part is green
					return Owned(format!("{} ", image_cmd.green()));
				}
			}

			// Special handling for /prompt command with template name
			if line.starts_with("/prompt ") && line.len() > 8 {
				let prompt_cmd = "/prompt";
				let template_part = &line[8..]; // "/prompt ".len() = 8

				if !template_part.is_empty() {
					// Check if template exists
					let template_exists = self
						.config
						.prompts
						.iter()
						.any(|prompt| prompt.name == template_part);

					if template_exists {
						// Highlight valid template name in bright green
						return Owned(format!(
							"{} {}",
							prompt_cmd.green(),
							template_part.bright_green()
						));
					} else {
						// Highlight invalid template name in yellow
						return Owned(format!("{} {}", prompt_cmd.green(), template_part.yellow()));
					}
				} else {
					// Just the command part is green
					return Owned(format!("{} ", prompt_cmd.green()));
				}
			}

			// Special handling for /run command with command name
			if line.starts_with("/run ") && line.len() > 5 {
				let run_cmd = "/run";
				let command_part = &line[5..]; // "/run ".len() = 5

				if !command_part.is_empty() {
					// Check if command exists for this role
					let command_exists =
						crate::session::chat::command_exists(self.config, self.role, command_part);

					if command_exists {
						// Highlight valid command name in bright green
						return Owned(format!(
							"{} {}",
							run_cmd.green(),
							command_part.bright_green()
						));
					} else {
						// Highlight invalid command name in yellow
						return Owned(format!("{} {}", run_cmd.green(), command_part.yellow()));
					}
				} else {
					// Just the command part is green
					return Owned(format!("{} ", run_cmd.green()));
				}
			}

			// Check if this is a valid command
			let is_valid_command = self
				.commands
				.iter()
				.any(|cmd| line == cmd || cmd.starts_with(line));

			if is_valid_command {
				// Highlight valid commands in green
				Owned(line.green().to_string())
			} else {
				// Keep invalid commands normal colored
				Borrowed(line)
			}
		} else {
			Borrowed(line)
		}
	}

	fn highlight_char(&self, _line: &str, _pos: usize, _kind: CmdKind) -> bool {
		false
	}

	fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
		// Make hints appear in dim gray color - like bash autocomplete
		Owned(hint.bright_black().to_string())
	}
}

impl<'a> Validator for CommandCompleter<'a> {}

// Helper for rustyline
pub struct CommandHelper<'a> {
	completer: CommandCompleter<'a>,
	hinter: Option<HistoryHinter>,
}

impl<'a> CommandHelper<'a> {
	pub fn new(config: &'a crate::config::Config, role: &'a str) -> Self {
		Self {
			completer: CommandCompleter::new(config, role),
			hinter: Some(HistoryHinter {}),
		}
	}
}

// Implement Helper trait
impl<'a> Helper for CommandHelper<'a> {}

// Implement the required traits for rustyline helper
impl<'a> Completer for CommandHelper<'a> {
	type Candidate = Pair;

	fn complete(
		&self,
		line: &str,
		pos: usize,
		ctx: &rustyline::Context<'_>,
	) -> Result<(usize, Vec<Self::Candidate>), ReadlineError> {
		self.completer.complete(line, pos, ctx)
	}
}

impl<'a> Hinter for CommandHelper<'a> {
	type Hint = String;

	fn hint(&self, line: &str, pos: usize, ctx: &rustyline::Context<'_>) -> Option<Self::Hint> {
		if line.starts_with('/') {
			self.completer.hint(line, pos, ctx)
		} else if let Some(hinter) = &self.hinter {
			hinter.hint(line, pos, ctx)
		} else {
			None
		}
	}
}

impl<'a> Highlighter for CommandHelper<'a> {
	fn highlight<'l>(&self, line: &'l str, pos: usize) -> Cow<'l, str> {
		self.completer.highlight(line, pos)
	}

	fn highlight_char(&self, line: &str, pos: usize, kind: CmdKind) -> bool {
		self.completer.highlight_char(line, pos, kind)
	}

	fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
		self.completer.highlight_hint(hint)
	}
}

impl<'a> Validator for CommandHelper<'a> {}
