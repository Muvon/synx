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

// Layered response processing implementation

use crate::config::Config;
use crate::mcp::get_available_functions;
use crate::session::chat::get_animation_manager;
use crate::session::chat::session::ChatSession;
use crate::session::estimate_full_context_tokens;
use anyhow::Result;
use colored::*;

/// Run a specific workflow by name. Used by both pipeline steps and /workflow command.
pub async fn run_workflow_by_name(
	workflow_name: &str,
	input: &str,
	chat_session: &mut ChatSession,
	config: &Config,
	operation_cancelled: tokio::sync::watch::Receiver<bool>,
) -> Result<String> {
	// Ensure system message is cached before processing with layers
	let mut system_message_cached = false;
	for msg in &chat_session.session.messages {
		if msg.role == "system" && msg.cached {
			system_message_cached = true;
			break;
		}
	}

	if !system_message_cached {
		if let Ok(cached) = chat_session.session.add_cache_checkpoint(true) {
			if cached && crate::session::model_supports_caching(&chat_session.model) {
				println!(
					"{}",
					"System message has been automatically marked for caching to save tokens."
						.yellow()
				);
				let _ = chat_session.save();
			}
		}
	}

	// Use AnimationManager for animation
	let animation_manager = get_animation_manager();
	let current_cost = chat_session.session.info.total_cost;
	let max_threshold = config.max_session_tokens_threshold;

	let tools = get_available_functions(config).await;
	let current_context_tokens =
		estimate_full_context_tokens(&chat_session.session.messages, Some(&tools)) as u64;

	animation_manager
		.start_with_params(current_cost, current_context_tokens, max_threshold)
		.await;

	if config.get_log_level().is_debug_enabled() {
		println!("{}", "Using workflow processing with model-specific caching - only supported models will use caching".bright_cyan());
	} else {
		println!(
			"{}",
			format!("Using workflow: {}", workflow_name).bright_cyan()
		);
	}

	let workflow_def = config
		.workflows
		.iter()
		.find(|w| w.name == workflow_name)
		.ok_or_else(|| anyhow::anyhow!("Workflow '{}' not found", workflow_name))?
		.clone();

	let workflow_orchestrator = crate::session::workflows::WorkflowOrchestrator::new(
		workflow_def,
		workflow_name.to_string(),
	);

	let result = match workflow_orchestrator
		.execute(
			input,
			&mut chat_session.session,
			config,
			operation_cancelled,
		)
		.await
	{
		Ok((output, _progress)) => output,
		Err(e) => {
			animation_manager.stop_current().await;
			return Err(e);
		}
	};

	animation_manager.stop_current().await;
	Ok(result)
}

/// Process a response using the layered architecture (called from pipeline and legacy paths).
/// Looks up the first workflow name from the role's pipeline and executes it.
pub async fn process_layered_response(
	input: &str,
	chat_session: &mut ChatSession,
	config: &Config,
	role: &str,
	operation_cancelled: tokio::sync::watch::Receiver<bool>,
) -> Result<String> {
	if let Some(role_data) = config.role_map.get(role) {
		if let Some(pipeline) = &role_data.workflow {
			// Find first non-empty workflow name in pipeline
			if let Some(workflow_name) = pipeline.iter().find(|s| !s.is_empty()) {
				return run_workflow_by_name(
					workflow_name,
					input,
					chat_session,
					config,
					operation_cancelled,
				)
				.await;
			}
		}
	}

	// No workflow configured or all steps empty - pass through unchanged
	Ok(input.to_string())
}
