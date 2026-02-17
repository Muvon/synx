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

//! Global animation manager - ensures only one animation runs at a time
//!
//! This module provides a centralized animation management system that:
//! - Ensures only one animation runs at a time (prevents overlapping animations)
//! - Dynamically updates cost and context values in real-time
//! - Provides clean cancellation and cleanup
//! - Prevents animation stuck bugs
//! - Responds INSTANTLY to Ctrl+C cancellation (no delays)

use crate::log_debug;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// Shared animation state for dynamic updates
#[derive(Clone)]
pub struct AnimationState {
	/// Current cost (updated dynamically)
	pub cost: Arc<AtomicU64>, // Store as u64 (multiply by 10000 for precision)
	/// Current context tokens (updated dynamically)
	pub context_tokens: Arc<AtomicU64>,
	/// Max threshold for percentage calculation
	pub max_threshold: Arc<AtomicU64>,
}

impl AnimationState {
	pub fn new() -> Self {
		Self {
			cost: Arc::new(AtomicU64::new(0)),
			context_tokens: Arc::new(AtomicU64::new(0)),
			max_threshold: Arc::new(AtomicU64::new(0)),
		}
	}

	/// Update cost (converts f64 to u64 with 4 decimal precision)
	pub fn update_cost(&self, cost: f64) {
		self.cost.store((cost * 10000.0) as u64, Ordering::Relaxed);
	}

	/// Get cost (converts u64 back to f64)
	pub fn get_cost(&self) -> f64 {
		self.cost.load(Ordering::Relaxed) as f64 / 10000.0
	}

	/// Update context tokens
	pub fn update_context_tokens(&self, tokens: u64) {
		self.context_tokens.store(tokens, Ordering::Relaxed);
	}

	/// Get context tokens
	pub fn get_context_tokens(&self) -> u64 {
		self.context_tokens.load(Ordering::Relaxed)
	}

	/// Update max threshold
	pub fn update_max_threshold(&self, threshold: usize) {
		self.max_threshold
			.store(threshold as u64, Ordering::Relaxed);
	}

	/// Get max threshold
	pub fn get_max_threshold(&self) -> usize {
		self.max_threshold.load(Ordering::Relaxed) as usize
	}
}

impl Default for AnimationState {
	fn default() -> Self {
		Self::new()
	}
}

/// Global animation manager - singleton pattern
pub struct AnimationManager {
	/// Current animation task (if any)
	current_task: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
	/// Cancellation flag for current animation
	cancel_flag: Arc<AtomicBool>,
	/// Shared animation state for dynamic updates
	state: AnimationState,
	/// Optional cancellation receiver from session (for instant Ctrl+C response)
	cancel_rx: Arc<std::sync::Mutex<Option<watch::Receiver<bool>>>>,
	/// Suspended flag - prevents animation from starting during user prompts
	suspended: Arc<AtomicBool>,
	/// Shared spinner reference for suspend/resume operations
	spinner: Arc<std::sync::Mutex<Option<indicatif::ProgressBar>>>,
}

impl AnimationManager {
	/// Create new animation manager
	pub fn new() -> Self {
		Self {
			current_task: Arc::new(std::sync::Mutex::new(None)),
			cancel_flag: Arc::new(AtomicBool::new(false)),
			state: AnimationState::new(),
			cancel_rx: Arc::new(std::sync::Mutex::new(None)),
			suspended: Arc::new(AtomicBool::new(false)),
			spinner: Arc::new(std::sync::Mutex::new(None)),
		}
	}

	/// Get shared animation state for external updates
	pub fn get_state(&self) -> AnimationState {
		self.state.clone()
	}

	/// Set cancellation receiver from session (for instant Ctrl+C response)
	/// This allows the animation to respond immediately to Ctrl+C without waiting for stop_current()
	pub fn set_cancel_receiver(&self, rx: watch::Receiver<bool>) {
		*self.cancel_rx.lock().unwrap() = Some(rx);
	}

	/// Clear cancellation receiver (call when animation stops)
	/// Suspend animation - stops current animation and prevents new ones from starting
	/// Use this before displaying user prompts to prevent animation from covering the prompt
	pub async fn suspend(&self) {
		// Set suspended flag FIRST to prevent any race conditions
		self.suspended.store(true, Ordering::SeqCst);
		// Then stop current animation
		self.stop_current().await;
		log_debug!("Animation suspended - user prompt imminent");
	}

	/// Resume animation - allows animation to start again
	/// Call this after user input is complete
	pub fn resume(&self) {
		self.suspended.store(false, Ordering::SeqCst);
		log_debug!("Animation resumed");
	}

	/// Check if animation is suspended
	pub fn is_suspended(&self) -> bool {
		self.suspended.load(Ordering::SeqCst)
	}

	/// Execute a function while temporarily suspending the spinner
	/// This prevents output from interfering with the animation
	/// If no spinner is active, just executes the function normally
	pub fn with_suspended_spinner<F, R>(&self, f: F) -> R
	where
		F: FnOnce() -> R,
	{
		let spinner_guard = self.spinner.lock().unwrap();
		if let Some(ref spinner) = *spinner_guard {
			// Spinner is active - use indicatif's suspend to hide it temporarily
			spinner.suspend(f)
		} else {
			// No spinner active - just execute normally
			drop(spinner_guard);
			f()
		}
	}

	pub fn clear_cancel_receiver(&self) {
		*self.cancel_rx.lock().unwrap() = None;
	}

	/// Stop current animation (if any)
	pub async fn stop_current(&self) {
		// Set cancellation flag - the tokio::select! will detect this instantly
		self.cancel_flag.store(true, Ordering::SeqCst);

		// Clear the cancellation receiver
		self.clear_cancel_receiver();

		// Wait for task to finish gracefully (cleanup will run properly)
		let task = {
			let mut guard = self.current_task.lock().unwrap();
			guard.take()
		};

		if let Some(task) = task {
			// Wait for graceful shutdown - cleanup code will run
			let _ = task.await;
		}

		// Reset cancellation flag for next animation
		self.cancel_flag.store(false, Ordering::SeqCst);
	}
	/// Start new animation (stops any existing animation first)
	///
	/// This ensures only one animation runs at a time, preventing:
	/// - Overlapping animations
	/// - Animation stuck bugs
	/// - Stale cost/context values
	///
	/// **Pro-level feature**: Dynamically reads live cost/context from shared state
	/// during animation loop for real-time updates during long operations.
	/// Start new animation (stops any existing animation first)
	///
	/// This ensures only one animation runs at a time, preventing:
	/// - Overlapping animations
	/// - Animation stuck bugs
	/// - Stale cost/context values
	///
	/// **Pro-level feature**: Dynamically reads live cost/context from shared state
	/// during animation loop for real-time updates during long operations.
	pub async fn start_animation(&self, mode: &crate::session::output::OutputMode) {
		// Check if suspended - don't start animation during user prompts
		if self.is_suspended() {
			log_debug!("Animation start requested but manager is suspended (user prompt active)");
			return;
		}

		// Stop any existing animation first
		self.stop_current().await;

		// Don't show animation in non-interactive modes
		if mode.should_suppress_cli_output() {
			return;
		}

		self.start_internal().await;
	}
	///
	/// Use this for standalone animations where you have specific cost/context values.
	/// Automatically detects interactive vs non-interactive mode.
	pub async fn start_with_params(&self, cost: f64, context_tokens: u64, max_threshold: usize) {
		// Stop any existing animation first
		self.stop_current().await;

		// Don't show animation in non-interactive mode
		if !std::io::stdin().is_terminal() {
			// Show static line for non-interactive mode
			use crate::config::with_thread_config;
			let should_print =
				with_thread_config(|config| config.runtime_output_mode.as_deref() != Some("jsonl"))
					.unwrap_or(true);

			if should_print {
				if cost > 0.0 {
					println!(
						" ── cost: ${:.5} ────────────────────────────────────────",
						cost
					);
				} else if max_threshold > 0 {
					let percentage =
						(context_tokens as f64 / max_threshold as f64 * 100.0).min(100.0);
					println!(
						" ── context: {:.1}% ────────────────────────────────────────",
						percentage
					);
				}
			}
			return;
		}

		// Update state with provided values
		self.state.update_cost(cost);
		self.state.update_context_tokens(context_tokens);
		self.state.update_max_threshold(max_threshold);

		self.start_internal().await;
	}

	/// Internal animation start logic
	async fn start_internal(&self) {
		// Clone references for animation task
		let cancel_flag = self.cancel_flag.clone();
		let current_task = self.current_task.clone();
		let state = self.state.clone();
		let cancel_rx = self.cancel_rx.lock().unwrap().clone();
		let spinner_ref = self.spinner.clone();

		let task = tokio::spawn(async move {
			// Animation loop with truly dynamic cost/context updates
			let mut spinner: Option<indicatif::ProgressBar> = None;
			let start_time = std::time::Instant::now();

			'animation: loop {
				// Check cancellation flags FIRST for instant response
				if cancel_flag.load(Ordering::SeqCst) {
					break 'animation;
				}

				// Check session cancellation receiver if available (INSTANT Ctrl+C response)
				if let Some(ref rx) = cancel_rx {
					if *rx.borrow() {
						break 'animation;
					}
				}

				// Read live cost/context from shared state (dynamic updates!)
				let current_cost = state.get_cost();
				let current_context_tokens = state.get_context_tokens();
				let max_threshold = state.get_max_threshold();

				// Calculate dynamic base message with live cost/context
				let base_message = if current_cost > 0.0 && max_threshold > 0 {
					let percentage =
						(current_context_tokens as f64 / max_threshold as f64 * 100.0).min(100.0);
					format!("[${:.2}|{:.1}%] Working …", current_cost, percentage)
				} else if current_cost > 0.0 {
					format!("[${:.2}|∞] Working …", current_cost)
				} else if max_threshold > 0 {
					// No cost but still show context percentage
					let percentage =
						(current_context_tokens as f64 / max_threshold as f64 * 100.0).min(100.0);
					format!("[{:.1}%] Working …", percentage)
				} else {
					"Working …".to_string()
				};

				// Create spinner on first iteration
				if spinner.is_none() {
					use indicatif::{ProgressBar, ProgressStyle};
					use std::time::Duration;

					let s = ProgressBar::new_spinner();
					s.set_style(
						ProgressStyle::default_spinner()
							.template(" {spinner:.cyan} {msg:.cyan}")
							.unwrap()
							.tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧"),
					);
					s.set_message(base_message.clone());
					s.enable_steady_tick(Duration::from_millis(50));

					// Store spinner reference for suspend operations
					*spinner_ref.lock().unwrap() = Some(s.clone());
					spinner = Some(s);
				}

				// Update message with elapsed time and dynamic cost/context
				if let Some(ref s) = spinner {
					let elapsed = start_time.elapsed();
					let elapsed_secs = elapsed.as_secs();
					let message = if elapsed_secs > 0 {
						use colored::Colorize;
						let time_and_hint = format!(
							"({} • Ctrl+C to interrupt)",
							crate::session::chat::animation::format_elapsed_time(elapsed)
						);
						format!("{} {}", base_message, time_and_hint.dimmed())
					} else {
						use colored::Colorize;
						format!("{} {}", base_message, "(Ctrl+C to interrupt)".dimmed())
					};
					s.set_message(message);
				}

				// CRITICAL FIX: Use tokio::select! for INSTANT cancellation response
				// This allows the animation to break immediately on Ctrl+C instead of waiting up to 100ms
				tokio::select! {
					// Sleep for animation update interval
					_ = tokio::time::sleep(Duration::from_millis(100)) => {
						// Normal sleep completed, continue loop
					}
					// INSTANT cancellation from session's watch channel
					_ = async {
						if let Some(ref rx) = cancel_rx {
							let mut rx_clone = rx.clone();
							// Wait for cancellation signal
							while !*rx_clone.borrow() {
								if rx_clone.changed().await.is_err() {
									break;
								}
							}
						} else {
							// No receiver - wait forever
							std::future::pending::<()>().await;
						}
					} => {
						// Cancellation received - break immediately
						log_debug!("Animation cancelled via session cancellation channel");
						break 'animation;
					}
					// INSTANT cancellation from stop_current() call
					_ = async {
						while !cancel_flag.load(Ordering::SeqCst) {
							tokio::task::yield_now().await;

						}
					} => {
						// stop_current() was called - break immediately
						log_debug!("Animation cancelled via stop_current()");
						break 'animation;
					}
				}
			}

			// Clean up spinner when done
			// Clear shared spinner reference first
			*spinner_ref.lock().unwrap() = None;

			if let Some(s) = spinner {
				// CRITICAL: Disable steady tick first to stop background drawing thread
				// This prevents race condition where tick thread draws after finish_and_clear
				s.disable_steady_tick();
				// Small yield to ensure background thread stops
				tokio::task::yield_now().await;
				s.finish_and_clear();
			}
		});

		// Store task reference
		*current_task.lock().unwrap() = Some(task);
	}

	/// Check if animation is currently running
	pub fn is_running(&self) -> bool {
		self.current_task.lock().unwrap().is_some()
	}
}

impl Default for AnimationManager {
	fn default() -> Self {
		Self::new()
	}
}

/// Global animation manager instance
/// Made public so terminal_output module can access it for spinner suspension
pub static GLOBAL_ANIMATION_MANAGER: std::sync::OnceLock<AnimationManager> =
	std::sync::OnceLock::new();

/// Get global animation manager instance
pub fn get_animation_manager() -> &'static AnimationManager {
	GLOBAL_ANIMATION_MANAGER.get_or_init(AnimationManager::new)
}
