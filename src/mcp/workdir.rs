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

//! Thread-local working directory management for parallel tool execution.
//!
//! Each thread (parallel tool call) gets its own isolated working directory.
//! `session` is the anchor set at session start (used by workdir reset).
//! `current` tracks mid-session changes via the workdir tool.

use std::path::PathBuf;

struct WorkDir {
	session: PathBuf,
	current: PathBuf,
}

thread_local! {
	static WORKDIR: std::cell::RefCell<Option<WorkDir>> = const { std::cell::RefCell::new(None) };
}

/// Set the session working directory. Call at every session boundary.
/// Resets both the active directory and the reset anchor to `path`.
pub fn set_session_working_directory(path: PathBuf) {
	WORKDIR.with(|w| {
		*w.borrow_mut() = Some(WorkDir {
			session: path.clone(),
			current: path,
		});
	});
}

/// Override the active directory mid-session (workdir tool). Does not move the reset anchor.
pub fn set_thread_working_directory(path: PathBuf) {
	WORKDIR.with(|w| {
		let mut w = w.borrow_mut();
		if let Some(ref mut wd) = *w {
			wd.current = path;
		}
	});
}

/// Active working directory for the current thread.
pub fn get_thread_working_directory() -> PathBuf {
	WORKDIR.with(|w| {
		w.borrow()
			.as_ref()
			.map(|wd| wd.current.clone())
			.unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
	})
}

/// Session anchor — the directory to return to on workdir reset.
pub fn get_thread_original_working_directory() -> PathBuf {
	WORKDIR.with(|w| {
		w.borrow()
			.as_ref()
			.map(|wd| wd.session.clone())
			.unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
	})
}
