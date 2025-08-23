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

use anyhow::Result;
use colored::Colorize;

/// Handle /plan command - display current plan stored in MCP plan tool
pub async fn handle_plan() -> Result<bool> {
	// Access the plan storage directly from the MCP plan tool
	match crate::mcp::dev::plan::core::get_current_plan_display().await {
		Ok(plan_display) => {
			println!("{}", plan_display);
		}
		Err(e) => {
			println!(
				"{}: {}",
				"No active plan".bright_yellow(),
				e.to_string().dimmed()
			);
			println!(
				"💡 Use the {} MCP tool only for complex, multi-step tasks that require structured breakdown",
				"plan".bright_cyan()
			);
			println!("For simple tasks, just execute them directly without creating a plan");
		}
	}

	Ok(false) // Command handled, don't exit session
}
