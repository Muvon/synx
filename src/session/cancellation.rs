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

//! Cancellation management for Octomind sessions
//!
//! This module provides a robust, zero-polling cancellation system using
//! tokio's watch channel for proper async cancellation semantics.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::signal;
use tokio::sync::watch;

/// Manages cancellation state for a session with proper signal handling
pub struct SessionCancellation {
	/// Sender for cancellation events
	cancel_tx: watch::Sender<bool>,
	/// Receiver for cancellation events
	cancel_rx: watch::Receiver<bool>,
	/// Tracks if we've seen the first Ctrl+C
	first_interrupt: Arc<AtomicBool>,
}

impl Default for SessionCancellation {
	fn default() -> Self {
		Self::new()
	}
}

impl SessionCancellation {
	/// Create a new cancellation manager
	pub fn new() -> Self {
		let (cancel_tx, cancel_rx) = watch::channel(false);

		Self {
			cancel_tx,
			cancel_rx,
			first_interrupt: Arc::new(AtomicBool::new(false)),
		}
	}

	/// Get current operation receiver
	pub fn operation_receiver(&self) -> watch::Receiver<bool> {
		self.cancel_rx.clone()
	}

	/// Start signal handling for this session with error recovery
	pub fn start_signal_handler(&self) -> tokio::task::JoinHandle<()> {
		let cancel_tx = self.cancel_tx.clone();
		let first_interrupt = self.first_interrupt.clone();

		tokio::spawn(async move {
			// Set up signal handlers with error handling
			#[cfg(unix)]
			{
				use signal::unix::{signal, SignalKind};

				let sigint = match signal(SignalKind::interrupt()) {
					Ok(sig) => sig,
					Err(e) => {
						eprintln!("Warning: Failed to register SIGINT handler: {}", e);
						return;
					}
				};

				let sigterm = match signal(SignalKind::terminate()) {
					Ok(sig) => sig,
					Err(e) => {
						eprintln!("Warning: Failed to register SIGTERM handler: {}", e);
						return;
					}
				};

				let mut sigint = sigint;
				let mut sigterm = sigterm;

				tokio::select! {
					_ = async {
						loop {
							sigint.recv().await;
							if !handle_interrupt(&first_interrupt, &cancel_tx) {
								break;
							}
						}
					} => {},
					_ = sigterm.recv() => {
						println!("\n🛑 Termination signal received - exiting...");
						std::io::Write::flush(&mut std::io::stdout()).unwrap_or(());
						std::process::exit(130);
					}
				}
			}

			#[cfg(windows)]
			{
				loop {
					match signal::ctrl_c().await {
						Ok(()) => {
							if !handle_interrupt(&first_interrupt, &cancel_tx) {
								break;
							}
						}
						Err(e) => {
							eprintln!("Warning: Failed to listen for Ctrl+C: {}", e);
							break;
						}
					}
				}
			}
		})
	}

	/// Get a new operation receiver
	pub fn new_operation(&mut self) -> watch::Receiver<bool> {
		// Clone and mark current value as seen to avoid spurious wakeups from old changes
		let mut rx = self.cancel_rx.clone();
		rx.mark_unchanged();
		rx
	}

	/// Check if the current operation is cancelled
	pub fn is_cancelled(&self) -> bool {
		*self.cancel_rx.borrow()
	}

	/// Wait for cancellation (async)
	pub async fn cancelled(&mut self) {
		// Wait for the value to become true
		while !*self.cancel_rx.borrow() {
			if self.cancel_rx.changed().await.is_err() {
				break;
			}
		}
	}

	/// Reset the cancellation state
	pub fn reset(&mut self) {
		self.first_interrupt.store(false, Ordering::SeqCst);
		// Reset the watch channel
		let _ = self.cancel_tx.send(false);
	}

	/// Cancel all operations and shutdown
	pub fn shutdown(&self) {
		let _ = self.cancel_tx.send(true);
	}
}

/// Handle interrupt signal with double-Ctrl+C detection
fn handle_interrupt(first_interrupt: &Arc<AtomicBool>, cancel_tx: &watch::Sender<bool>) -> bool {
	if first_interrupt.load(Ordering::SeqCst) {
		// Second Ctrl+C - force exit (always visible)
		// Use std::println! directly to avoid with_suspended_spinner which could deadlock
		// with the animation task's indicatif lock
		std::println!("\n\u{1f6d1} Forcing exit...");
		std::io::Write::flush(&mut std::io::stdout()).unwrap_or(());
		std::process::exit(130);
	} else {
		// First Ctrl+C — send cancellation signal IMMEDIATELY before any IO.
		// log_debug!/println! use with_suspended_spinner which acquires the spinner
		// mutex and then indicatif's internal BarState lock via suspend().  If the
		// steady-tick thread or animation task holds BarState at that moment, the
		// signal handler blocks and cancel_tx.send() never fires — making Ctrl+C
		// appear completely unresponsive.
		first_interrupt.store(true, Ordering::SeqCst);
		let _ = cancel_tx.send(true);

		// Now safe to log — even if this blocks briefly, cancellation is already sent
		crate::log_debug!("Ctrl+C: Interrupting current operation...");
		crate::log_debug!("Press Ctrl+C again to force exit");
		std::io::Write::flush(&mut std::io::stdout()).unwrap_or(());

		true // Continue handling
	}
}
