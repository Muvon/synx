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

// Directory operations module - handling file listing with ripgrep

use super::super::{McpToolCall, McpToolResult};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::process::Command;

// Convert glob pattern to regex pattern for use with ripgrep
fn convert_glob_to_regex(glob_pattern: &str) -> String {
	// Handle multiple patterns separated by |
	let patterns: Vec<&str> = glob_pattern.split('|').collect();

	if patterns.len() > 1 {
		// Multiple patterns - convert each and join with |
		let regex_patterns: Vec<String> = patterns
			.iter()
			.map(|p| convert_single_glob_to_regex(p.trim()))
			.collect();
		format!("({})", regex_patterns.join("|"))
	} else {
		// Single pattern
		convert_single_glob_to_regex(glob_pattern)
	}
}

// Convert a single glob pattern to regex
fn convert_single_glob_to_regex(pattern: &str) -> String {
	let mut regex = String::new();
	let chars: Vec<char> = pattern.chars().collect();
	let mut i = 0;

	while i < chars.len() {
		match chars[i] {
			'*' => {
				// Convert * to .*? (non-greedy match any characters)
				regex.push_str(".*?");
			}
			'?' => {
				// Convert ? to . (match any single character)
				regex.push('.');
			}
			'.' => {
				// Escape dots for literal match
				regex.push_str("\\.");
			}
			'^' | '$' | '(' | ')' | '[' | ']' | '{' | '}' | '+' | '\\' => {
				// Escape regex special characters
				regex.push('\\');
				regex.push(chars[i]);
			}
			_ => {
				// Regular character
				regex.push(chars[i]);
			}
		}
		i += 1;
	}

	// Add end-of-line anchor to ensure complete filename match
	format!("{}$", regex)
}

// Execute list_files command
pub async fn execute_list_files(call: &McpToolCall) -> Result<McpToolResult> {
	// Extract directory parameter
	let directory = match call.parameters.get("directory") {
		Some(Value::String(dir)) => dir.clone(),
		_ => return Err(anyhow!("Missing or invalid 'directory' parameter")),
	};

	// Extract optional parameters
	let pattern = call
		.parameters
		.get("pattern")
		.and_then(|v| v.as_str())
		.map(|s| s.to_string());

	let content = call
		.parameters
		.get("content")
		.and_then(|v| v.as_str())
		.map(|s| s.to_string());

	let max_depth = call
		.parameters
		.get("max_depth")
		.and_then(|v| v.as_u64())
		.map(|n| n as usize);

	let include_hidden = call
		.parameters
		.get("include_hidden")
		.and_then(|v| v.as_bool())
		.unwrap_or(false);

	let max_lines = call
		.parameters
		.get("max_lines")
		.and_then(|v| v.as_i64())
		.unwrap_or(20) as usize;

	// Build the ripgrep command using proper argument passing
	let mut cmd = Command::new("rg");

	// Add depth limit if specified
	if let Some(depth) = max_depth {
		cmd.arg("--max-depth").arg(depth.to_string());
	}

	// Add hidden files flag if requested
	if include_hidden {
		cmd.arg("--hidden");
	}

	// Set current directory
	cmd.current_dir(&directory);

	// Configure the command based on the operation type
	let output_type = if let Some(ref content_pattern) = content {
		// Search for content in files
		cmd.arg(content_pattern);
		"content search"
	} else if let Some(ref _name_pattern) = pattern {
		// List files and filter by pattern
		cmd.arg("--files");
		// We'll filter the output by pattern later to avoid shell escaping issues
		"filename pattern"
	} else {
		// Default: list all files using ripgrep
		cmd.arg("--files");
		"file listing"
	};

	// Execute the command
	let output = tokio::task::spawn_blocking(move || {
		let output = cmd.output();

		match output {
			Ok(output) => {
				let stdout = String::from_utf8_lossy(&output.stdout).to_string();
				let stderr = String::from_utf8_lossy(&output.stderr).to_string();

				// Parse the output into a list of files
				let mut files: Vec<String> = stdout.lines().map(|s| s.to_string()).collect();

				// Filter by pattern if we're doing filename pattern matching
				if let Some(ref name_pattern) = pattern {
					let regex_pattern = convert_glob_to_regex(name_pattern);
					if let Ok(regex) = regex::Regex::new(&regex_pattern) {
						files.retain(|file| regex.is_match(file));
					}
				}

				// Apply truncation if max_lines is set (0 means unlimited)
				let (truncated_files, truncation_info) = if max_lines > 0 && files.len() > max_lines
				{
					let total_count = files.len();
					let half_limit = max_lines / 2;
					let remaining = max_lines - half_limit;

					let mut truncated = Vec::new();

					// Add first half
					truncated.extend(files.iter().take(half_limit).cloned());

					// Add truncation marker
					let truncated_count = total_count - max_lines;
					truncated.push(format!(
						"[{} lines truncated - use more specific patterns or increase max_lines]",
						truncated_count
					));

					// Add last portion
					truncated.extend(files.iter().skip(total_count - remaining).cloned());

					(
						truncated,
						Some(format!(
							"Output truncated: showing {} of {} total files",
							max_lines, total_count
						)),
					)
				} else {
					(files.clone(), None)
				};

				let output_str = if stdout.is_empty() && !stderr.is_empty() {
					stderr
				} else {
					truncated_files.join("\n")
				};

				let mut result = json!({
					"success": output.status.success(),
					"output": output_str,
					"files": truncated_files,
					"count": files.len(),
					"displayed_count": truncated_files.len(),
					"type": output_type,
					"parameters": {
						"directory": directory,
						"pattern": pattern,
						"content": content,
						"max_depth": max_depth,
						"include_hidden": include_hidden,
						"max_lines": max_lines
					}
				});

				// Add truncation info if present
				if let Some(info) = truncation_info {
					result["truncation_info"] = json!(info);
				}

				result
			}
			Err(e) => json!({
				"success": false,
				"output": format!("Failed to list files: {}", e),
				"files": [],
				"count": 0,
				"displayed_count": 0,
				"parameters": {
					"directory": directory,
					"pattern": pattern,
					"content": content,
					"max_depth": max_depth,
					"include_hidden": include_hidden,
					"max_lines": max_lines
				}
			}),
		}
	})
	.await?;

	Ok(McpToolResult {
		tool_name: "list_files".to_string(),
		tool_id: call.tool_id.clone(),
		result: output,
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_glob_to_regex_conversion() {
		// Test single pattern
		assert_eq!(convert_glob_to_regex("*.rs"), ".*?\\.rs$");
		assert_eq!(convert_glob_to_regex("*.py"), ".*?\\.py$");

		// Test multiple patterns (the problematic case)
		assert_eq!(
			convert_glob_to_regex("*.rs|*.py|*.js|*.ts"),
			"(.*?\\.rs$|.*?\\.py$|.*?\\.js$|.*?\\.ts$)"
		);

		// Test pattern with directory
		assert_eq!(convert_glob_to_regex("src/*.rs"), "src/.*?\\.rs$");

		// Test pattern with question mark
		assert_eq!(convert_glob_to_regex("test?.py"), "test.\\.py$");
	}

	#[test]
	fn test_single_glob_to_regex() {
		assert_eq!(convert_single_glob_to_regex("*.rs"), ".*?\\.rs$");
		assert_eq!(convert_single_glob_to_regex("test?.py"), "test.\\.py$");
		assert_eq!(convert_single_glob_to_regex("file.txt"), "file\\.txt$");
		assert_eq!(convert_single_glob_to_regex("*.c"), ".*?\\.c$");
	}
}
