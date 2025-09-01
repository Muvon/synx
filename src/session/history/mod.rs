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

// Role-based history management system

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Mutex;

lazy_static::lazy_static! {
	// Per-role mutexes for thread-safe history operations
	static ref HISTORY_MUTEXES: Mutex<HashMap<String, Mutex<()>>> = Mutex::new(HashMap::new());
}

/// Get the history directory path
pub fn get_history_dir() -> Result<PathBuf> {
	let data_dir = crate::directories::get_octomind_data_dir()?;
	let history_dir = data_dir.join("history");

	// Ensure history directory exists
	if !history_dir.exists() {
		fs::create_dir_all(&history_dir)
			.with_context(|| format!("Failed to create history directory: {:?}", history_dir))?;
	}

	Ok(history_dir)
}

/// Get the session history file path for a specific role
pub fn get_session_history_file_path(role: &str) -> Result<PathBuf> {
	let history_dir = get_history_dir()?;
	Ok(history_dir.join(format!("session_{}.history", role)))
}

/// Get the ask command history file path
pub fn get_ask_history_file_path() -> Result<PathBuf> {
	let history_dir = get_history_dir()?;
	Ok(history_dir.join("ask.history"))
}

/// Get or create a mutex for a specific role's history file
fn get_role_mutex(role: &str) -> std::sync::MutexGuard<'static, ()> {
	let mut mutexes = HISTORY_MUTEXES.lock().unwrap();

	// Create mutex for this role if it doesn't exist
	if !mutexes.contains_key(role) {
		mutexes.insert(role.to_string(), Mutex::new(()));
	}

	// Get reference to the mutex (this is safe because we never remove mutexes)
	let mutex_ref = mutexes.get(role).unwrap() as *const Mutex<()>;
	drop(mutexes); // Release the outer mutex

	// Safety: The mutex reference is valid because we never remove mutexes from the HashMap
	unsafe { (*mutex_ref).lock().unwrap() }
}

/// Migrate old global history file to role-based system
fn migrate_legacy_history_if_needed(role: &str) -> Result<()> {
	// Only migrate for developer role to avoid duplicating history across roles
	if role != "developer" {
		return Ok(());
	}

	let history_dir = get_history_dir()?;
	let legacy_history = history_dir.parent().unwrap().join("history");
	let role_history_path = get_session_history_file_path(role)?;

	// Check if legacy global history file exists and role-specific file doesn't exist yet
	if legacy_history.exists() && legacy_history.is_file() && !role_history_path.exists() {
		crate::log_debug!(
			"Migrating legacy global history to role-based system for role: {}",
			role
		);

		// Read legacy history
		let legacy_content = fs::read_to_string(&legacy_history)
			.with_context(|| format!("Failed to read legacy history file: {:?}", legacy_history))?;

		// Create new role-specific history file with version marker
		let mut role_file = OpenOptions::new()
			.create(true)
			.truncate(true)
			.write(true)
			.open(&role_history_path)
			.with_context(|| {
				format!(
					"Failed to create role history file: {:?}",
					role_history_path
				)
			})?;

		writeln!(role_file, "# OCTOMIND_HISTORY_VERSION=1")?;

		// Process and migrate each line from legacy file
		for line in legacy_content.lines() {
			if !line.trim().is_empty() {
				// Encode newlines to preserve multiline entries as single history records
				let encoded_line = line.replace("\\", "\\\\").replace("\n", "\\n");
				writeln!(role_file, "{}", encoded_line)?;
			}
		}

		role_file.flush()?;

		// Rename legacy file to prevent future conflicts
		let backup_path = history_dir.parent().unwrap().join("history.migrated");
		fs::rename(&legacy_history, &backup_path).with_context(|| {
			format!("Failed to rename legacy history file to: {:?}", backup_path)
		})?;

		crate::log_debug!(
			"Successfully migrated legacy history to: {:?}",
			role_history_path
		);
		crate::log_debug!("Legacy file backed up to: {:?}", backup_path);
	}

	Ok(())
}

/// Load history from a role-specific file
pub fn load_session_history_from_file(role: &str) -> Result<Vec<String>> {
	let _lock = get_role_mutex(role);

	// Migrate legacy history if needed (only for developer role)
	migrate_legacy_history_if_needed(role)?;

	let history_path = get_session_history_file_path(role)?;

	if !history_path.exists() {
		return Ok(Vec::new());
	}

	let file = std::fs::File::open(&history_path)
		.with_context(|| format!("Failed to open history file: {:?}", history_path))?;

	let reader = BufReader::new(file);
	let mut history_lines = Vec::new();

	for line in reader.lines() {
		let line = line.with_context(|| "Failed to read line from history file")?;

		// Skip version marker and empty lines
		if line.starts_with("# OCTOMIND_HISTORY_VERSION") || line.trim().is_empty() {
			continue;
		}

		// Decode newlines to restore multiline entries
		let decoded_line = line.replace("\\n", "\n").replace("\\\\", "\\");
		history_lines.push(decoded_line);
	}

	Ok(history_lines)
}

/// Append a line to role-specific history file
pub fn append_to_session_history_file(role: &str, line: &str) -> Result<()> {
	let _lock = get_role_mutex(role);
	let history_path = get_session_history_file_path(role)?;

	// Ensure file exists with version marker
	if !history_path.exists() {
		let mut file = OpenOptions::new()
			.create(true)
			.truncate(true)
			.write(true)
			.open(&history_path)
			.with_context(|| format!("Failed to create history file: {:?}", history_path))?;

		writeln!(file, "# OCTOMIND_HISTORY_VERSION=1")?;
		file.flush()?;
	}

	// Append the new line
	let mut file = OpenOptions::new()
		.create(true)
		.append(true)
		.open(&history_path)
		.with_context(|| format!("Failed to open history file for append: {:?}", history_path))?;

	// Encode newlines to preserve multiline entries as single history records
	let encoded_line = line.replace("\\", "\\\\").replace("\n", "\\n");
	writeln!(file, "{}", encoded_line)?;
	file.flush()?;

	Ok(())
}
