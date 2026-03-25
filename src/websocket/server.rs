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

// WebSocket server implementation

use super::protocol::{
	ClientMessage, CommandMessage, CostPayload, ServerMessage, SessionMessage, StatusPayload,
	UserMessage,
};
use crate::config::Config;
use crate::session::cancellation::SessionCancellation;
use crate::session::chat::session::{
	execute_api_call_and_process_response, prepare_for_api_call, process_layers_if_enabled,
	setup_and_initialize_session, setup_system_prompt_and_cache, ChatSession, GenericSessionArgs,
};
use crate::session::output::{OutputMode, WebSocketSink};
use crate::{log_debug, log_error, log_info};
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

/// WebSocket server for handling AI sessions
pub struct WebSocketServer {
	addr: SocketAddr,
	config: Arc<Config>,
	role: String,
}

impl WebSocketServer {
	/// Create a new WebSocket server
	pub fn new(host: &str, port: u16, config: Config, role: String) -> Result<Self> {
		let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
		Ok(Self {
			addr,
			config: Arc::new(config),
			role,
		})
	}

	/// Start the WebSocket server
	pub async fn start(&self) -> Result<()> {
		let listener = TcpListener::bind(&self.addr).await?;
		log_info!("WebSocket server listening on ws://{}", self.addr);
		println!("🚀 WebSocket server started on ws://{}", self.addr);
		println!("Press Ctrl+C to stop the server");

		// Active sessions map (session_id -> ChatSession)
		let sessions: Arc<Mutex<HashMap<String, ChatSession>>> =
			Arc::new(Mutex::new(HashMap::new()));

		loop {
			match listener.accept().await {
				Ok((stream, peer_addr)) => {
					log_info!("Connection accepted from {}", peer_addr);

					// Handle connection directly (sequential for simplicity)
					// This avoids Send/Sync issues and is fine for initial implementation
					if let Err(e) = handle_connection(
						stream,
						peer_addr,
						Arc::clone(&self.config),
						self.role.clone(),
						Arc::clone(&sessions),
					)
					.await
					{
						log_error!("Connection handler failed for {}: {}", peer_addr, e);
					}
				}
				Err(e) => {
					log_error!("Failed to accept connection: {}", e);
				}
			}
		}
	}
}

/// Handle a single WebSocket connection
async fn handle_connection(
	stream: TcpStream,
	peer_addr: SocketAddr,
	config: Arc<Config>,
	role: String,
	sessions: Arc<Mutex<HashMap<String, ChatSession>>>,
) -> Result<()> {
	// Accept WebSocket connection with compression enabled
	let ws_config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
		.max_message_size(Some(10 * 1024 * 1024)) // 10MB max message size
		.max_frame_size(Some(10 * 1024 * 1024)) // 10MB max frame size
		.accept_unmasked_frames(false);

	let ws_stream = tokio_tungstenite::accept_async_with_config(stream, Some(ws_config)).await?;
	log_info!("WebSocket handshake completed for {}", peer_addr);

	let (mut ws_sender, mut ws_receiver) = ws_stream.split();

	// Send welcome message
	let welcome = ServerMessage::status(
		format!("Connected to Octomind WebSocket server (role: {})", role),
		None,
	);
	send_message(&mut ws_sender, &welcome).await?;

	// Process messages sequentially (like terminal)
	while let Some(msg) = ws_receiver.next().await {
		match msg {
			Ok(Message::Text(text)) => {
				log_debug!("Received message from {}: {} bytes", peer_addr, text.len());

				// Parse client message
				let client_msg = match serde_json::from_str::<ClientMessage>(&text) {
					Ok(msg) => {
						log_debug!("Parsed message: {:?}", msg);
						msg
					}
					Err(e) => {
						log_error!("Invalid JSON from {}: {}", peer_addr, e);
						let error = ServerMessage::error(format!("Invalid JSON: {}", e));
						send_message(&mut ws_sender, &error).await?;
						continue;
					}
				};

				// Validate message
				if let Err(e) = client_msg.validate() {
					log_error!("Message validation failed from {}: {}", peer_addr, e);
					let error = ServerMessage::error(e);
					send_message(&mut ws_sender, &error).await?;
					continue;
				}

				// Process the message
				if let Err(e) =
					process_client_message(client_msg, &mut ws_sender, &config, &role, &sessions)
						.await
				{
					log_error!("Message processing failed for {}: {}", peer_addr, e);
					let error = ServerMessage::error(format!("Internal error: {}", e));
					send_message(&mut ws_sender, &error).await?;
				}
			}
			Ok(Message::Close(_)) => {
				log_info!("Client {} closed connection", peer_addr);
				break;
			}
			Ok(Message::Ping(data)) => {
				log_debug!("Ping received from {}", peer_addr);
				// Respond to ping with pong
				if let Err(e) = ws_sender.send(Message::Pong(data)).await {
					log_error!("Failed to send pong to {}: {}", peer_addr, e);
					break;
				}
			}
			Ok(_) => {
				// Ignore other message types (binary, pong, etc.)
			}
			Err(e) => {
				log_error!("WebSocket protocol error from {}: {}", peer_addr, e);
				break;
			}
		}
	}

	log_info!("Connection closed: {}", peer_addr);
	Ok(())
}

/// Process a client message and send responses
async fn process_client_message(
	client_msg: ClientMessage,
	ws_sender: &mut futures_util::stream::SplitSink<
		WebSocketStream<TcpStream>,
		tokio_tungstenite::tungstenite::Message,
	>,
	config: &Config,
	role: &str,
	sessions: &Arc<Mutex<HashMap<String, ChatSession>>>,
) -> Result<()> {
	match client_msg {
		ClientMessage::Session(msg) => {
			handle_session_message(msg, ws_sender, config, role, sessions).await
		}
		ClientMessage::Message(msg) => {
			handle_user_message(msg, ws_sender, config, role, sessions).await
		}
		ClientMessage::Command(msg) => {
			handle_command_message(msg, ws_sender, config, role, sessions).await
		}
	}
}

/// Handle a "session" type message: create new or resume existing session.
/// No AI call is made — just session setup. Responds with session_id.
async fn handle_session_message(
	msg: SessionMessage,
	ws_sender: &mut futures_util::stream::SplitSink<
		WebSocketStream<TcpStream>,
		tokio_tungstenite::tungstenite::Message,
	>,
	config: &Config,
	role: &str,
	sessions: &Arc<Mutex<HashMap<String, ChatSession>>>,
) -> Result<()> {
	log_debug!("Handling session message: session_id={:?}", msg.session_id);

	let (mut chat_session, config_for_role, session_role, is_new) = match &msg.session_id {
		Some(session_id) => {
			// session_id present: create-or-resume
			// Check memory first
			let existing = sessions.lock().await.remove(session_id);
			if let Some(session) = existing {
				log_debug!("Resumed session from memory: {}", session_id);
				let cfg = config.get_merged_config_for_role(role);
				let role_name = role.to_string();
				(session, cfg, role_name, false)
			} else {
				// Try disk: resume if exists, create with this name if not
				let args = if crate::session::get_sessions_dir()
					.map(|d| d.join(format!("{}.jsonl", session_id)).exists())
					.unwrap_or(false)
				{
					log_debug!("Resuming session from disk: {}", session_id);
					GenericSessionArgs {
						resume: Some(session_id.clone()),
						role: role.to_string(),
						mode: "websocket".into(),
						..Default::default()
					}
				} else {
					log_debug!("Creating named session: {}", session_id);
					GenericSessionArgs {
						name: Some(session_id.clone()),
						role: role.to_string(),
						mode: "websocket".into(),
						..Default::default()
					}
				};

				match setup_and_initialize_session(&args, config).await {
					Ok((session, cfg, role_name, _)) => {
						let is_new = !session.was_resumed;
						(session, cfg, role_name, is_new)
					}
					Err(e) => {
						let error =
							ServerMessage::error(format!("Failed to initialize session: {}", e));
						send_message(ws_sender, &error).await?;
						return Ok(());
					}
				}
			}
		}
		None => {
			// No session_id: create new auto-named session
			log_debug!("Creating new auto-named session with role: {}", role);
			let args = GenericSessionArgs {
				role: role.to_string(),
				mode: "websocket".into(),
				..Default::default()
			};
			match setup_and_initialize_session(&args, config).await {
				Ok((session, cfg, role_name, _)) => (session, cfg, role_name, true),
				Err(e) => {
					let error = ServerMessage::error(format!("Failed to create session: {}", e));
					send_message(ws_sender, &error).await?;
					return Ok(());
				}
			}
		}
	};

	setup_system_prompt_and_cache(&mut chat_session, &config_for_role, &session_role, false)
		.await?;

	let session_id = chat_session.session.info.name.clone();
	let status_msg = if is_new {
		format!("Session created: {}", session_id)
	} else {
		format!("Session resumed: {}", session_id)
	};

	log_info!("{}", status_msg);

	chat_session.save()?;
	sessions
		.lock()
		.await
		.insert(session_id.clone(), chat_session);

	send_message(
		ws_sender,
		&ServerMessage::status(status_msg, Some(session_id)),
	)
	.await?;
	Ok(())
}

/// Look up an existing session: memory first, then disk. Never auto-create.
/// Returns the session or a ServerMessage error suitable for sending to the client.
async fn lookup_session(
	session_id: &str,
	sessions: &Arc<Mutex<HashMap<String, ChatSession>>>,
	config: &Config,
	role: &str,
) -> std::result::Result<ChatSession, ServerMessage> {
	let existing = sessions.lock().await.remove(session_id);
	if let Some(session) = existing {
		log_debug!("Resumed session from memory: {}", session_id);
		return Ok(session);
	}

	log_debug!("Loading session from disk: {}", session_id);
	let mut args = GenericSessionArgs::resume(session_id.to_string(), role.to_string());
	args.mode = "websocket".to_string();
	match setup_and_initialize_session(&args, config).await {
		Ok((mut session, config_for_role, session_role, _)) => {
			if let Err(e) =
				setup_system_prompt_and_cache(&mut session, &config_for_role, &session_role, false)
					.await
			{
				return Err(ServerMessage::error(format!(
					"Failed to setup session {}: {}",
					session_id, e
				)));
			}
			log_info!("Session loaded from disk: {}", session_id);
			Ok(session)
		}
		Err(_) => Err(ServerMessage::error(format!(
			"Session not found: {}. Send a \"session\" message first to create or resume a session.",
			session_id
		))),
	}
}

/// Handle a "command" type message: execute a session command (e.g. /info, /model, /mcp list).
/// Requires session_id and command. Args are optional.
/// The command is assembled into the CLI slash-command format and routed through process_command.
async fn handle_command_message(
	msg: CommandMessage,
	ws_sender: &mut futures_util::stream::SplitSink<
		WebSocketStream<TcpStream>,
		tokio_tungstenite::tungstenite::Message,
	>,
	config: &Config,
	role: &str,
	sessions: &Arc<Mutex<HashMap<String, ChatSession>>>,
) -> Result<()> {
	let session_id = msg.session_id.as_str();
	let command_name = msg.command.trim();
	let args = msg.args.as_slice();

	// Build the slash-command string exactly as the CLI would receive it
	let slash_command = if args.is_empty() {
		format!("/{}", command_name)
	} else {
		format!("/{} {}", command_name, args.join(" "))
	};

	log_debug!(
		"Handling command message: session_id={}, command={}",
		session_id,
		slash_command
	);

	let mut chat_session = match lookup_session(session_id, sessions, config, role).await {
		Ok(s) => s,
		Err(error) => {
			send_message(ws_sender, &error).await?;
			return Ok(());
		}
	};

	let session_id = session_id.to_string();
	let config_for_role = config.get_merged_config_for_role(role);
	let mut cancellation = SessionCancellation::new();
	let operation_rx = cancellation.new_operation();

	// Handle /done specially — it hits unreachable!() in process_command
	// because the CLI intercepts it before routing. We handle it here directly.
	if command_name == "done" {
		use crate::session::chat::session::commands::handle_done;
		match handle_done(&mut chat_session, &config_for_role, operation_rx).await {
			Ok(_) => {
				let status = ServerMessage::status(
					"Conversation compressed".to_string(),
					Some(session_id.clone()),
				);
				send_message(ws_sender, &status).await?;
			}
			Err(e) => {
				let error = ServerMessage::error(format!("Compression failed: {}", e));
				send_message(ws_sender, &error).await?;
			}
		}
		chat_session.save()?;
		sessions.lock().await.insert(session_id, chat_session);
		return Ok(());
	}

	use crate::session::chat::session::commands::CommandResult;
	let command_result = chat_session
		.process_command(
			&slash_command,
			&mut config_for_role.clone(),
			role,
			operation_rx,
		)
		.await?;

	match command_result {
		CommandResult::Handled => {
			log_debug!("Command '{}' executed successfully", slash_command);
			let status = ServerMessage::status(
				format!("Command '{}' executed successfully", slash_command),
				Some(session_id.clone()),
			);
			send_message(ws_sender, &status).await?;
		}
		CommandResult::HandledWithOutput(command_output) => {
			log_debug!(
				"Command '{}' executed with structured output",
				slash_command
			);
			let response = ServerMessage::Status(StatusPayload {
				message: format!("Command '{}' executed successfully", slash_command),
				session_id: Some(session_id.clone()),
				data: Some(command_output.to_json()),
			});
			send_message(ws_sender, &response).await?;
		}
		CommandResult::Exit => {
			log_info!("Session ended by command '{}'", slash_command);
			let status =
				ServerMessage::status("Session ended".to_string(), Some(session_id.clone()));
			send_message(ws_sender, &status).await?;
			// Don't store session back — it's ended
			return Ok(());
		}
		CommandResult::TreatAsUserInput => {
			// Command not recognised — return error rather than silently treating as AI input
			let error = ServerMessage::error(format!(
				"Unknown command: '{}'. Use type \"message\" to send user input.",
				slash_command
			));
			send_message(ws_sender, &error).await?;
		}
	}

	chat_session.save()?;
	sessions.lock().await.insert(session_id, chat_session);
	Ok(())
}

/// Handle a "message" type message: send content to an existing session and get AI response.
/// session_id must refer to an already-established session (from a prior "session" message).
async fn handle_user_message(
	msg: UserMessage,
	ws_sender: &mut futures_util::stream::SplitSink<
		WebSocketStream<TcpStream>,
		tokio_tungstenite::tungstenite::Message,
	>,
	config: &Config,
	role: &str,
	sessions: &Arc<Mutex<HashMap<String, ChatSession>>>,
) -> Result<()> {
	let session_id = msg.session_id.as_str();
	let input = msg.content.clone();

	log_debug!(
		"Handling user message: session_id={}, content_len={}",
		session_id,
		input.len()
	);

	let mut chat_session = match lookup_session(session_id, sessions, config, role).await {
		Ok(s) => s,
		Err(error) => {
			send_message(ws_sender, &error).await?;
			return Ok(());
		}
	};

	let session_id = session_id.to_string();

	// Get current directory for file operations
	let current_dir = crate::mcp::get_thread_working_directory();
	let config_for_role = config.get_merged_config_for_role(role);
	let mut cancellation = SessionCancellation::new();
	let operation_rx = cancellation.new_operation();

	// Drain any completed async jobs before processing user input.
	// Each completed job is injected as a user message so the AI sees it.
	if let Ok(job) = chat_session.job_rx.try_recv() {
		let job_msg = if job.output.starts_with("ERROR: ") {
			format!(
				"[Async agent '{}' failed]\n\n{}",
				job.agent_name,
				job.output.trim_start_matches("ERROR: ")
			)
		} else {
			format!(
				"[Async agent '{}' completed]\n\n{}",
				job.agent_name, job.output
			)
		};
		chat_session.add_user_message(&job_msg)?;
	}

	// Process through layers if enabled (first message)
	let first_message_processed = !chat_session.session.messages.is_empty();
	log_debug!(
		"Processing input through layers: first_message={}",
		!first_message_processed
	);

	let (processed_input, layers_modified_session, _layer_cancelled) = process_layers_if_enabled(
		&input,
		&mut chat_session,
		&config_for_role,
		role,
		first_message_processed,
		operation_rx.clone(),
	)
	.await?;

	// Set first_prompt_idx BEFORE compression so the anchor is always correct.
	// Compression uses first_prompt_idx as the lower boundary.
	if !layers_modified_session && chat_session.first_prompt_idx.is_none() {
		chat_session.first_prompt_idx = Some(chat_session.session.messages.len());
	}

	// Conversation compression: check if AI should compress older exchanges.
	// Runs BEFORE user message is added to avoid breaking the new request.
	let _compression_occurred =
		match crate::session::chat::conversation_compression::check_and_compress_conversation(
			&mut chat_session,
			&config_for_role,
			operation_rx.clone(),
			false,
		)
		.await
		{
			Ok(compressed) => compressed,
			Err(e) => {
				log_debug!(
					"Conversation compression failed: {}. Continuing session.",
					e
				);
				false
			}
		};

	// Add user message if layers didn't modify session
	if !layers_modified_session {
		let final_input_with_constraints =
			crate::session::chat::session::utils::append_constraints_if_exists(
				&processed_input,
				&config_for_role.custom_constraints_file_name,
				&current_dir,
			);
		chat_session.add_user_message(&final_input_with_constraints)?;
	}

	// Prepare for API call
	prepare_for_api_call(&mut chat_session, &config_for_role, operation_rx.clone()).await?;

	// Create channel for WebSocket sink to stream messages
	let (ws_tx, mut ws_rx) = tokio::sync::mpsc::unbounded_channel();
	let ws_sink = WebSocketSink::new(ws_tx.clone());

	// Forward MCP server notifications through the WebSocket channel
	crate::mcp::process::set_notification_sender(Some(session_id.clone()), ws_tx);

	// Execute API call — events stream in real-time via WebSocketSink
	let api_result = execute_api_call_and_process_response(
		&mut chat_session,
		&config_for_role,
		role,
		operation_rx.clone(),
		OutputMode::WebSocket,
		ws_sink,
	)
	.await;

	// Drain any remaining queued stream messages after API completion
	while let Ok(msg) = ws_rx.try_recv() {
		send_message(ws_sender, &msg).await?;
	}

	match api_result {
		Ok(_) => {
			// Cost message (events already emitted via sink — no reconstruction needed)
			let total_tokens = chat_session.session.info.input_tokens
				+ chat_session.session.info.output_tokens
				+ chat_session.session.info.cache_read_tokens
				+ chat_session.session.info.cache_write_tokens
				+ chat_session.session.info.reasoning_tokens;
			let cost_msg = ServerMessage::Cost(CostPayload {
				session_tokens: total_tokens,
				session_cost: chat_session.session.info.total_cost,
				input_tokens: chat_session.session.info.input_tokens,
				output_tokens: chat_session.session.info.output_tokens,
				cache_read_tokens: chat_session.session.info.cache_read_tokens,
				cache_write_tokens: chat_session.session.info.cache_write_tokens,
				reasoning_tokens: chat_session.session.info.reasoning_tokens,
				session_id: session_id.clone(),
			});
			send_message(ws_sender, &cost_msg).await?;
		}
		Err(e) => {
			log_error!("API call failed: {}", e);
			let error = ServerMessage::error(format!("Error: {}", e));
			send_message(ws_sender, &error).await?;
		}
	}

	// Process pending scheduled entries (same as non-interactive keep-alive in main_loop)
	while crate::mcp::core::has_pending_schedules() {
		log_debug!("WebSocket: waiting for scheduled entries...");
		crate::mcp::core::next_schedule_sleep().await;

		while let Some(entry) = crate::mcp::core::pop_due_entry() {
			log_debug!(
				"Schedule entry [{}] fired (websocket): {}",
				entry.id,
				entry.description
			);

			chat_session.add_user_message(&entry.message)?;
			let sched_operation_rx = cancellation.new_operation();
			prepare_for_api_call(
				&mut chat_session,
				&config_for_role,
				sched_operation_rx.clone(),
			)
			.await?;

			let (sched_tx, mut sched_rx) = tokio::sync::mpsc::unbounded_channel();
			let sched_sink = WebSocketSink::new(sched_tx);

			let sched_result = execute_api_call_and_process_response(
				&mut chat_session,
				&config_for_role,
				role,
				sched_operation_rx,
				OutputMode::WebSocket,
				sched_sink,
			)
			.await;

			while let Ok(msg) = sched_rx.try_recv() {
				send_message(ws_sender, &msg).await?;
			}

			if let Err(e) = sched_result {
				log_debug!("Error processing scheduled entry [{}]: {}", entry.id, e);
			}
		}
	}

	// Save session
	log_debug!("Saving session: {}", session_id);
	chat_session.save()?;

	// Clear the notification sender now that this request is done
	crate::mcp::process::clear_notification_sender(Some(session_id.clone()));

	// Store session back
	sessions
		.lock()
		.await
		.insert(session_id.clone(), chat_session);
	log_debug!("Session stored back in memory: {}", session_id);

	Ok(())
}

/// Send a server message through WebSocket
async fn send_message(
	ws_sender: &mut futures_util::stream::SplitSink<
		WebSocketStream<TcpStream>,
		tokio_tungstenite::tungstenite::Message,
	>,
	msg: &ServerMessage,
) -> Result<()> {
	let json = serde_json::to_string(msg)?;
	log_debug!(
		"Sending message: type={:?}, size={} bytes",
		std::mem::discriminant(msg),
		json.len()
	);
	ws_sender.send(Message::text(json)).await?;
	Ok(())
}
