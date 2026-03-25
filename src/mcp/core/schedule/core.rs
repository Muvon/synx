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

//! Core schedule tool: global store, MCP handler, and session-loop helpers.

use super::storage::{parse_when, ScheduleEntry, ScheduleStore};
use crate::mcp::{McpFunction, McpToolCall, McpToolResult};
use anyhow::Result;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, OnceLock};

// ---------------------------------------------------------------------------
// Global store
// ---------------------------------------------------------------------------

static SCHEDULE_STORE: OnceLock<Arc<Mutex<ScheduleStore>>> = OnceLock::new();

/// Get schedule storage for the current context.
/// Returns session-scoped storage if in a session, otherwise CLI global.
fn get_store() -> Arc<Mutex<ScheduleStore>> {
	if let Some(session_id) = crate::session::context::current_session_id() {
		crate::session::context::get_schedule_storage(&session_id)
	} else {
		SCHEDULE_STORE
			.get_or_init(|| Arc::new(Mutex::new(ScheduleStore::new())))
			.clone()
	}
}

// ---------------------------------------------------------------------------
// Session-loop helpers (called from main_loop.rs)
// ---------------------------------------------------------------------------

/// Flush all due entries into the session inbox.  Call once per loop iteration
/// so the inbox is the single source of truth for all injected messages.
pub fn flush_due_to_inbox() {
	let store = get_store();
	while let Some(entry) = store.lock().unwrap().pop_due() {
		crate::session::inbox::push_inbox_message(crate::session::inbox::InboxMessage {
			source: crate::session::inbox::InboxSource::Schedule {
				id: entry.id.clone(),
			},
			content: entry.message,
		});
	}
}

/// Returns true if there are any pending scheduled entries.
pub fn has_pending_schedules() -> bool {
	!get_store().lock().unwrap().is_empty()
}

/// Returns a future that resolves when the next scheduled entry is due.
/// Returns `futures::future::pending()` (never resolves) when the store is empty —
/// so the select! arm is a no-op when nothing is scheduled.
pub async fn next_schedule_sleep() {
	let duration = get_store().lock().unwrap().next_due_duration();
	match duration {
		Some(d) => tokio::time::sleep(d).await,
		None => futures::future::pending::<()>().await,
	}
}

// ---------------------------------------------------------------------------
// MCP tool definition
// ---------------------------------------------------------------------------

pub fn get_schedule_function() -> McpFunction {
	McpFunction {
		name: "schedule".to_string(),
		description: r#"Schedule a message to be automatically injected as a user message into the current session at a future time. The session keeps running until all scheduled messages have fired — nothing is blocked.

Each scheduled entry fires exactly once and is automatically removed after triggering. To repeat a task, schedule it again.

**commands:**
- `add`    — schedule a new message (requires `when` and `message`; `description` recommended)
- `list`   — show all pending scheduled entries with IDs, trigger times, and countdown
- `remove` — cancel a scheduled entry by `id`
- `edit`   — update an existing entry by `id` (any of `when`, `message`, `description`)

**`when` format** (local timezone):
- Relative: `"in 5m"`, `"in 2h"`, `"in 1h30m"`, `"in 90s"`, `"in 2h 30m"`
- Time today: `"15:30"`, `"3:30pm"`, `"9am"` (if already past, fires tomorrow)
- Exact datetime: `"2026-03-22 15:30"`

**`description`** — what this task is about (shown in list, helps you track intent).

**`message`** — the EXACT text that will be injected verbatim as a user message when the timer fires. Write it as if a human typed it: include all context the AI will need to act on it, because the AI will see only this message at trigger time with no other hint about why it arrived."#.to_string(),
		parameters: json!({
			"type": "object",
			"properties": {
				"command": {
					"type": "string",
					"enum": ["add", "list", "remove", "edit"],
					"description": "Action to perform."
				},
				"when": {
					"type": "string",
					"description": "When to fire. Relative: 'in 5m', 'in 2h', 'in 1h30m', 'in 90s'. Time today: '15:30', '3:30pm'. Exact: '2026-03-22 15:30'. Required for add; optional for edit."
				},
				"message": {
					"type": "string",
					"description": "The exact text injected verbatim as a user message when the timer fires. Write it with full context — the AI will see only this text at trigger time. Required for add; optional for edit."
				},
				"description": {
					"type": "string",
					"description": "Human-readable description of what this scheduled task is about. Shown in list output. Recommended for add; optional for edit."
				},
				"id": {
					"type": "string",
					"description": "Entry ID (from list output). Required for remove and edit."
				}
			},
			"required": ["command"]
		}),
	}
}

// ---------------------------------------------------------------------------
// MCP tool handler
// ---------------------------------------------------------------------------

pub async fn execute_schedule_tool(call: &McpToolCall) -> Result<McpToolResult> {
	let command = match call.parameters.get("command") {
		Some(Value::String(s)) if !s.trim().is_empty() => s.clone(),
		Some(_) => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"'command' must be a non-empty string".to_string(),
			))
		}
		None => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"missing required parameter 'command'".to_string(),
			))
		}
	};

	match command.as_str() {
		"add" => handle_add(call),
		"list" => handle_list(call),
		"remove" => handle_remove(call),
		"edit" => handle_edit(call),
		other => Ok(McpToolResult::error(
			call.tool_name.clone(),
			call.tool_id.clone(),
			format!("unknown command '{}' — use: add, list, remove, edit", other),
		)),
	}
}

// ---------------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------------

fn handle_add(call: &McpToolCall) -> Result<McpToolResult> {
	let when_str = match call.parameters.get("when") {
		Some(Value::String(s)) if !s.trim().is_empty() => s.clone(),
		Some(_) => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"'when' must be a non-empty string".to_string(),
			))
		}
		None => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"missing required parameter 'when' for add".to_string(),
			))
		}
	};

	let message = match call.parameters.get("message") {
		Some(Value::String(s)) if !s.trim().is_empty() => s.clone(),
		Some(_) => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"'message' must be a non-empty string".to_string(),
			))
		}
		None => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"missing required parameter 'message' for add".to_string(),
			))
		}
	};

	let description = match call.parameters.get("description") {
		Some(Value::String(s)) if !s.trim().is_empty() => s.clone(),
		_ => String::new(),
	};

	let trigger_at = match parse_when(&when_str) {
		Ok(t) => t,
		Err(e) => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				format!("invalid 'when' value: {}", e),
			))
		}
	};

	let entry = ScheduleEntry::new(description.clone(), message, trigger_at);
	let id = entry.id.clone();
	let countdown = entry.countdown();
	let trigger_fmt = entry.trigger_at.format("%Y-%m-%d %H:%M:%S").to_string();

	get_store().lock().unwrap().add(entry);

	let desc_line = if description.is_empty() {
		String::new()
	} else {
		format!("\nDescription: {}", description)
	};

	Ok(McpToolResult::success(
		call.tool_name.clone(),
		call.tool_id.clone(),
		format!(
			"✅ Scheduled [{}] at {} ({}){}\n\nThe message will be injected as a user message when the timer fires.",
			id, trigger_fmt, countdown, desc_line
		),
	))
}

fn handle_list(call: &McpToolCall) -> Result<McpToolResult> {
	let store = get_store();
	let guard = store.lock().unwrap();
	let entries = guard.entries();

	if entries.is_empty() {
		return Ok(McpToolResult::success(
			call.tool_name.clone(),
			call.tool_id.clone(),
			"No scheduled entries.".to_string(),
		));
	}

	let mut lines = vec![format!("{} scheduled entries:\n", entries.len())];
	for entry in entries {
		let trigger_fmt = entry.trigger_at.format("%Y-%m-%d %H:%M:%S").to_string();
		let desc = if entry.description.is_empty() {
			"(no description)".to_string()
		} else {
			entry.description.clone()
		};
		// Truncate message preview to 80 chars.
		let preview = if entry.message.len() > 80 {
			format!("{}…", &entry.message[..80])
		} else {
			entry.message.clone()
		};
		lines.push(format!(
			"[{}] {} ({}) — {}\n  Message: {}",
			entry.id,
			trigger_fmt,
			entry.countdown(),
			desc,
			preview
		));
	}

	Ok(McpToolResult::success(
		call.tool_name.clone(),
		call.tool_id.clone(),
		lines.join("\n"),
	))
}

fn handle_remove(call: &McpToolCall) -> Result<McpToolResult> {
	let id = match call.parameters.get("id") {
		Some(Value::String(s)) if !s.trim().is_empty() => s.clone(),
		Some(_) => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"'id' must be a non-empty string".to_string(),
			))
		}
		None => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"missing required parameter 'id' for remove".to_string(),
			))
		}
	};

	let removed = get_store().lock().unwrap().remove(&id);
	if removed {
		Ok(McpToolResult::success(
			call.tool_name.clone(),
			call.tool_id.clone(),
			format!("✅ Removed scheduled entry [{}].", id),
		))
	} else {
		Ok(McpToolResult::error(
			call.tool_name.clone(),
			call.tool_id.clone(),
			format!("No scheduled entry found with id '{}'.", id),
		))
	}
}

fn handle_edit(call: &McpToolCall) -> Result<McpToolResult> {
	let id = match call.parameters.get("id") {
		Some(Value::String(s)) if !s.trim().is_empty() => s.clone(),
		Some(_) => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"'id' must be a non-empty string".to_string(),
			))
		}
		None => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"missing required parameter 'id' for edit".to_string(),
			))
		}
	};

	let new_when = match call.parameters.get("when") {
		Some(Value::String(s)) if !s.trim().is_empty() => match parse_when(s) {
			Ok(t) => Some(t),
			Err(e) => {
				return Ok(McpToolResult::error(
					call.tool_name.clone(),
					call.tool_id.clone(),
					format!("invalid 'when' value: {}", e),
				))
			}
		},
		_ => None,
	};

	let new_message = match call.parameters.get("message") {
		Some(Value::String(s)) if !s.trim().is_empty() => Some(s.clone()),
		_ => None,
	};

	let new_description = match call.parameters.get("description") {
		Some(Value::String(s)) if !s.trim().is_empty() => Some(s.clone()),
		_ => None,
	};

	if new_when.is_none() && new_message.is_none() && new_description.is_none() {
		return Ok(McpToolResult::error(
			call.tool_name.clone(),
			call.tool_id.clone(),
			"edit requires at least one of: when, message, description".to_string(),
		));
	}
	let store = get_store();
	let updated = store
		.lock()
		.unwrap()
		.edit(&id, new_description, new_message, new_when);

	if updated {
		// Read back the updated entry for confirmation.
		let store = get_store();
		let guard = store.lock().unwrap();
		let entry = guard.entries().iter().find(|e| e.id == id);
		let summary = entry
			.map(|e| {
				format!(
					" → fires at {} ({})",
					e.trigger_at.format("%Y-%m-%d %H:%M:%S"),
					e.countdown()
				)
			})
			.unwrap_or_default();

		Ok(McpToolResult::success(
			call.tool_name.clone(),
			call.tool_id.clone(),
			format!("✅ Updated scheduled entry [{}]{}.", id, summary),
		))
	} else {
		Ok(McpToolResult::error(
			call.tool_name.clone(),
			call.tool_id.clone(),
			format!("No scheduled entry found with id '{}'.", id),
		))
	}
}
