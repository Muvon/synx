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

// System prompt construction and compression hint injection

use std::path::Path;

pub async fn create_system_prompt(
	project_dir: &Path,
	config: &crate::config::Config,
	mode: &str,
) -> String {
	// Get mode-specific configuration
	let (_, mcp_config, _, _, system_prompt) = config.get_role_config(mode);

	// For developer role, process placeholders to add project context
	let mut prompt =
		crate::session::helper_functions::process_placeholders_async(system_prompt, project_dir)
			.await;

	// Add MCP tools information if enabled
	if !mcp_config.server_refs.is_empty() {
		let config_for_role = config.get_merged_config_for_role(mode);
		let functions = crate::mcp::get_available_functions(&config_for_role).await;
		if !functions.is_empty() {
			prompt.push_str("\n\nYou have access to the following tools:");

			for function in &functions {
				prompt.push_str(&format!(
					"\n\n- {} - {}",
					function.name, function.description
				));
			}
		}
	}

	prompt
}

/// Add compression context hints to system prompt for resumed sessions.
/// Informs the AI about compression state to improve reasoning with compressed context.
pub fn add_compression_hints_to_prompt(
	prompt: &mut String,
	compression_stats: &crate::session::CompressionStats,
) {
	if compression_stats.total_compressions() == 0 {
		return;
	}

	prompt.push_str(&format!(
		"\n\n## CONTEXT COMPRESSION ACTIVE\n\
		- {} compressions performed\n\
		- {} tokens saved ({:.1}% reduction)\n\
		- Compressed sections marked with [COMPRESSED: id]\n\
		- Technical details preserved verbatim in TECHNICAL sections\n\
		- Focus on recent uncompressed messages for current context",
		compression_stats.total_compressions(),
		compression_stats.total_tokens_saved,
		compression_stats.avg_compression_ratio() * 100.0
	));
}
