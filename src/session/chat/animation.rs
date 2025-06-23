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

// Animation module for loading indicators

use anyhow::Result;
use colored::*;
use crossterm::{cursor, execute};
use std::io::{stdout, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// Animation frames for loading indicator
const LOADING_FRAMES: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

// Show loading animation while waiting for response
pub async fn show_loading_animation(cancel_flag: Arc<AtomicBool>, _cost: f64) -> Result<()> {
	let mut stdout = stdout();
	let mut frame_idx = 0;

	// Save cursor position
	execute!(stdout, cursor::SavePosition)?;

	while !cancel_flag.load(Ordering::SeqCst) {
		// Display frame with color if supported
		execute!(stdout, cursor::RestorePosition)?;

		print!(
			" {} {}",
			LOADING_FRAMES[frame_idx].cyan(),
			"Generating response...".bright_blue()
		);

		stdout.flush()?;

		// Update frame index
		frame_idx = (frame_idx + 1) % LOADING_FRAMES.len();

		// Shorter delay to be more responsive to cancellation
		tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
	}

	// Clear loading message completely and print a newline
	execute!(stdout, cursor::RestorePosition)?;
	print!("                                        "); // Clear the entire loading message with spaces
	execute!(stdout, cursor::RestorePosition)?;
	stdout.flush()?;

	Ok(())
}

// Show static pricing line for non-interactive mode
pub async fn show_no_animation(cancel_flag: Arc<AtomicBool>, cost: f64) -> Result<()> {
	// Display static pricing line for non-interactive mode
	if !std::io::stdin().is_terminal() {
		println!(
			" ── cost: ${:.5} ────────────────────────────────────────",
			cost
		);
	}

	// Wait for cancellation without showing any visual animation
	while !cancel_flag.load(Ordering::SeqCst) {
		tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
	}
	Ok(())
}

// Smart animation that automatically detects interactive vs non-interactive mode
pub async fn show_smart_animation(cancel_flag: Arc<AtomicBool>, cost: f64) -> Result<()> {
	if std::io::stdin().is_terminal() {
		// Interactive mode - show full animation
		show_loading_animation(cancel_flag, cost).await
	} else {
		// Non-interactive mode (piped, run command, etc.) - no animation
		show_no_animation(cancel_flag, cost).await
	}
}

// Display generation message for non-interactive mode (without animation)
pub fn show_generation_message_static(cost: f64) {
	if !std::io::stdin().is_terminal() {
		// Non-interactive mode - show static pricing line
		println!(
			" ── cost: ${:.5} ────────────────────────────────────────",
			cost
		);
	}
	// Interactive mode - do nothing (animation will handle it)
}
