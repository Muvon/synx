// Copyright 2026 Muvon Un Limited
//
// Licensed under the Apache License, Version 2.0 (the "License")

//! Lesson extraction: calls LLM to analyze a session transcript and extract
//! generalizable lessons, then stores them via the configured backend.

use super::backend::create_backend;
use super::Lesson;
use crate::config::Config;
use crate::session::chat::ChatSession;
use anyhow::Result;

const EXTRACTION_SYSTEM_PROMPT: &str = r#"# Task
Extract generalizable lessons from the conversation transcript below that would help an AI assistant avoid repeating mistakes and reuse successful patterns in future sessions on the same project.

# Rules
1. Extract ONLY lessons that are NEW — not already in the EXISTING LESSONS list.
2. Each lesson must be a single, self-contained fact or rule — no references to "this session" or "the user said".
3. Prioritize (highest to lowest): corrections by the user > patterns that succeeded > domain-specific facts > workflow preferences.
4. Skip anything obvious from reading source code, generic programming advice, or one-off task details.
5. Output 0-5 lessons. If nothing is worth remembering, output the word NONE and stop.

# Confidence Levels
- high: User explicitly stated or corrected this.
- medium: Inferred from a pattern that worked without correction.
- low: Observed once, may not generalize.

# Existing Lessons (DO NOT duplicate)
{existing_lessons}

# Output Format
For each lesson, use exactly:
<lesson confidence="high|medium|low" tags="keyword1,keyword2">
The lesson text — concise, actionable, generalizable.
</lesson>"#;

/// Extract lessons from a session and store them via the backend.
///
/// Called from `/done` (always) and auto-compaction (when enough user messages).
/// Returns the number of lessons stored.
pub async fn extract_and_store_lessons(
	session: &mut ChatSession,
	config: &Config,
	role: &str,
	project: &str,
	operation_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<usize> {
	let learning = &config.learning;
	if !learning.enabled {
		return Ok(0);
	}

	let backend = create_backend(learning);

	// Retrieve existing lessons for dedup
	let existing = backend
		.retrieve_all(role, project, config)
		.await
		.unwrap_or_default();
	let existing_text = if existing.is_empty() {
		"(none)".to_string()
	} else {
		existing
			.iter()
			.map(|l| format!("- [{}] {}", l.confidence, l.content))
			.collect::<Vec<_>>()
			.join("\n")
	};

	// Build transcript from session messages
	let transcript = build_transcript(&session.session.messages);
	if transcript.is_empty() {
		return Ok(0);
	}

	// Build prompt
	let system = EXTRACTION_SYSTEM_PROMPT.replace("{existing_lessons}", &existing_text);

	// Call LLM
	let response = call_learning_llm(
		session,
		config,
		&learning.model,
		system,
		transcript,
		operation_rx,
	)
	.await?;

	// Parse lessons
	let lessons = parse_lesson_tags(&response, role, project, &session.session.info.name);
	if lessons.is_empty() {
		return Ok(0);
	}

	// Store each
	let mut stored = 0;
	for lesson in &lessons {
		if let Err(e) = backend.store(lesson, config).await {
			crate::log_debug!("Failed to store lesson: {}", e);
		} else {
			stored += 1;
		}
	}

	Ok(stored)
}

/// Build a compact transcript from session messages.
fn build_transcript(messages: &[crate::session::Message]) -> String {
	let mut transcript = String::new();
	for msg in messages {
		if msg.role == "system" {
			continue;
		}
		let role_label = match msg.role.as_str() {
			"user" => "USER",
			"assistant" => "ASSISTANT",
			"tool" => "TOOL",
			_ => continue,
		};

		// Truncate long messages to keep transcript manageable
		let content = if msg.content.len() > 500 {
			format!("{}...[truncated]", &msg.content[..500])
		} else {
			msg.content.clone()
		};

		transcript.push_str(&format!("[{}]: {}\n\n", role_label, content));
	}
	transcript
}

/// Parse `<lesson>` tags from LLM response.
fn parse_lesson_tags(response: &str, role: &str, project: &str, source: &str) -> Vec<Lesson> {
	let mut lessons = Vec::new();
	let now = chrono::Utc::now().to_rfc3339();

	// Find all <lesson ...>...</lesson> blocks
	let mut remaining = response;
	while let Some(start) = remaining.find("<lesson") {
		let after_tag = &remaining[start..];
		let Some(close_bracket) = after_tag.find('>') else {
			break;
		};
		let attrs = &after_tag[7..close_bracket]; // between <lesson and >
		let after_open = &after_tag[close_bracket + 1..];
		let Some(end_tag) = after_open.find("</lesson>") else {
			break;
		};
		let content = after_open[..end_tag].trim();

		if !content.is_empty() {
			let confidence = extract_attr(attrs, "confidence").unwrap_or("medium".into());
			let tags_str = extract_attr(attrs, "tags").unwrap_or_default();
			let tags: Vec<String> = tags_str
				.split(',')
				.map(|t| t.trim().to_string())
				.filter(|t| !t.is_empty())
				.collect();

			let importance = match confidence.as_str() {
				"high" => 0.8,
				"medium" => 0.5,
				"low" => 0.3,
				_ => 0.5,
			};

			lessons.push(Lesson {
				content: content.to_string(),
				memory_type: "learning".into(),
				importance,
				confidence,
				tags,
				source: source.to_string(),
				role: role.to_string(),
				project: project.to_string(),
				created: now.clone(),
			});
		}

		remaining = &after_open[end_tag + 9..]; // skip past </lesson>
	}

	lessons
}

/// Extract an XML attribute value: `key="value"`.
fn extract_attr(attrs: &str, key: &str) -> Option<String> {
	let pattern = format!("{}=\"", key);
	let start = attrs.find(&pattern)? + pattern.len();
	let end = attrs[start..].find('"')? + start;
	Some(attrs[start..end].to_string())
}

/// Call the learning LLM (cheap model) for extraction or retrieval prep.
pub(crate) async fn call_learning_llm(
	session: &mut ChatSession,
	config: &Config,
	model: &str,
	system_content: String,
	user_content: String,
	operation_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<String> {
	let now = crate::utils::time::now_secs();
	let messages = vec![
		crate::session::Message {
			role: "system".to_string(),
			content: system_content,
			timestamp: now,
			cached: false,
			tool_call_id: None,
			name: None,
			tool_calls: None,
			images: None,
			videos: None,
			thinking: None,
			id: None,
		},
		crate::session::Message {
			role: "user".to_string(),
			content: user_content,
			timestamp: now,
			cached: false,
			tool_call_id: None,
			name: None,
			tool_calls: None,
			images: None,
			videos: None,
			thinking: None,
			id: None,
		},
	];

	let params = crate::session::ChatCompletionWithValidationParams::new(
		&messages, model, 0.3,  // low temperature for structured output
		1.0,  // top_p
		0,    // top_k (0 = default)
		4096, // max_tokens
		config,
	)
	.with_max_retries(1)
	.with_chat_session(session)
	.with_cancellation_token(operation_rx);

	let response = crate::session::chat_completion_with_validation(params).await?;
	Ok(response.content)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_parse_lesson_tags_single() {
		let response = r#"Some preamble text.
<lesson confidence="high" tags="auth,api">
Bearer token auth is required for all endpoints
</lesson>
Some trailing text."#;

		let lessons = parse_lesson_tags(response, "developer", "octofs", "test-session");
		assert_eq!(lessons.len(), 1);
		assert_eq!(
			lessons[0].content,
			"Bearer token auth is required for all endpoints"
		);
		assert_eq!(lessons[0].confidence, "high");
		assert_eq!(lessons[0].importance, 0.8);
		assert_eq!(lessons[0].tags, vec!["auth", "api"]);
		assert_eq!(lessons[0].role, "developer");
		assert_eq!(lessons[0].project, "octofs");
	}

	#[test]
	fn test_parse_lesson_tags_multiple() {
		let response = r#"
<lesson confidence="high" tags="error">
Use custom error types not anyhow
</lesson>
<lesson confidence="low" tags="style">
User prefers single PRs
</lesson>"#;

		let lessons = parse_lesson_tags(response, "dev", "proj", "src");
		assert_eq!(lessons.len(), 2);
		assert_eq!(lessons[0].confidence, "high");
		assert_eq!(lessons[1].confidence, "low");
		assert_eq!(lessons[1].importance, 0.3);
	}

	#[test]
	fn test_parse_lesson_tags_empty_content_skipped() {
		let response = r#"<lesson confidence="high" tags="x">
</lesson>"#;
		let lessons = parse_lesson_tags(response, "dev", "proj", "src");
		assert_eq!(lessons.len(), 0);
	}

	#[test]
	fn test_parse_lesson_tags_no_lessons() {
		let response = "No lessons to extract from this session.";
		let lessons = parse_lesson_tags(response, "dev", "proj", "src");
		assert_eq!(lessons.len(), 0);
	}

	#[test]
	fn test_parse_lesson_tags_missing_confidence_defaults_medium() {
		let response = r#"<lesson tags="test">
Some lesson without confidence attr
</lesson>"#;
		let lessons = parse_lesson_tags(response, "dev", "proj", "src");
		assert_eq!(lessons.len(), 1);
		assert_eq!(lessons[0].confidence, "medium");
		assert_eq!(lessons[0].importance, 0.5);
	}

	#[test]
	fn test_extract_attr() {
		assert_eq!(
			extract_attr(r#" confidence="high" tags="a,b""#, "confidence"),
			Some("high".into())
		);
		assert_eq!(
			extract_attr(r#" confidence="high" tags="a,b""#, "tags"),
			Some("a,b".into())
		);
		assert_eq!(extract_attr(r#" confidence="high""#, "missing"), None);
	}

	#[test]
	fn test_build_transcript() {
		let messages = vec![
			crate::session::Message {
				role: "system".into(),
				content: "You are helpful".into(),
				timestamp: 0,
				cached: false,
				tool_call_id: None,
				name: None,
				tool_calls: None,
				images: None,
				videos: None,
				thinking: None,
				id: None,
			},
			crate::session::Message {
				role: "user".into(),
				content: "Fix the auth bug".into(),
				timestamp: 0,
				cached: false,
				tool_call_id: None,
				name: None,
				tool_calls: None,
				images: None,
				videos: None,
				thinking: None,
				id: None,
			},
			crate::session::Message {
				role: "assistant".into(),
				content: "I'll fix it".into(),
				timestamp: 0,
				cached: false,
				tool_call_id: None,
				name: None,
				tool_calls: None,
				images: None,
				videos: None,
				thinking: None,
				id: None,
			},
		];
		let transcript = build_transcript(&messages);
		assert!(!transcript.contains("system"));
		assert!(!transcript.contains("You are helpful"));
		assert!(transcript.contains("[USER]: Fix the auth bug"));
		assert!(transcript.contains("[ASSISTANT]: I'll fix it"));
	}
}
