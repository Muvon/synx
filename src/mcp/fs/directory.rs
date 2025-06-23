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

// Directory operations module - handling file listing with ripgrep (FIXED VERSION)

use super::super::{McpToolCall, McpToolResult};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

// Parse a ripgrep output line to extract filename and rest, handling Windows paths correctly
fn parse_ripgrep_line(line: &str) -> Option<(&str, &str)> {
	// Find all colon positions
	let colon_positions: Vec<usize> = line.match_indices(':').map(|(i, _)| i).collect();

	// We need at least 2 colons for filename:line_number:content format
	if colon_positions.len() < 2 {
		return None;
	}

	// On Windows, the first colon might be after drive letter (C:)
	// Look for the colon that's followed by digits (line number)
	for i in 0..colon_positions.len() - 1 {
		let colon_pos = colon_positions[i];
		let next_colon_pos = colon_positions[i + 1];

		// Check if the part between these colons is a line number (digits)
		let potential_line_num = &line[colon_pos + 1..next_colon_pos];
		if potential_line_num.chars().all(|c| c.is_ascii_digit()) && !potential_line_num.is_empty()
		{
			// Found the filename:line_number:content pattern
			let filename = &line[..colon_pos];
			let rest = &line[colon_pos + 1..];
			return Some((filename, rest));
		}
	}

	// Fallback: if no digit pattern found, use the last colon before content
	// This handles edge cases where line numbers might have non-digit characters
	if colon_positions.len() >= 2 {
		let colon_pos = colon_positions[colon_positions.len() - 2];
		let filename = &line[..colon_pos];
		let rest = &line[colon_pos + 1..];
		return Some((filename, rest));
	}

	None
}

// Parse a ripgrep context line (with dashes) to extract filename and rest, handling Windows paths
fn parse_ripgrep_dash_line(line: &str) -> Option<(&str, &str)> {
	// Find all dash positions
	let dash_positions: Vec<usize> = line.match_indices('-').map(|(i, _)| i).collect();

	// We need at least 2 dashes for filename-line_number-content format
	if dash_positions.len() < 2 {
		return None;
	}

	// On Windows, look for the dash that's followed by digits (line number)
	for i in 0..dash_positions.len() - 1 {
		let dash_pos = dash_positions[i];
		let next_dash_pos = dash_positions[i + 1];

		// Check if the part between these dashes is a line number (digits)
		let potential_line_num = &line[dash_pos + 1..next_dash_pos];
		if potential_line_num.chars().all(|c| c.is_ascii_digit()) && !potential_line_num.is_empty()
		{
			// Found the filename-line_number-content pattern
			let filename = &line[..dash_pos];
			let rest = &line[dash_pos + 1..];
			return Some((filename, rest));
		}
	}

	// Fallback: use the last dash before content
	if dash_positions.len() >= 2 {
		let dash_pos = dash_positions[dash_positions.len() - 2];
		let filename = &line[..dash_pos];
		let rest = &line[dash_pos + 1..];
		return Some((filename, rest));
	}

	None
}

// Group ripgrep output by file for token efficiency while preserving line numbers
fn group_ripgrep_output(lines: &[String]) -> String {
	let mut result = Vec::new();
	let mut current_file = String::new();
	let mut file_lines = Vec::new();

	for line in lines {
		if line.contains("[") && line.contains("truncated") {
			// Handle truncation markers
			if !file_lines.is_empty() {
				result.push(format!("{}:\n{}", current_file, file_lines.join("\n")));
				file_lines.clear();
			}
			result.push(line.clone());
			continue;
		}

		// Parse ripgrep output: filename:line_number:content or filename-line_number-content (context)
		// Need to handle Windows paths (C:\path\file.rs:123:content) by finding the colon before line number
		if let Some((filename, rest)) = parse_ripgrep_line(line) {
			if filename != current_file {
				// New file - output previous file's lines
				if !file_lines.is_empty() {
					result.push(format!("{}:\n{}", current_file, file_lines.join("\n")));
					file_lines.clear();
				}
				current_file = filename.to_string();
			}

			// Add the line content (without filename)
			file_lines.push(rest.to_string());
		} else if let Some((filename, rest)) = parse_ripgrep_dash_line(line) {
			// Context line (filename-line_number-content)

			if filename != current_file {
				// New file - output previous file's lines
				if !file_lines.is_empty() {
					result.push(format!("{}:\n{}", current_file, file_lines.join("\n")));
					file_lines.clear();
				}
				current_file = filename.to_string();
			}

			// Add the context line (with dash to indicate context)
			file_lines.push(format!("-{}", rest));
		} else if line == "--" {
			// Separator between match groups - preserve it
			file_lines.push("--".to_string());
		} else {
			// Other lines (shouldn't happen in normal ripgrep output, but handle gracefully)
			file_lines.push(line.clone());
		}
	}

	// Output the last file's lines
	if !file_lines.is_empty() {
		result.push(format!("{}:\n{}", current_file, file_lines.join("\n")));
	}

	result.join("\n\n")
}

// Analyze search results to extract file types and directories
fn analyze_search_results(
	lines: &[String],
	is_content_search: bool,
) -> (HashMap<String, usize>, HashMap<String, usize>) {
	let mut file_extensions = HashMap::new();
	let mut directories = HashMap::new();

	for line in lines {
		let filepath = if is_content_search {
			// For content search: extract filename from "filename:line:content" format
			if let Some((filename, _)) = parse_ripgrep_line(line) {
				filename
			} else {
				continue;
			}
		} else {
			// For file listing: line is the filepath
			line.as_str()
		};

		// Extract file extension
		if let Some(ext) = Path::new(filepath).extension() {
			let ext_str = format!(".{}", ext.to_string_lossy());
			*file_extensions.entry(ext_str).or_insert(0) += 1;
		}

		// Extract directory
		if let Some(parent) = Path::new(filepath).parent() {
			let dir_str = parent.to_string_lossy().to_string();
			if !dir_str.is_empty() {
				*directories.entry(dir_str).or_insert(0) += 1;
			}
		}
	}

	(file_extensions, directories)
}

// Format statistics for file types and directories
fn format_statistics(
	file_extensions: &HashMap<String, usize>,
	directories: &HashMap<String, usize>,
) -> String {
	let mut result = String::new();

	if !file_extensions.is_empty() {
		let mut sorted_ext: Vec<_> = file_extensions.iter().collect();
		sorted_ext.sort_by(|a, b| b.1.cmp(a.1));
		let ext_str = sorted_ext
			.iter()
			.take(5)
			.map(|(ext, count)| format!("{}({})", ext, count))
			.collect::<Vec<_>>()
			.join(", ");
		result.push_str(&format!("Found in file types: {}", ext_str));
	}

	if !directories.is_empty() {
		let mut sorted_dirs: Vec<_> = directories.iter().collect();
		sorted_dirs.sort_by(|a, b| b.1.cmp(a.1));
		let dir_str = sorted_dirs
			.iter()
			.take(3)
			.map(|(dir, count)| format!("{}({})", dir, count))
			.collect::<Vec<_>>()
			.join(", ");
		if !result.is_empty() {
			result.push('\n');
		}
		result.push_str(&format!("Top directories: {}", dir_str));
	}

	result
}

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
			'[' => {
				// Character class - pass through as-is
				regex.push('[');
				i += 1;
				while i < chars.len() && chars[i] != ']' {
					regex.push(chars[i]);
					i += 1;
				}
				if i < chars.len() {
					regex.push(']');
				}
			}
			c if "(){}^$+|\\".contains(c) => {
				// Escape regex special characters
				regex.push('\\');
				regex.push(c);
			}
			c => {
				// Regular character
				regex.push(c);
			}
		}
		i += 1;
	}

	regex
}

// Execute list_files command with PROPER content search vs file listing handling
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

	let line_numbers = call
		.parameters
		.get("line_numbers")
		.and_then(|v| v.as_bool())
		.unwrap_or(true);

	let context_lines = call
		.parameters
		.get("context")
		.and_then(|v| v.as_i64())
		.unwrap_or(0) as usize;

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

	// Configure the command based on the operation type
	let (output_type, is_content_search) = if let Some(ref content_pattern) = content {
		// Content search: search for content within files
		if line_numbers {
			cmd.arg("--line-number");
		}

		// Add context if specified
		if context_lines > 0 {
			cmd.arg("--context").arg(context_lines.to_string());
		}

		// Add the search pattern
		cmd.arg(content_pattern);

		// Add the directory as the search path
		cmd.arg(&directory);

		("content search", true)
	} else {
		// File listing: list files (optionally filtered by pattern)
		cmd.arg("--files");

		// Add the directory as the search path
		cmd.arg(&directory);

		("file listing", false)
	};

	// Execute the command
	let output = tokio::task::spawn_blocking(move || {
        let output = cmd.output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if is_content_search {
                    // For content search, preserve the original ripgrep output format
                    // which includes filenames, line numbers, and matched content
                    let lines: Vec<String> = stdout.lines().map(|s| s.to_string()).collect();

                    // Apply truncation if max_lines is set (0 means unlimited)
                    let (truncated_lines, truncation_info) = if max_lines > 0 && lines.len() > max_lines {
                        let total_count = lines.len();
                        let half_limit = max_lines / 2;
                        let remaining = max_lines - half_limit;

                        let mut truncated = Vec::new();

                        // Add first half
                        truncated.extend(lines.iter().take(half_limit).cloned());

                        // Add truncation marker with statistics
                        let truncated_count = total_count - max_lines;
                        let (file_extensions, directories) = analyze_search_results(&lines, true);
                        let statistics = format_statistics(&file_extensions, &directories);
                        let truncation_msg = if statistics.is_empty() {
                            format!("[{} lines truncated - use more specific patterns or increase max_lines]", truncated_count)
                        } else {
                            format!("[{} lines truncated - use more specific patterns or increase max_lines]\n\n{}", truncated_count, statistics)
                        };
                        truncated.push(truncation_msg);

                        // Add last portion
                        truncated.extend(lines.iter().skip(total_count - remaining).cloned());

                        (truncated, Some(format!("Output truncated: showing {} of {} total lines", max_lines, total_count)))
                    } else {
                        (lines.clone(), None)
                    };

                    let output_str = if stdout.is_empty() && !stderr.is_empty() {
                        stderr
                    } else {
                        // Use grouped output for token efficiency
                        group_ripgrep_output(&truncated_lines)
                    };

                    // For content search, we return the formatted output with matches
                    let mut result = json!({
                        "success": output.status.success(),
                        "output": output_str,
                        "lines": truncated_lines,
                        "total_lines": lines.len(),
                        "displayed_lines": truncated_lines.len(),
                        "type": output_type,
                        "parameters": {
                        "directory": directory,
                        "pattern": pattern,
                        "content": content,
                        "max_depth": max_depth,
                        "include_hidden": include_hidden,
                        "max_lines": max_lines,
                        "line_numbers": line_numbers,
                        "context": context_lines
                    }
                });

                    // Add truncation info if present
                    if let Some(info) = truncation_info {
                        result["truncation_info"] = json!(info);
                    }

                    result
                } else {
                    // For file listing, parse as files and apply pattern filtering
                    let mut files: Vec<String> = stdout.lines().map(|s| s.to_string()).collect();

                    // Filter by pattern if we're doing filename pattern matching
                    if let Some(ref name_pattern) = pattern {
                        let regex_pattern = convert_glob_to_regex(name_pattern);
                        if let Ok(regex) = regex::Regex::new(&regex_pattern) {
                            files.retain(|file| regex.is_match(file));
                        }
                    }

                    // Apply truncation if max_lines is set (0 means unlimited)
                    let (truncated_files, truncation_info) = if max_lines > 0 && files.len() > max_lines {
                        let total_count = files.len();
                        let half_limit = max_lines / 2;
                        let remaining = max_lines - half_limit;

                        let mut truncated = Vec::new();

                        // Add first half
                        truncated.extend(files.iter().take(half_limit).cloned());

                        // Add truncation marker with statistics
                        let truncated_count = total_count - max_lines;
                        let (file_extensions, directories) = analyze_search_results(&files, false);
                        let statistics = format_statistics(&file_extensions, &directories);
                        let truncation_msg = if statistics.is_empty() {
                            format!("[{} lines truncated - use more specific patterns or increase max_lines]", truncated_count)
                        } else {
                            format!("[{} lines truncated - use more specific patterns or increase max_lines]\n\n{}", truncated_count, statistics)
                        };
                        truncated.push(truncation_msg);

                        // Add last portion
                        truncated.extend(files.iter().skip(total_count - remaining).cloned());

                        (truncated, Some(format!("Output truncated: showing {} of {} total files", max_lines, total_count)))
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
                        "max_lines": max_lines,
                        "line_numbers": line_numbers,
                        "context": context_lines
                    }
                });

                    // Add truncation info if present
                    if let Some(info) = truncation_info {
                        result["truncation_info"] = json!(info);
                    }

                    result
                }
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
                "max_lines": max_lines,
                "line_numbers": line_numbers,
                "context": context_lines
            }
        }),
        }
    })
        .await
        .map_err(|e| anyhow!("Failed to execute file listing command: {}", e))?;

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
	fn test_parse_ripgrep_line_unix_path() {
		let line = "/home/user/file.rs:123:println!(\"test\");";
		let result = parse_ripgrep_line(line);
		assert_eq!(
			result,
			Some(("/home/user/file.rs", "123:println!(\"test\");"))
		);
	}

	#[test]
	fn test_parse_ripgrep_line_windows_path() {
		let line = "C:\\Users\\Test\\file.rs:123:println!(\"test\");";
		let result = parse_ripgrep_line(line);
		assert_eq!(
			result,
			Some(("C:\\Users\\Test\\file.rs", "123:println!(\"test\");"))
		);
	}

	#[test]
	fn test_parse_ripgrep_line_windows_path_with_spaces() {
		let line = "C:\\Users\\Test User\\My File.rs:456:let x = 42;";
		let result = parse_ripgrep_line(line);
		assert_eq!(
			result,
			Some(("C:\\Users\\Test User\\My File.rs", "456:let x = 42;"))
		);
	}

	#[test]
	fn test_parse_ripgrep_dash_line_unix_path() {
		let line = "/home/user/file.rs-123-some context line";
		let result = parse_ripgrep_dash_line(line);
		assert_eq!(
			result,
			Some(("/home/user/file.rs", "123-some context line"))
		);
	}

	#[test]
	fn test_parse_ripgrep_dash_line_windows_path() {
		let line = "C:\\Users\\Test\\file.rs-123-some context line";
		let result = parse_ripgrep_dash_line(line);
		assert_eq!(
			result,
			Some(("C:\\Users\\Test\\file.rs", "123-some context line"))
		);
	}

	#[test]
	fn test_parse_ripgrep_line_invalid_format() {
		let line = "just some text without proper format";
		let result = parse_ripgrep_line(line);
		assert_eq!(result, None);
	}

	#[test]
	fn test_parse_ripgrep_line_single_colon() {
		let line = "C:\\Users\\file.rs";
		let result = parse_ripgrep_line(line);
		assert_eq!(result, None);
	}
}
