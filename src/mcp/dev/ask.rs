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

// Ask tool — halts AI execution and prompts the user for input.
// Works in both CLI (reads stdin) and WebSocket (sends InputRequest, awaits InputResponse).

use crate::mcp::{McpFunction, McpToolCall, McpToolResult};
use anyhow::Result;
use serde_json::json;
use std::sync::Mutex;
use tokio::sync::oneshot;

/// A pending ask request: the question to display and a channel to send the answer back.
pub struct AskRequest {
	pub question: String,
	pub answer_tx: oneshot::Sender<String>,
}

// Global channel: ask tool → execute_tools_with_context.
// std::sync::Mutex is fine here — the lock is never held across await points.
lazy_static::lazy_static! {
	static ref ASK_TX: tokio::sync::mpsc::UnboundedSender<AskRequest> = {
		let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<AskRequest>();
		*ASK_RX.lock().unwrap() = Some(rx);
		tx
	};
	static ref ASK_RX: Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<AskRequest>>> =
		Mutex::new(None);

	/// Pending WS answer slot — std::sync::Mutex, never held across await.
	static ref WS_PENDING_ANSWER: Mutex<Option<oneshot::Sender<String>>> =
		Mutex::new(None);
}

/// Take the global ask receiver. Called once per session before tool execution starts.
/// Returns None if already taken (only one consumer allowed).
pub fn take_ask_receiver() -> Option<tokio::sync::mpsc::UnboundedReceiver<AskRequest>> {
	// Touch ASK_TX first so the channel + receiver are created before we take the RX.
	let _ = &*ASK_TX;
	ASK_RX.lock().unwrap().take()
}

/// Called by the WS server when it receives a ClientMessage::InputResponse.
/// Routes the answer to the blocked ask tool task.
pub fn deliver_ws_answer(answer: String) {
	if let Some(tx) = WS_PENDING_ANSWER.lock().unwrap().take() {
		let _ = tx.send(answer);
	}
}

/// For WebSocket mode: store the answer_tx in the global slot and send an InputRequest
/// server message via the MCP notification sender so the WS server forwards it to the client.
pub fn send_ws_input_request(req: AskRequest) {
	// Store the answer channel so deliver_ws_answer can route the reply
	*WS_PENDING_ANSWER.lock().unwrap() = Some(req.answer_tx);

	// Send InputRequest to the client via the notification channel
	let msg =
		crate::websocket::ServerMessage::InputRequest(crate::websocket::InputRequestPayload {
			question: req.question,
			session_id: String::new(), // filled in by the WS server if needed
		});
	crate::mcp::process::send_notification_message(msg);
}

/// Execute the ask tool: send the question to the session loop and await the user's answer.
pub async fn execute_ask(call: &McpToolCall) -> Result<McpToolResult> {
	let question = match call.parameters.get("question") {
		Some(serde_json::Value::String(q)) if !q.trim().is_empty() => q.clone(),
		Some(_) => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"'question' parameter must be a non-empty string".to_string(),
			))
		}
		None => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"missing required parameter: 'question'".to_string(),
			))
		}
	};

	let (answer_tx, answer_rx) = oneshot::channel::<String>();

	// Send the request to the session loop (CLI or WS handler)
	if ASK_TX
		.send(AskRequest {
			question,
			answer_tx,
		})
		.is_err()
	{
		return Ok(McpToolResult::error(
			call.tool_name.clone(),
			call.tool_id.clone(),
			"ask channel unavailable — no session loop is listening".to_string(),
		));
	}

	// Halt this tool task until the session loop delivers the answer
	match answer_rx.await {
		Ok(answer) => Ok(McpToolResult::success(
			call.tool_name.clone(),
			call.tool_id.clone(),
			answer,
		)),
		Err(_) => Ok(McpToolResult::error(
			call.tool_name.clone(),
			call.tool_id.clone(),
			"ask request was dropped before an answer was received".to_string(),
		)),
	}
}

/// MCP function definition for the ask tool.
pub fn get_ask_function() -> McpFunction {
	McpFunction {
		name: "ask".to_string(),
		description: "Pause execution and ask the user a clarification question. Use ONLY when you genuinely cannot proceed without human input — missing requirement, ambiguous instruction, or a decision that only the user can make. Do NOT use for routine confirmations or when you can make a reasonable assumption. The question must be fully self-contained: include all relevant context, file paths, options, and references so the user can answer without looking anything up. Works in both CLI (reads stdin) and WebSocket (sends InputRequest, awaits InputResponse).".to_string(),
		parameters: json!({
			"type": "object",
			"properties": {
				"question": {
					"type": "string",
					"description": "The question to display to the user. Must be fully self-contained with all context, options, and references needed to answer it. Be specific: state what you already know, what is unclear, and what decision or information you need."
				}
			},
			"required": ["question"]
		}),
	}
}
