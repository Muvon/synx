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

// API preparation utilities

use super::super::context_truncation::{
	check_and_truncate_context_with_cancellation, TruncationOptions,
};
use super::core::ChatSession;
use crate::config::Config;
use crate::log_info;
use crate::session::model_supports_caching;
use anyhow::Result;
use std::sync::Arc;

// Helper function to prepare for API call (context truncation and caching)
pub async fn prepare_for_api_call(
	chat_session: &mut ChatSession,
	config: &Config,
	operation_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
	// Check if we need to truncate the context to stay within token limits
	check_and_truncate_context_with_cancellation(
		chat_session,
		config,
		TruncationOptions::default(), // Normal truncation, no defer
		Some(Arc::new(std::sync::atomic::AtomicBool::new(
			*operation_rx.borrow(),
		))),
	)
	.await?;

	// Ensure system message is cached before making API calls
	let mut system_message_cached = false;

	// Check if system message is already cached
	for msg in &chat_session.session.messages {
		if msg.role == "system" && msg.cached {
			system_message_cached = true;
			break;
		}
	}

	// If system message not already cached, add a cache checkpoint
	if !system_message_cached {
		if let Ok(cached) = chat_session.session.add_cache_checkpoint(true) {
			if cached && model_supports_caching(&chat_session.model) {
				log_info!(
					"System message has been automatically marked for caching to save tokens."
				);
				// Save the session to ensure the cached status is persisted
				let _ = chat_session.save();
			}
		}
	}

	Ok(())
}
