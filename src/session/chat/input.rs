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

// User input handling module

use crate::config::Config;
use crate::mcp::get_available_functions;
use crate::session::estimate_full_context_tokens;
use crate::session::history::{append_to_session_history_file, load_session_history_from_file};
use anyhow::Result;
use colored::*;
use rustyline::error::ReadlineError;
use rustyline::{
	Cmd, ConditionalEventHandler, Event, EventHandler, KeyEvent, Modifiers, RepeatCount,
};
use rustyline::{CompletionType, Config as RustylineConfig, EditMode, Editor};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Result of user input operation
#[derive(Debug)]
pub enum InputResult {
	/// Normal text input from user
	Text(String),
	/// Input was cancelled (Ctrl+C)
	Cancelled,
	/// User wants to exit (Ctrl+D)
	Exit,
	/// Add message to context without sending (Ctrl+G)
	AddWithoutSending(String),
}

struct SmartCtrlEHandler;

impl ConditionalEventHandler for SmartCtrlEHandler {
	fn handle(
		&self,
		_evt: &Event,
		_n: RepeatCount,
		_positive: bool,
		ctx: &rustyline::EventContext,
	) -> Option<Cmd> {
		if ctx.has_hint() {
			Some(Cmd::CompleteHint)
		} else {
			None // Default Emacs behavior (move to end of line)
		}
	}
}

struct CtrlGHandler {
	flag: Arc<AtomicBool>,
}

impl ConditionalEventHandler for CtrlGHandler {
	fn handle(
		&self,
		_evt: &Event,
		_n: RepeatCount,
		_positive: bool,
		_ctx: &rustyline::EventContext,
	) -> Option<Cmd> {
		self.flag.store(true, Ordering::SeqCst);
		Some(Cmd::AcceptLine)
	}
}

struct ShowHelpHandler;

impl ConditionalEventHandler for ShowHelpHandler {
	fn handle(
		&self,
		_evt: &Event,
		_n: RepeatCount,
		_positive: bool,
		ctx: &rustyline::EventContext,
	) -> Option<Cmd> {
		if ctx.line().is_empty() {
			use std::io::{self, Write};

			// Clear current line
			print!("\r\x1B[K");
			let _ = io::stdout().flush();

			// Print help
			display_shortcuts_help();

			// Manually print prompt to make it visible again
			print!("> ");
			let _ = io::stdout().flush();

			Some(Cmd::Noop)
		} else {
			None
		}
	}
}

use crate::log_info;

/// Display the context/cost status line - shown only at session start
pub fn display_status_line(current_context_tokens: u64, max_session_tokens_threshold: usize) {
	let mut status_parts = Vec::new();

	if max_session_tokens_threshold > 0 {
		let percentage = (current_context_tokens as f64 / max_session_tokens_threshold as f64
			* 100.0)
			.min(100.0);
		status_parts.push(format!("Context: {:.1}%", percentage));
	} else {
		status_parts.push("Context: unlimited".to_string());
	}

	status_parts.push("? for shortcuts".to_string());
	status_parts.push("/help for commands".to_string());
	println!("{}", status_parts.join(" • ").bright_black());
}

fn calculate_context_percentage(
	current_context_tokens: u64,
	max_session_tokens_threshold: usize,
) -> Option<f64> {
	if max_session_tokens_threshold > 0 {
		Some(
			(current_context_tokens as f64 / max_session_tokens_threshold as f64 * 100.0)
				.min(100.0),
		)
	} else {
		None
	}
}

fn display_shortcuts_help() {
	use std::io::{self, Write};

	println!();
	println!(
		"{}",
		"╭─ Keyboard Shortcuts ─────────────────────────────────────╮".bright_cyan()
	);
	println!(
		"{}",
		"│ /           - Commands (type /help for list)            │".bright_black()
	);
	println!(
		"{}",
		"│ Tab         - Complete command/file                     │".bright_black()
	);
	println!(
		"{}",
		"│ Shift+Tab   - Search history                            │".bright_black()
	);
	println!(
		"{}",
		"│ Ctrl+J      - Insert newline (multi-line input)         │".bright_black()
	);
	println!(
		"{}",
		"│ Ctrl+G      - Add message without sending to API        │".bright_black()
	);
	println!(
		"{}",
		"│ Ctrl+E      - Accept hint (when available)              │".bright_black()
	);
	println!(
		"{}",
		"│ Ctrl+R      - Search command history                    │".bright_black()
	);
	println!(
		"{}",
		"│ Ctrl+C      - Cancel current operation                  │".bright_black()
	);
	println!(
		"{}",
		"│ Ctrl+D      - Exit session                              │".bright_black()
	);
	println!(
		"{}",
		"│ Ctrl+P/N    - Navigate command history                  │".bright_black()
	);
	println!(
		"{}",
		"│ →           - Accept hint (when at end of line)         │".bright_black()
	);
	println!(
		"{}",
		"╰──────────────────────────────────────────────────────────╯".bright_cyan()
	);
	println!();

	// Force flush to ensure everything is displayed
	let _ = io::stdout().flush();
}

/// Calculate current context tokens for the session
/// This uses actual message count + system prompt + tools, NOT lifetime accumulated tokens
pub async fn calculate_current_context_tokens(
	messages: &[crate::session::Message],
	config: &Config,
	role: &str,
) -> u64 {
	// Get system prompt for the role
	let (_, _, _, _, system_prompt) = config.get_role_config(role);

	// Get available tools
	let tools = get_available_functions(config).await;

	// Calculate actual context tokens
	estimate_full_context_tokens(messages, Some(system_prompt), Some(&tools)) as u64
}

// Read user input with support for multiline input, command completion, and persistent history
// show_status_line controls whether to display the context/cost status line (only on first interaction)
pub fn read_user_input(
	estimated_cost: f64,
	octomind_config: &Config,
	role: &str,
	current_context_tokens: u64,
	max_session_tokens_threshold: usize,
	show_status_line: bool,
) -> Result<InputResult> {
	let add_without_sending = Arc::new(AtomicBool::new(false));

	// Configure rustyline with proper completion behavior for file completion
	let config = RustylineConfig::builder()
		.completion_type(CompletionType::List) // Bash-like completion with partial matches
		.edit_mode(EditMode::Emacs)
		.auto_add_history(false) // Manual history control for whitespace-prefixed inputs
		.bell_style(rustyline::config::BellStyle::None) // No bell
		.max_history_size(1000)? // Limit history size
		.color_mode(rustyline::ColorMode::Enabled) // Enable proper ANSI color handling
		.build();

	// Create editor with our custom helper
	let mut editor = Editor::with_config(config)?;

	use crate::session::chat_helper::CommandHelper;
	editor.set_helper(Some(CommandHelper::new(octomind_config, role)));

	// Set up custom key bindings
	// Ctrl+E: Smart behavior - ONLY accepts hints when available,
	// otherwise falls back to default Emacs behavior (move to end of line)
	editor.bind_sequence(
		Event::KeySeq(vec![KeyEvent::new('e', Modifiers::CTRL)]),
		EventHandler::Conditional(Box::new(SmartCtrlEHandler)),
	);

	// Tab for completion - use Complete for proper file completion behavior
	editor.bind_sequence(
		Event::KeySeq(vec![KeyEvent::new('\t', Modifiers::empty())]),
		EventHandler::Simple(Cmd::Complete),
	);

	// Shift+Tab for reverse completion (bash-like)
	editor.bind_sequence(
		Event::KeySeq(vec![KeyEvent::new('\t', Modifiers::SHIFT)]),
		EventHandler::Simple(Cmd::ReverseSearchHistory),
	);

	// Right arrow to accept hint when at end of line
	editor.bind_sequence(
		Event::KeySeq(vec![
			KeyEvent::new('\x1b', Modifiers::empty()),
			KeyEvent::new('[', Modifiers::empty()),
			KeyEvent::new('C', Modifiers::empty()),
		]),
		EventHandler::Simple(Cmd::CompleteHint),
	);

	// Right arrow to accept hint when at end of line
	// Using escape sequence for right arrow key: \x1b[C
	editor.bind_sequence(
		Event::KeySeq(vec![
			KeyEvent::new('\x1b', Modifiers::empty()),
			KeyEvent::new('[', Modifiers::empty()),
			KeyEvent::new('C', Modifiers::empty()),
		]),
		EventHandler::Simple(Cmd::CompleteHint),
	);

	// Ctrl+J to insert newline for multi-line input
	editor.bind_sequence(
		Event::KeySeq(vec![KeyEvent::new('j', Modifiers::CTRL)]),
		EventHandler::Simple(Cmd::Newline),
	);

	editor.bind_sequence(
		Event::KeySeq(vec![KeyEvent::new('g', Modifiers::CTRL)]),
		EventHandler::Conditional(Box::new(CtrlGHandler {
			flag: add_without_sending.clone(),
		})),
	);

	editor.bind_sequence(
		Event::KeySeq(vec![KeyEvent::new('?', Modifiers::empty())]),
		EventHandler::Conditional(Box::new(ShowHelpHandler)),
	);

	// Load persistent history using role-based system
	match load_session_history_from_file(role) {
		Ok(history_lines) => {
			for line in history_lines {
				let _ = editor.add_history_entry(line);
			}
		}
		Err(e) => {
			log_info!("Could not load history for role '{}': {}", role, e);
		}
	}

	// Display status line for user feedback (only on first interaction)
	if show_status_line {
		display_status_line(current_context_tokens, max_session_tokens_threshold);
	}

	// Set prompt with cost and context percentage
	let prompt = if estimated_cost > 0.0 {
		let context_pct =
			calculate_context_percentage(current_context_tokens, max_session_tokens_threshold);

		if let Some(pct) = context_pct {
			format!("[${:.2}|{:.1}%] > ", estimated_cost, pct)
				.bright_blue()
				.to_string()
		} else {
			format!("[${:.2}|∞] > ", estimated_cost)
				.bright_blue()
				.to_string()
		}
	} else {
		"> ".bright_blue().to_string()
	};

	// Read line with command completion and history search (Ctrl+R)
	match editor.readline(&prompt) {
		Ok(line) => {
			// Check if Ctrl+G was pressed
			let is_add_without_sending = add_without_sending.load(Ordering::SeqCst);

			// Check if line starts with whitespace (bash-like behavior)
			// If it does, skip adding to history (both in-memory and persistent)
			let starts_with_whitespace = line.starts_with(char::is_whitespace);

			if !starts_with_whitespace {
				// Add to in-memory history only if not starting with whitespace
				let _ = editor.add_history_entry(line.clone());

				// Append to persistent file using role-based thread-safe method
				// This includes ALL inputs - both regular inputs and commands starting with '/'
				if let Err(e) = append_to_session_history_file(role, &line) {
					// Don't fail if history can't be saved, just log it
					log_info!(
						"Could not append to history file for role '{}': {}",
						role,
						e
					);
				}
			}

			// Log user input only if it's not a command (doesn't start with '/')
			// Note: We still log even if it starts with whitespace, as logging is separate from history
			if !line.trim().starts_with('/') {
				let _ = crate::session::logger::log_user_request(&line);
			}

			if is_add_without_sending {
				Ok(InputResult::AddWithoutSending(line))
			} else {
				Ok(InputResult::Text(line))
			}
		}
		Err(ReadlineError::Interrupted) => {
			// Ctrl+C - Return cancellation result
			Ok(InputResult::Cancelled)
		}
		Err(ReadlineError::Eof) => {
			// Ctrl+D - Show resume command
			println!("\n{}: /exit", "Type".bright_yellow());
			Ok(InputResult::Exit)
		}
		Err(err) => {
			println!("Error: {:?}", err);
			Ok(InputResult::Text(String::new()))
		}
	}
}
