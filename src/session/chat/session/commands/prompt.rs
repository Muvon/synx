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

// Prompt command handler

use super::super::core::ChatSession;
use crate::config::Config;
use anyhow::Result;
use colored::Colorize;

pub async fn handle_prompt(
	session: &mut ChatSession,
	config: &Config,
	role: &str,
	params: &[&str],
) -> Result<bool> {
	// Handle /prompt command for sending predefined prompt templates
	if params.is_empty() {
		// Show available prompts
		if config.prompts.is_empty() {
			println!("{}", "No prompt templates configured.".bright_yellow());
			println!(
				"{}",
				"Prompt templates can be defined in the [[prompts]] section of your configuration."
					.bright_blue()
			);
			println!("{}", "Example configuration:".bright_cyan());
			println!(
				"{}",
				r#"[[prompts]]
name = "review"
description = "Request code review"
prompt = "Please review the code above focusing on best practices and security.""#
					.bright_white()
			);
		} else {
			println!("{}", "Available prompt templates:".bright_cyan());
			for prompt in &config.prompts {
				if let Some(description) = &prompt.description {
					println!(
						"  {} {} - {}",
						"/prompt".cyan(),
						prompt.name.bright_yellow(),
						description.bright_white()
					);
				} else {
					println!("  {} {}", "/prompt".cyan(), prompt.name.bright_yellow());
				}
			}
			println!();
			println!("{}", "Usage: /prompt <template_name>".bright_blue());
			println!("{}", "Example: /prompt review".bright_green());
		}
		return Ok(false);
	}

	let prompt_name = params[0];

	// Find the prompt configuration
	let prompt_config = if let Some(p) = config.prompts.iter().find(|p| p.name == prompt_name) {
		p
	} else {
		let available_prompts: Vec<&str> = config.prompts.iter().map(|p| p.name.as_str()).collect();
		if !available_prompts.is_empty() {
			println!(
				"{} {}",
				"Prompt template not found:".bright_red(),
				prompt_name.bright_yellow()
			);
			println!("{}", "Available templates:".bright_cyan());
			for prompt in &available_prompts {
				println!("  {}", prompt.bright_yellow());
			}
		} else {
			println!("{}", "No prompt templates configured.".bright_yellow());
		}
		return Ok(false);
	};

	// Process the prompt template (support variable substitution if needed)
	let processed_prompt = process_prompt_template(&prompt_config.prompt, config, role)?;

	// Add the prompt as a user message to the session
	if let Err(e) = session.add_user_message(&processed_prompt) {
		return Err(anyhow::anyhow!(
			"Failed to add prompt message to session: {}",
			e
		));
	}

	// Trigger processing pipeline: mark continuation so main loop processes immediately
	session.continuation_pending = true;

	println!(
		"{} {}",
		"Prompt template applied:".bright_green(),
		prompt_name.bright_yellow()
	);

	// Return false to continue session (don't exit)
	Ok(false)
}

/// Process prompt template with variable substitution
fn process_prompt_template(template: &str, _config: &Config, _role: &str) -> Result<String> {
	// For now, return template as-is
	// Future enhancement: Add variable substitution similar to system prompts
	// Could support variables like {role}, {model}, {timestamp}, etc.
	Ok(template.to_string())
}
