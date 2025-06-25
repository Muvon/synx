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

// Session continuation module - handles automatic session reset when token limits are reached

use crate::session::chat::session::ChatSession;
use crate::{config::Config, log_info};
use anyhow::Result;
use regex::Regex;
use std::path::Path;

// All constants kept internal - no configuration needed
pub const SUMMARY_REQUEST_PROMPT: &str = r#"
CRITICAL: Session approaching token limits. Provide COMPREHENSIVE handoff summary to continue work seamlessly from scratch:

## MAIN OBJECTIVE & SCOPE
What we're building/fixing/implementing:
- Primary goal and why it matters
- Scope boundaries and what's included/excluded
- Success criteria and expected outcomes

## DETAILED PROGRESS ACCOMPLISHED
Complete breakdown of what's been done:
- Specific code changes made (functions, files, logic)
- Configuration changes and settings modified
- Tools executed and their results/outputs
- Problems identified and solutions implemented
- Key insights discovered during implementation
- Any debugging steps taken and findings

## CURRENT IMPLEMENTATION STATE
Exact technical situation right now:
- What's working vs what's broken/incomplete
- Active work in progress (half-finished implementations)
- Current file states and modifications pending
- Any compilation/runtime issues encountered
- Dependencies or prerequisites that are ready/missing

## REQUIRED FILE CONTEXTS
CRITICAL: List ALL files needed as context using EXACT format below. This will be automatically parsed to load file contents.

**MANDATORY FORMAT - Use code block with exact pattern:**
```
filename:startline:endline
filename:startline:endline
filename:startline:endline
```

**PARSING REQUIREMENTS:**
- Each line must be exactly: filepath:number:number
- No spaces around colons
- Use absolute paths from project root (src/main.rs not ./src/main.rs)
- Line numbers must be positive integers
- Start line must be ≤ end line
- End line must be ≤ 10000
- Maximum 10 file ranges total

**INCLUDE THESE FILES:**
- Core implementation files with key functions/classes
- Configuration files with relevant sections
- Test files if testing is involved
- Any modified or newly created files
- Files containing error patterns or debugging areas

**EXAMPLE CORRECT FORMAT:**
```
src/session/chat/session_continuation.rs:100:200
src/config/mod.rs:50:100
tests/integration_test.rs:1:50
```

**WRONG FORMATS (will not be parsed):**
- src/main.rs : 1 : 50 (spaces around colons)
- ./src/main.rs:1:50 (relative path with ./)
- src/main.rs lines 1-50 (text description)
- src/main.rs:1:50, src/lib.rs:1:100 (comma separated)

## IMMEDIATE NEXT STEPS
Specific actionable steps to continue (in order):
- Exact next implementation tasks
- Files to modify and what changes to make
- Commands to run or tools to execute
- Testing or verification steps needed
- Expected challenges and how to handle them

## CRITICAL TECHNICAL DETAILS
Essential information for seamless continuation:
- Important variable names, function signatures, or data structures
- Key algorithms or logic patterns being used
- Error handling approaches or edge cases discovered
- Performance considerations or constraints
- Integration points with existing systems
- Any architectural decisions made and why

## CONTEXT FOR UNDERSTANDING
Background information needed to work effectively:
- How this work fits into the larger system
- Related components or dependencies involved
- Previous attempts or approaches that didn't work
- Domain knowledge or business logic relevant
- Any user requirements or constraints to remember

PROVIDE COMPLETE DETAILS - imagine explaining to a new developer who needs to pick up exactly where you left off.
"#;

pub const CONTINUATION_USER_MESSAGE_TEMPLATE: &str = r#"Thank you for the summary.

Currently we are working on the following requests:
<tasks>
{}
</tasks>

Here's the required file context:
<files>
{}
</files>

Let's continue our work from where we left off.
Please proceed with the next steps as outlined in your summary.
CRITICAL: use tool calling in paralell when its possible to reach results faster and more efficiently."#;

/// Parameters for continuation processing
pub struct ContinuationParams<'a> {
	pub chat_session: &'a mut ChatSession,
	pub config: &'a Config,
	pub current_tokens: usize,
}

impl<'a> ContinuationParams<'a> {
	pub fn new(
		chat_session: &'a mut ChatSession,
		config: &'a Config,
		current_tokens: usize,
	) -> Self {
		Self {
			chat_session,
			config,
			current_tokens,
		}
	}
}

/// Check if we should trigger session continuation
pub fn should_trigger_continuation(params: &ContinuationParams) -> bool {
	params.config.max_session_tokens_threshold > 0
		&& params.current_tokens >= params.config.max_session_tokens_threshold
		&& !params.chat_session.continuation_pending
}

/// Check if we're currently in continuation process
pub fn is_continuation_in_progress(chat_session: &ChatSession) -> bool {
	chat_session.continuation_pending
}

/// Inject summary request into current session
/// This can happen during ANY processing step, not just user input
pub fn inject_summary_request(params: &mut ContinuationParams) -> Result<()> {
	use colored::*;

	println!(
		"\n{}",
		"🔄 Token limit reached during processing - requesting work summary...".bright_blue()
	);

	// Add summary request as user message
	let summary_message = crate::session::Message {
		role: "user".to_string(),
		content: SUMMARY_REQUEST_PROMPT.to_string(),
		timestamp: std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap_or_default()
			.as_secs(),
		cached: false,
		tool_calls: None,
		tool_call_id: None,
		name: None,
		images: None,
	};

	params.chat_session.session.messages.push(summary_message);
	params.chat_session.continuation_pending = true;

	log_info!("Summary request injected into conversation flow");

	Ok(())
}

/// Process continuation after AI responds with summary
/// Returns true if continuation was processed, false if this wasn't a continuation response
pub fn process_continuation_response(
	chat_session: &mut ChatSession,
	response_content: &str,
	has_tool_calls: bool,
) -> Result<bool> {
	// Only process if we're expecting a continuation response
	if !chat_session.continuation_pending {
		return Ok(false);
	}

	// If the response has tool calls, let them execute first
	if has_tool_calls {
		return Ok(false);
	}

	use colored::*;

	println!(
		"\n{}",
		"✅ Work summary received - resetting session for continuation...".bright_green()
	);

	// Find system message to preserve
	let system_message = chat_session
		.session
		.messages
		.iter()
		.find(|msg| msg.role == "system")
		.cloned();

	// Clear all messages
	chat_session.session.messages.clear();

	// Rebuild with minimal context
	if let Some(system_msg) = system_message {
		chat_session.session.messages.push(system_msg);
	}

	// Add the AI's summary as assistant message
	let summary_message = crate::session::Message {
		role: "assistant".to_string(),
		content: response_content.to_string(),
		timestamp: std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap_or_default()
			.as_secs(),
		cached: false,
		tool_calls: None,
		tool_call_id: None,
		name: None,
		images: None,
	};
	chat_session.session.messages.push(summary_message);

	// Collect user request history before clearing session
	let user_history = collect_user_request_history(&chat_session.session.messages);

	// Parse file context requirements from the AI's summary
	let file_contexts = parse_file_contexts(response_content);

	// Generate file context content
	let context_content = generate_file_context_content(&file_contexts);

	// Create continuation message with user history and file context
	let continuation_content = CONTINUATION_USER_MESSAGE_TEMPLATE
		.replace("{}", &user_history)
		.replace("{}", &context_content);

	let continue_message = crate::session::Message {
		role: "user".to_string(),
		content: continuation_content,
		timestamp: std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap_or_default()
			.as_secs(),
		cached: false,
		tool_calls: None,
		tool_call_id: None,
		name: None,
		images: None,
	};
	chat_session.session.messages.push(continue_message);

	// Reset continuation state
	chat_session.continuation_pending = false;

	// Log context information
	if !file_contexts.is_empty() {
		log_info!("Loaded context from {} file(s)", file_contexts.len());
		for (filepath, start, end) in &file_contexts {
			log_info!("   {} (lines {}-{})", filepath, start, end);
		}
	} else {
		log_info!("No file contexts found in AI summary - check format");
		crate::log_debug!("Summary content for context parsing: {}", response_content);
	}

	// Log user history preservation
	let history_lines = user_history.lines().count();
	if history_lines > 1 {
		log_info!(
			"Preserved {} user request(s) for continuation context",
			history_lines
		);
	}

	log_info!("Session context preserved - continuing automatically...");

	Ok(true)
}

/// Check and handle continuation at any point during processing
/// This is the main entry point that should be called during response processing
pub fn check_and_handle_continuation(
	chat_session: &mut ChatSession,
	config: &Config,
) -> Result<bool> {
	let current_tokens = crate::session::estimate_message_tokens(&chat_session.session.messages);

	let mut params = ContinuationParams::new(chat_session, config, current_tokens);

	if should_trigger_continuation(&params) {
		inject_summary_request(&mut params)?;
		return Ok(true); // Continuation triggered
	}

	Ok(false) // No continuation needed
}

/// Collect meaningful user requests from session history for continuation context
/// Filters out system-generated messages and keeps last 5-7 user requests
fn collect_user_request_history(messages: &[crate::session::Message]) -> String {
	let mut user_requests = Vec::new();

	for message in messages {
		// Only collect user messages
		if message.role != "user" {
			continue;
		}

		// Skip system-generated continuation requests
		if message
			.content
			.contains("CRITICAL: Session approaching token limits")
		{
			continue;
		}

		// Skip empty or very short messages
		let content = message.content.trim();
		if content.is_empty() || content.len() < 10 {
			continue;
		}

		// Skip session commands (starting with /)
		if content.starts_with('/') {
			continue;
		}

		// Truncate very long requests for readability
		let truncated_content = if content.len() > 200 {
			format!("{}...", content.chars().take(200).collect::<String>())
		} else {
			content.to_string()
		};

		user_requests.push(truncated_content);
	}

	// Keep only the last 5-7 requests to avoid overwhelming context
	if user_requests.len() > 7 {
		user_requests = user_requests[user_requests.len() - 7..].to_vec();
	}

	if user_requests.is_empty() {
		return "No specific user requests found in session history.".to_string();
	}

	// Format as numbered list with proactive language
	let mut result = String::new();
	for (i, request) in user_requests.iter().enumerate() {
		result.push_str(&format!("{}. {}\n", i + 1, request));
	}

	result
}

/// Parse file context requirements from AI summary response
/// Expected format: filename:startline:endline (in code blocks or specific sections)
fn parse_file_contexts(summary_content: &str) -> Vec<(String, usize, usize)> {
	let mut contexts = Vec::new();

	// Pre-compile regex patterns outside loops
	let code_block_pattern =
		Regex::new(r"```(?:\w+)?\s*\n((?:[^\n`]+:[0-9]+:[0-9]+\s*\n?)+)\s*```").unwrap();
	let file_pattern = Regex::new(r"^([^:\n]+):(\d+):(\d+)\s*$").unwrap();
	let general_file_pattern = Regex::new(r"(?:^|\s|-)([^\s\n:]+):(\d+):(\d+)").unwrap();
	let fallback_pattern = Regex::new(r"([^\s:]+):(\d+):(\d+)").unwrap();

	// First, try to find contexts within code blocks (preferred format)
	for code_block in code_block_pattern.captures_iter(summary_content) {
		if let Some(block_content) = code_block.get(1) {
			// Parse each line in the code block
			for line in block_content.as_str().lines() {
				let line = line.trim();
				if let Some(captures) = file_pattern.captures(line) {
					if let (Some(filename), Some(start_str), Some(end_str)) =
						(captures.get(1), captures.get(2), captures.get(3))
					{
						if let (Ok(start_line), Ok(end_line)) = (
							start_str.as_str().parse::<usize>(),
							end_str.as_str().parse::<usize>(),
						) {
							let filename = filename.as_str().trim().to_string();

							// Validate line range and filename
							if start_line > 0
								&& end_line >= start_line
								&& end_line <= 10000 && !filename.is_empty()
							{
								contexts.push((filename, start_line, end_line));
							}
						}
					}
				}
			}
		}
	}

	// If no code blocks found, fall back to looking for patterns in REQUIRED FILE CONTEXTS section
	if contexts.is_empty() {
		// Look for the specific section header and parse content after it using simple string operations
		if let Some(section_start) = summary_content.find("## REQUIRED FILE CONTEXTS") {
			let content_after_header = &summary_content[section_start..];

			// Find the end of this section (next ## header or end of text)
			let section_end = content_after_header
				.find("\n## ")
				.unwrap_or(content_after_header.len());

			let section_content = &content_after_header[..section_end];

			// More flexible pattern for general text (handles paths with spaces/special chars)
			for captures in general_file_pattern.captures_iter(section_content) {
				if let (Some(filename), Some(start_str), Some(end_str)) =
					(captures.get(1), captures.get(2), captures.get(3))
				{
					if let (Ok(start_line), Ok(end_line)) = (
						start_str.as_str().parse::<usize>(),
						end_str.as_str().parse::<usize>(),
					) {
						let filename = filename.as_str().trim().to_string();

						// Validate line range and filename
						if start_line > 0
							&& end_line >= start_line
							&& end_line <= 10000 && !filename.is_empty()
						{
							contexts.push((filename, start_line, end_line));
						}
					}
				}
			}
		}
	}

	// Final fallback: look anywhere in the content (most permissive)
	if contexts.is_empty() {
		for captures in fallback_pattern.captures_iter(summary_content) {
			if let (Some(filename), Some(start_str), Some(end_str)) =
				(captures.get(1), captures.get(2), captures.get(3))
			{
				if let (Ok(start_line), Ok(end_line)) = (
					start_str.as_str().parse::<usize>(),
					end_str.as_str().parse::<usize>(),
				) {
					let filename = filename.as_str().to_string();

					// Validate line range
					if start_line > 0 && end_line >= start_line && end_line <= 10000 {
						contexts.push((filename, start_line, end_line));
					}
				}
			}
		}
	}

	// Remove duplicates while preserving order
	let mut unique_contexts = Vec::new();
	for context in contexts {
		if !unique_contexts.contains(&context) {
			unique_contexts.push(context);
		}
	}

	// Limit to maximum 10 file contexts for performance
	unique_contexts.truncate(10);
	unique_contexts
}

/// Read file content for specified line ranges with 1-indexed line numbers
fn read_file_context(filepath: &str, start_line: usize, end_line: usize) -> Result<String> {
	use std::fs;
	use std::io::{BufRead, BufReader};

	// Validate file exists and is readable
	if !Path::new(filepath).exists() {
		return Ok(format!("// File not found: {}", filepath));
	}

	let file = fs::File::open(filepath)?;
	let reader = BufReader::new(file);
	let mut result = String::new();

	result.push_str(&format!(
		"=== {} (lines {}-{}) ===\n",
		filepath, start_line, end_line
	));

	for (line_num, line_result) in reader.lines().enumerate() {
		let line_number = line_num + 1; // Convert to 1-indexed

		if line_number < start_line {
			continue;
		}

		if line_number > end_line {
			break;
		}

		match line_result {
			Ok(line_content) => {
				result.push_str(&format!("{}: {}\n", line_number, line_content));
			}
			Err(_) => {
				result.push_str(&format!("{}: // Error reading line\n", line_number));
			}
		}
	}

	result.push('\n');
	Ok(result)
}

/// Generate file context content from parsed file requirements
fn generate_file_context_content(file_contexts: &[(String, usize, usize)]) -> String {
	if file_contexts.is_empty() {
		return "No specific file context requested.".to_string();
	}

	let mut context_content = String::new();
	context_content.push_str("FILE CONTEXT:\n\n");

	for (filepath, start_line, end_line) in file_contexts {
		match read_file_context(filepath, *start_line, *end_line) {
			Ok(file_content) => {
				context_content.push_str(&file_content);
			}
			Err(e) => {
				context_content.push_str(&format!(
					"=== {} (lines {}-{}) ===\n// Error reading file: {}\n\n",
					filepath, start_line, end_line, e
				));
			}
		}
	}

	context_content
}

#[cfg(test)]
mod tests {
	use super::*;

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
		assert_eq!(contexts[0], ("src/main.rs".to_string(), 1, 50));
		assert_eq!(contexts[1], ("src/lib.rs".to_string(), 100, 150));
		assert_eq!(contexts[2], ("config/settings.toml".to_string(), 10, 20));
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
		assert_eq!(contexts[0], ("src/session/mod.rs".to_string(), 200, 300));
		assert_eq!(contexts[1], ("tests/integration.rs".to_string(), 1, 100));
	}

	#[test]
	fn test_parse_file_contexts_fallback() {
		let summary = r#"
We need to look at src/core.rs:50:100 and also check lib/utils.rs:1:25 for the implementation.
		"#;

		let contexts = parse_file_contexts(summary);
		assert_eq!(contexts.len(), 2);
		assert_eq!(contexts[0], ("src/core.rs".to_string(), 50, 100));
		assert_eq!(contexts[1], ("lib/utils.rs".to_string(), 1, 25));
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
		assert_eq!(
			contexts[0],
			(
				"src/session/chat/session_continuation.rs".to_string(),
				100,
				200
			)
		);
		assert_eq!(contexts[1], ("src/config/mod.rs".to_string(), 50, 100));
		assert_eq!(
			contexts[2],
			("tests/integration_test.rs".to_string(), 1, 50)
		);
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
}
