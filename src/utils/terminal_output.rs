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

//! Terminal output utilities with automatic spinner suspension
//!
//! This module provides the `with_suspended_spinner` function used by the
//! spinner-aware print macros defined in `src/lib.rs`.
//!
//! The macros (`println!`, `print!`, `eprintln!`, `eprint!`) are defined at
//! crate root level to properly shadow `std::println!` etc. throughout the crate.

/// Suspend spinner and execute a closure
///
/// This function checks if the global animation manager exists and suspends
/// the spinner before executing the closure. If no spinner is active, it just
/// executes the closure normally.
#[inline]
pub fn with_suspended_spinner<F, R>(f: F) -> R
where
	F: FnOnce() -> R,
{
	// Try to get the global animation manager
	// If it exists, suspend the spinner before printing
	if let Some(manager) = crate::session::chat::animation_manager::GLOBAL_ANIMATION_MANAGER.get() {
		manager.with_suspended_spinner(f)
	} else {
		// No animation manager initialized yet - just execute normally
		f()
	}
}
