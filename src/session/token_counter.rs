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

// Token counting utilities

use std::sync::OnceLock;
use tiktoken_rs::{cl100k_base, CoreBPE};

// Global tokenizer instance - created once and reused
static TOKENIZER: OnceLock<CoreBPE> = OnceLock::new();

// Get or initialize the global tokenizer instance
fn get_tokenizer() -> &'static CoreBPE {
	TOKENIZER.get_or_init(|| {
		cl100k_base().unwrap_or_else(|_| {
			// Fallback - this shouldn't happen in practice
			panic!("Failed to initialize tokenizer")
		})
	})
}

// Simple token counter that uses tiktoken to estimate token counts
pub fn estimate_tokens(text: &str) -> usize {
	// Use the cached global tokenizer
	let tokenizer = get_tokenizer();
	let tokens = tokenizer.encode_ordinary(text);
	tokens.len()
}

// Estimate tokens for a full message list
pub fn estimate_message_tokens(messages: &[crate::session::Message]) -> usize {
	let mut total = 0;

	for msg in messages {
		// Add ~4 tokens for role
		total += 4;

		// Add content tokens
		total += estimate_tokens(&msg.content);
	}

	// Add some overhead for message formatting
	total += messages.len() * 2;

	total
}

// Estimate tokens for full context including system prompt and tools
// This provides accurate estimates that match what's actually sent to API providers
pub fn estimate_full_context_tokens(
	messages: &[crate::session::Message],
	system_prompt: Option<&str>,
	tools: Option<&[crate::mcp::McpFunction]>,
) -> usize {
	// Start with basic message tokens
	let mut total = estimate_message_tokens(messages);

	// Add system prompt tokens if present
	if let Some(prompt) = system_prompt {
		total += estimate_tokens(prompt);
		// Add API formatting overhead for system message
		total += 10;
	}

	// Add tool definition tokens if present
	if let Some(tool_list) = tools {
		for tool in tool_list {
			// Estimate tokens for tool definition JSON
			// Create a simplified representation of the tool for token counting
			let tool_json = serde_json::json!({
				"name": tool.name,
				"description": tool.description,
				"input_schema": tool.parameters
			});
			let tool_str = serde_json::to_string(&tool_json).unwrap_or_default();
			total += estimate_tokens(&tool_str);
		}
		// Add JSON formatting overhead per tool (arrays, brackets, etc.)
		total += tool_list.len() * 5;
		// Add tools array overhead
		total += 10;
	}

	total
}
/// Calculate minimum tokens required for a session with given role and config
/// This includes system prompt + tool definitions + safety margin
pub async fn calculate_minimum_session_tokens(
	config: &crate::config::Config,
	role: &str,
) -> anyhow::Result<usize> {
	// Get system prompt for the role
	let (_, _, _, _, system_prompt) = config.get_role_config(role);
	let system_tokens = estimate_tokens(system_prompt);

	// Get tool definitions tokens
	let tool_tokens = if !config.mcp.servers.is_empty() {
		let tools = crate::mcp::get_available_functions(config).await;
		let mut total = 0;
		for tool in &tools {
			let tool_json = serde_json::json!({
				"name": tool.name,
				"description": tool.description,
				"input_schema": tool.parameters
			});
			let tool_str = serde_json::to_string(&tool_json).unwrap_or_default();
			total += estimate_tokens(&tool_str);
		}
		total + (tools.len() * 5) + 10 // JSON overhead
	} else {
		0
	};

	Ok(system_tokens + tool_tokens)
}

/// Validate that max_session_tokens_threshold is sufficient for role requirements
pub async fn validate_session_token_threshold(
	config: &crate::config::Config,
	role: &str,
) -> anyhow::Result<()> {
	if config.max_session_tokens_threshold == 0 {
		return Ok(()); // Disabled, no validation needed
	}

	let minimum_tokens = calculate_minimum_session_tokens(config, role).await?;
	let threshold = config.max_session_tokens_threshold;

	// Get detailed breakdown for error message
	let (_, _, _, _, system_prompt) = config.get_role_config(role);
	let system_tokens = estimate_tokens(system_prompt);
	let tool_tokens = minimum_tokens - system_tokens;

	// Apply 2x safety check
	if minimum_tokens * 2 > threshold {
		return Err(anyhow::anyhow!(
			"max_session_tokens_threshold ({}) is too low for role '{}'\n\
			 Minimum required: {} tokens (system prompt + tools)\n\
			 Recommended minimum: {} tokens (2x safety margin)\n\
			 \n\
			 Breakdown:\n\
			 - System prompt: {} tokens\n\
			 - Tool definitions: {} tokens\n\
			 - Safety margin: 2x multiplier\n\
			 \n\
			 Please increase max_session_tokens_threshold to at least {}",
			threshold,
			role,
			minimum_tokens,
			minimum_tokens * 2,
			system_tokens,
			tool_tokens,
			minimum_tokens * 2
		));
	}

	// Warn if threshold is close to minimum (less than 3x)
	if minimum_tokens * 3 > threshold {
		crate::log_info!(
			"⚠️  max_session_tokens_threshold ({}) is close to minimum requirements ({} tokens). \
			 Consider increasing for better session continuity.",
			threshold,
			minimum_tokens
		);
	}

	Ok(())
}
