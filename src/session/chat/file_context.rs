// Copyright 2026 Muvon Un Limited
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

// File context parsing and generation for compression summaries

use crate::utils::file_parser::parse_file_references;
use crate::utils::file_renderer::render_files_as_xml;

/// Parse file context requirements from AI summary response
/// Expected format: filename:startline:endline (in code blocks or specific sections)
pub fn parse_file_contexts(summary_content: &str) -> Vec<(String, usize, usize)> {
	let file_refs = parse_file_references(summary_content);

	// Convert HashMap to Vec for existing API
	let mut contexts = Vec::new();
	for (filepath, ranges) in file_refs {
		for range in ranges {
			contexts.push((filepath.clone(), range.start, range.end));
		}
	}

	contexts
}

/// Generate file context content from parsed file requirements in XML format
pub fn generate_file_context_content(file_contexts: &[(String, usize, usize)]) -> String {
	if file_contexts.is_empty() {
		return "No specific file context requested.".to_string();
	}

	// Convert to new format and use XML renderer
	use crate::utils::file_parser::{read_file_lines, LineRange};
	use std::collections::HashMap;

	let mut file_contents = HashMap::new();

	for (filepath, start_line, end_line) in file_contexts {
		if let Some(range) = LineRange::new(*start_line, *end_line) {
			let content = read_file_lines(filepath, &range);
			file_contents
				.entry(filepath.clone())
				.or_insert_with(Vec::new)
				.push(content);
		}
	}

	render_files_as_xml(&file_contents)
}

/// Extract file references from tool call arguments
/// Returns raw refs (path or path:start:end) that need merging
pub fn extract_file_refs_from_args(
	tool_name: &str,
	args: &serde_json::Value,
	refs: &mut Vec<String>,
) {
	let file_read_tools = ["view", "text_editor", "batch_edit", "extract_lines"];

	if !file_read_tools.contains(&tool_name) {
		return;
	}

	let args_obj = match args.as_object() {
		Some(obj) => obj,
		None => return,
	};

	if let Some(path) = args_obj.get("path").and_then(|p| p.as_str()) {
		let lines = args_obj.get("lines");
		if let Some(lines) = lines {
			if let Some(arr) = lines.as_array() {
				if arr.len() >= 2 {
					if let (Some(start), Some(end)) = (arr[0].as_u64(), arr[1].as_u64()) {
						refs.push(format!("{}:{}:{}", path, start, end));
						return;
					}
				}
			}
		}
		refs.push(path.to_string());
	}

	if tool_name == "view" {
		if let Some(paths) = args_obj.get("paths").and_then(|p| p.as_array()) {
			for p in paths {
				if let Some(path) = p.as_str() {
					refs.push(path.to_string());
				}
			}
		}
	}

	if tool_name == "extract_lines" {
		if let Some(from_path) = args_obj.get("from_path").and_then(|p| p.as_str()) {
			let from_range = args_obj.get("from_range");
			if let Some(arr) = from_range.and_then(|r| r.as_array()) {
				if arr.len() >= 2 {
					if let (Some(start), Some(end)) = (arr[0].as_u64(), arr[1].as_u64()) {
						refs.push(format!("{}:{}:{}", from_path, start, end));
						return;
					}
				}
			}
			refs.push(from_path.to_string());
		}
	}
}

/// Merge overlapping file ranges to produce compact references
/// Input: ["src/main.rs:10:50", "src/main.rs:30:100", "src/main.rs:200:250"]
/// Output: ["src/main.rs:10:100", "src/main.rs:200:250"]
pub fn merge_file_refs(refs: &[String]) -> Vec<String> {
	use std::collections::{BTreeMap, BTreeSet};

	let mut by_file: BTreeMap<String, Vec<(u64, u64)>> = BTreeMap::new();
	let mut whole_files: BTreeSet<String> = BTreeSet::new();

	for ref_str in refs {
		let parts: Vec<&str> = ref_str.split(':').collect();
		match parts.len() {
			1 => {
				whole_files.insert(parts[0].to_string());
			}
			3 => {
				if let (Ok(start), Ok(end)) = (parts[1].parse::<u64>(), parts[2].parse::<u64>()) {
					by_file
						.entry(parts[0].to_string())
						.or_default()
						.push((start, end));
				}
			}
			_ => {}
		}
	}

	let mut merged: Vec<String> = Vec::new();

	// Add whole files first (they supersede any ranges)
	for path in whole_files {
		by_file.remove(&path);
		merged.push(path);
	}

	// Merge overlapping ranges per file
	for (path, mut ranges) in by_file {
		if ranges.is_empty() {
			continue;
		}

		// Sort by start
		ranges.sort_by_key(|r| r.0);

		// Merge overlapping/adjacent ranges
		let mut merged_ranges: Vec<(u64, u64)> = Vec::new();
		for (start, end) in ranges {
			if let Some(last) = merged_ranges.last_mut() {
				// Overlap or adjacent (within 10 lines - merge small gaps)
				if start <= last.1 + 10 {
					last.1 = last.1.max(end);
					continue;
				}
			}
			merged_ranges.push((start, end));
		}

		// Convert to strings
		for (start, end) in merged_ranges {
			merged.push(format!("{}:{}:{}", path, start, end));
		}
	}

	merged
}
#[cfg(test)]
mod tests {
	use super::parse_file_contexts;

	#[test]
	fn test_parse_file_contexts_code_block() {
		let summary = r#"
## REQUIRED FILE CONTEXTS
List ALL files needed as context to continue work. Use EXACT format:
```
src/main.rs:1:50
src/lib.rs:100:150
config/settings.toml:10:20
```
		"#;

		let contexts = parse_file_contexts(summary);
		assert_eq!(contexts.len(), 3);

		// Since HashMap iteration order is not guaranteed, check that all expected items are present
		assert!(contexts.contains(&("src/main.rs".to_string(), 1, 50)));
		assert!(contexts.contains(&("src/lib.rs".to_string(), 100, 150)));
		assert!(contexts.contains(&("config/settings.toml".to_string(), 10, 20)));
	}

	#[test]
	fn test_parse_file_contexts_section() {
		let summary = r#"
## REQUIRED FILE CONTEXTS
The following files need context:
- src/session/mod.rs:200:300
- tests/integration.rs:1:100

## NEXT STEPS
Continue with implementation...
		"#;

		let contexts = parse_file_contexts(summary);
		assert_eq!(contexts.len(), 2);
		assert!(contexts.contains(&("src/session/mod.rs".to_string(), 200, 300)));
		assert!(contexts.contains(&("tests/integration.rs".to_string(), 1, 100)));
	}

	#[test]
	fn test_parse_file_contexts_fallback() {
		let summary = r#"
We need to look at src/core.rs:50:100 and also check lib/utils.rs:1:25 for the implementation.
		"#;

		let contexts = parse_file_contexts(summary);
		assert_eq!(contexts.len(), 2);
		assert!(contexts.contains(&("src/core.rs".to_string(), 50, 100)));
		assert!(contexts.contains(&("lib/utils.rs".to_string(), 1, 25)));
	}

	#[test]
	fn test_parse_file_contexts_mandatory_format() {
		let summary = r#"
## REQUIRED FILE CONTEXTS
CRITICAL: List ALL files needed as context using EXACT format below.

**MANDATORY FORMAT - Use code block with exact pattern:**
```
src/session/chat/session_continuation.rs:100:200
src/config/mod.rs:50:100
tests/integration_test.rs:1:50
```

**PARSING REQUIREMENTS:**
- Each line must be exactly: filepath:number:number
		"#;

		let contexts = parse_file_contexts(summary);
		assert_eq!(contexts.len(), 3);
		assert!(contexts.contains(&(
			"src/session/chat/session_continuation.rs".to_string(),
			100,
			200
		)));
		assert!(contexts.contains(&("src/config/mod.rs".to_string(), 50, 100)));
		assert!(contexts.contains(&("tests/integration_test.rs".to_string(), 1, 50)));
	}

	#[test]
	fn test_parse_file_contexts_invalid_ranges() {
		let summary = r#"
```
src/main.rs:0:50
src/lib.rs:100:50
src/test.rs:1:20000
```
		"#;

		let contexts = parse_file_contexts(summary);
		// Should filter out invalid ranges (start=0, end<start, end>10000)
		assert_eq!(contexts.len(), 0);
	}

	#[test]
	fn test_parse_file_contexts_with_context_tags() {
		let summary = r#"
## REQUIRED FILE CONTEXTS
<context>
src/session/chat/continuation.rs:100:200
src/config/mod.rs:50:100
tests/integration_test.rs:1:50
</context>
		"#;

		let contexts = parse_file_contexts(summary);
		assert_eq!(contexts.len(), 3);
		assert!(contexts.contains(&("src/session/chat/continuation.rs".to_string(), 100, 200)));
		assert!(contexts.contains(&("src/config/mod.rs".to_string(), 50, 100)));
		assert!(contexts.contains(&("tests/integration_test.rs".to_string(), 1, 50)));
	}

	#[test]
	fn test_parse_file_contexts_context_tags_priority() {
		// Context tags should take priority over code blocks
		let summary = r#"
<context>
src/main.rs:1:10
</context>

```
src/lib.rs:20:30
```
		"#;

		let contexts = parse_file_contexts(summary);
		// Should only parse context tags, not code blocks
		assert_eq!(contexts.len(), 1);
		assert!(contexts.contains(&("src/main.rs".to_string(), 1, 10)));
	}
}
