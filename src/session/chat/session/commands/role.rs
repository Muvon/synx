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

// Role switching command implementation

use super::super::core::ChatSession;
use crate::config::Config;
use anyhow::Result;
use colored::Colorize;

/// Handle /role command for runtime role switching
pub async fn handle_role(
	session: &mut ChatSession,
	config: &Config,
	params: &[&str],
) -> Result<bool> {
	if params.is_empty() {
		// Show current role
		println!(
			"{} {}",
			"Current role:".bright_cyan(),
			session.role.bright_yellow()
		);

		// Show available roles
		println!("\n{}", "Available roles:".bright_cyan());
		for role in &config.roles {
			let indicator = if role.name == session.role {
				"→".bright_green()
			} else {
				" ".normal()
			};
			println!("  {} {}", indicator, role.name.bright_white());
		}

		println!("\n💡 Usage: {} <role_name>", "/role".bright_green());
		return Ok(false);
	}

	let new_role = params[0];

	// Validate role exists
	if !config.roles.iter().any(|r| r.name == new_role) {
		println!(
			"{}: {}",
			"Invalid role".bright_red(),
			new_role.bright_yellow()
		);
		println!("\n{}", "Available roles:".bright_cyan());
		for role in &config.roles {
			println!("  {}", role.name.bright_white());
		}
		return Ok(false);
	}

	// Don't switch if already using this role
	if session.role == new_role {
		println!(
			"{} {}",
			"Already using role:".bright_yellow(),
			new_role.bright_green()
		);
		return Ok(false);
	}

	// Get new role configuration
	let (role_config, _, _, _, _) = config.get_role_config(new_role);

	// Update session role and related settings
	let old_role = session.role.clone();
	session.role = new_role.to_string();
	session.temperature = role_config.temperature;

	// Reinitialize the session for the new role (system prompt + MCP servers)
	if let Err(e) = session.reinitialize_for_role(new_role, config).await {
		// If reinitialization fails, revert the role change
		session.role = old_role.clone();
		session.temperature = config.get_role_config(&old_role).0.temperature;

		println!("{}: {}", "Failed to switch role".bright_red(), e);
		println!("{}", "Role change reverted".yellow());
		return Ok(false);
	}

	// Log the role change for session restoration
	if let Some(_session_file) = &session.session.session_file {
		let command_line = format!("/role {}", new_role);
		if let Err(e) =
			crate::session::logger::log_session_command(&session.session.info.name, &command_line)
		{
			eprintln!("Warning: Failed to log role change: {}", e);
		}
	}

	println!(
		"{} {} → {}",
		"Role switched:".bright_green(),
		old_role.bright_yellow(),
		new_role.bright_green()
	);

	// Show key changes
	println!(
		"{} {}",
		"Temperature:".blue(),
		session.temperature.to_string().bright_white()
	);

	Ok(false) // Command handled, don't exit session
}
