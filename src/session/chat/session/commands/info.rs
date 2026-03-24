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

// Info command handler

use super::super::core::ChatSession;
use super::{CommandOutput, CommandResult};
use anyhow::Result;

pub fn handle_info(session: &ChatSession) -> Result<CommandResult> {
	// Get session info - for now, extract key fields
	// The full JSON is available via to_json() for WebSocket
	let info = &session.session.info;

	let tokens_used = info.input_tokens + info.output_tokens;
	// Estimate cache savings: if we had paid full price for cache_read_tokens at the same
	// rate as non-cached input tokens, how much would it have cost?
	let cache_savings =
		if info.cache_read_tokens > 0 && info.input_tokens > 0 && info.total_cost > 0.0 {
			let total_weighted = (info.input_tokens as f64) + (info.output_tokens as f64 * 3.0);
			if total_weighted > 0.0 {
				let estimated_input_rate = info.total_cost / total_weighted;
				info.cache_read_tokens as f64 * estimated_input_rate
			} else {
				0.0
			}
		} else {
			0.0
		};

	let compression_stats = if info.compression_stats.total_compressions() > 0 {
		Some(info.compression_stats.clone())
	} else {
		None
	};

	Ok(CommandResult::HandledWithOutput(CommandOutput::Info {
		session_name: info.name.clone(),
		model: info.model.clone(),
		role: session.role.clone(),
		tokens_used,
		tokens_cached: info.cache_read_tokens,
		tokens_cache_write: info.cache_write_tokens,
		total_cost: info.total_cost,
		cache_savings,
		compression_stats,
	}))
}
