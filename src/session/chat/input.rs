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

// Custom event handler for smart Ctrl+E behavior
struct SmartCtrlEHandler;

impl ConditionalEventHandler for SmartCtrlEHandler {
	fn handle(
		&self,
		_evt: &Event,
		_n: RepeatCount,
		_positive: bool,
		ctx: &rustyline::EventContext,
	) -> Option<Cmd> {
		// Check if there's a hint available using the EventContext
		if ctx.has_hint() {
			// There's a hint, so complete it
			Some(Cmd::CompleteHint)
		} else {
			// No hint, use default Emacs behavior (move to end of line)
			// Return None to let the default key binding take effect
			None
		}
	}
}

// Custom handler for Ctrl+G - sets flag and accepts the line
struct CtrlGHandler {
	add_without_sending: Arc<AtomicBool>,
}

impl ConditionalEventHandler for CtrlGHandler {
	fn handle(
		&self,
		_evt: &Event,
		_n: RepeatCount,
		_positive: bool,
		_ctx: &rustyline::EventContext,
	) -> Option<Cmd> {
		// Set the flag to indicate this should be added without sending
		self.add_without_sending.store(true, Ordering::SeqCst);
		// Accept the line
		Some(Cmd::AcceptLine)
	}
}

use crate::log_info;

// Read user input with support for multiline input, command completion, and persistent history
pub fn read_user_input(
	estimated_cost: f64,
	octomind_config: &crate::config::Config,
	role: &str,
) -> Result<InputResult> {
	// Flag to track if Ctrl+G was pressed
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

	// Add command completion
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

	// Ctrl+G to submit without sending to API
	let add_without_sending_clone = add_without_sending.clone();
	editor.bind_sequence(
		Event::KeySeq(vec![KeyEvent::new('g', Modifiers::CTRL)]),
		EventHandler::Conditional(Box::new(CtrlGHandler {
			add_without_sending: add_without_sending_clone,
		})),
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

	// Set prompt with colors if terminal supports them and include cost estimation
	let prompt = if estimated_cost > 0.0 {
		format!("[~${:.2}] > ", estimated_cost)
			.bright_blue()
			.to_string()
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
			// Ctrl+D - Show session file path before exiting
			println!("\nExiting session...");

			// Show session file path if available
			if let Ok(sessions_dir) = crate::session::get_sessions_dir() {
				println!("Session files saved in: {}", sessions_dir.display());
			}

			log_info!("Session preserved for future reference.");
			Ok(InputResult::Exit)
		}
		Err(err) => {
			println!("Error: {:?}", err);
			Ok(InputResult::Text(String::new()))
		}
	}
}
