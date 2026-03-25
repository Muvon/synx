// Copyright 2026 Muvon Un Limited
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

//! Skill tool — tap-integrated skill discovery and context injection.
//!
//! Skills are reusable instruction packs stored in taps under
//! `<tap>/skills/<skill-name>/SKILL.md`, following the AgentSkills specification
//! (https://agentskills.io/specification).
//!
//! Actions:
//! - `list`   — discover available skills across all taps (filterable, paginated)
//! - `use`    — inject a skill's content into the current session context
//! - `forget` — remove a skill from context (triggers forced compression)

use crate::mcp::{McpFunction, McpToolCall, McpToolResult};
use serde_json::{json, Value};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Skill metadata (parsed from SKILL.md frontmatter)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SkillMeta {
	pub name: String,
	pub description: String,
	pub compatibility: Option<String>,
	pub license: Option<String>,
	/// Tools this skill requires (from `allowed-tools` frontmatter, space-delimited).
	pub allowed_tools: Vec<String>,
}

/// Parse YAML frontmatter from a SKILL.md file.
/// Frontmatter is delimited by `---` lines at the start of the file.
/// Returns None if the file has no valid frontmatter or missing required fields.
pub(crate) fn parse_skill_meta(content: &str) -> Option<SkillMeta> {
	let content = content.trim_start();
	if !content.starts_with("---") {
		return None;
	}

	// Find the closing ---
	let after_open = content.strip_prefix("---")?.trim_start_matches('\n');
	let end = after_open.find("\n---")?;
	let frontmatter = &after_open[..end];

	let mut name = None;
	let mut description = None;
	let mut compatibility = None;
	let mut license = None;
	let mut allowed_tools = Vec::new();

	for line in frontmatter.lines() {
		if let Some((key, value)) = line.split_once(':') {
			let key = key.trim();
			let value = value
				.trim()
				.trim_matches('"')
				.trim_matches('\'')
				.to_string();
			match key {
				"name" => name = Some(value),
				"description" => description = Some(value),
				"compatibility" => compatibility = Some(value),
				"license" => license = Some(value),
				// Space-delimited list: `allowed-tools: shell view text_editor`
				"allowed-tools" => {
					allowed_tools = value.split_whitespace().map(|s| s.to_string()).collect();
				}
				_ => {}
			}
		}
	}

	Some(SkillMeta {
		name: name?,
		description: description?,
		compatibility,
		license,
		allowed_tools,
	})
}

// ---------------------------------------------------------------------------
// Resource catalog — Tier 3 (scripts/, references/, assets/)
// ---------------------------------------------------------------------------

/// Scan a skill directory for Tier 3 resources and build a catalog string.
///
/// All resources (scripts/, references/, assets/) are listed with their
/// absolute paths. The AI uses `shell` / `view` to access them on demand.
///
/// Returns an empty string when no resources are found.
pub(crate) fn build_resource_catalog(skill_dir: &std::path::Path) -> String {
	let subdirs = ["scripts", "references", "assets"];

	let mut sections: Vec<String> = Vec::new();

	for subdir_name in &subdirs {
		let subdir = skill_dir.join(subdir_name);
		if !subdir.is_dir() {
			continue;
		}

		let mut entries: Vec<_> = match std::fs::read_dir(&subdir) {
			Ok(e) => e.flatten().collect(),
			Err(_) => continue,
		};
		// Sort for deterministic output
		entries.sort_by_key(|e| e.file_name());

		if entries.is_empty() {
			continue;
		}

		let mut section_lines = vec![format!("**{}/**", subdir_name)];

		for entry in &entries {
			let path = entry.path();
			if !path.is_file() {
				continue;
			}
			let fname = path
				.file_name()
				.map(|n| n.to_string_lossy().to_string())
				.unwrap_or_default();
			section_lines.push(format!("- `{}` — {}", fname, path.display()));
		}

		if section_lines.len() > 1 {
			sections.push(section_lines.join("\n"));
		}
	}

	if sections.is_empty() {
		return String::new();
	}

	format!("\n\n## Skill Resources\n\n{}", sections.join("\n\n"))
}

// ---------------------------------------------------------------------------
// Tool compatibility check
// ---------------------------------------------------------------------------

/// Returns the subset of `allowed_tools` that are NOT available in the current
/// session's tool map. An empty result means the skill is fully compatible.
fn missing_tools(allowed_tools: &[String]) -> Vec<String> {
	allowed_tools
		.iter()
		.filter(|t| crate::mcp::tool_map::get_server_for_tool(t).is_none())
		.cloned()
		.collect()
}

// ---------------------------------------------------------------------------
// Skill discovery across all taps
// ---------------------------------------------------------------------------

/// Scan all active taps for skills. Returns (meta, skill_dir) pairs.
/// Skills live at `<tap>/skills/<skill-name>/SKILL.md`.
fn find_all_skills() -> Vec<(SkillMeta, PathBuf)> {
	let taps = match crate::agent::taps::get_taps() {
		Ok(t) => t,
		Err(e) => {
			crate::log_debug!("skill: failed to load taps: {}", e);
			return Vec::new();
		}
	};

	let mut skills = Vec::new();

	for tap in &taps {
		let skills_dir = match tap.skills_dir() {
			Ok(d) => d,
			Err(_) => continue,
		};

		if !skills_dir.exists() {
			continue;
		}

		let entries = match std::fs::read_dir(&skills_dir) {
			Ok(e) => e,
			Err(_) => continue,
		};

		for entry in entries.flatten() {
			let skill_dir = entry.path();
			if !skill_dir.is_dir() {
				continue;
			}

			let skill_md = skill_dir.join("SKILL.md");
			if !skill_md.exists() {
				continue;
			}

			let content = match std::fs::read_to_string(&skill_md) {
				Ok(c) => c,
				Err(_) => continue,
			};

			if let Some(meta) = parse_skill_meta(&content) {
				skills.push((meta, skill_dir));
			}
		}
	}

	skills
}

/// Find a specific skill by name across all taps.
/// Returns (meta, skill_dir, full_content) — reads SKILL.md only once.
fn find_skill_by_name(name: &str) -> Option<(SkillMeta, PathBuf, String)> {
	let taps = match crate::agent::taps::get_taps() {
		Ok(t) => t,
		Err(e) => {
			crate::log_debug!("skill: failed to get taps: {}", e);
			return None;
		}
	};

	for tap in &taps {
		let skills_dir = match tap.skills_dir() {
			Ok(d) => d,
			Err(_) => continue,
		};

		let skill_dir = skills_dir.join(name);
		if !skill_dir.is_dir() {
			continue;
		}

		let skill_md = skill_dir.join("SKILL.md");
		let content = match std::fs::read_to_string(&skill_md) {
			Ok(c) => c,
			Err(_) => continue,
		};

		if let Some(meta) = parse_skill_meta(&content) {
			if meta.name == name {
				return Some((meta, skill_dir, content));
			}
		}
	}

	None
}

// ---------------------------------------------------------------------------
// MCP tool definition
// ---------------------------------------------------------------------------

pub fn get_skill_function() -> McpFunction {
	McpFunction {
		name: "skill".to_string(),
		description: r#"Manage skills from taps. Skills are reusable instruction packs that inject domain knowledge into context.

**Actions:**
- `list`   — discover available skills across all taps. Supports optional `pattern` (substring filter on name/description), `offset`, and `limit` (default 20).
- `use`    — inject a skill's full content into the current session context. The skill instructions become immediately active.
- `forget` — remove a skill from context. Triggers conversation compression to clean up the injected content.

**Workflow:**
1. `skill(action="list")` to explore what's available
2. `skill(action="use", name="skill-name")` to activate a skill
3. `skill(action="forget", name="skill-name")` when the skill is no longer needed"#.to_string(),
		parameters: json!({
			"type": "object",
			"properties": {
				"action": {
					"type": "string",
					"enum": ["list", "use", "forget"],
					"description": "Action to perform: list (discover skills), use (inject skill into context), forget (remove skill from context)"
				},
				"name": {
					"type": "string",
					"description": "Skill name (required for use and forget actions)"
				},
				"pattern": {
					"type": "string",
					"description": "Optional substring filter applied to skill name and description (for list action)"
				},
				"offset": {
					"type": "integer",
					"description": "Pagination offset for list action (default: 0)"
				},
				"limit": {
					"type": "integer",
					"description": "Maximum number of skills to return for list action (default: 20)"
				}
			},
			"required": ["action"]
		}),
	}
}

// ---------------------------------------------------------------------------
// Tool handler
// ---------------------------------------------------------------------------

pub async fn execute_skill_tool(call: &McpToolCall) -> Result<McpToolResult, String> {
	let action = match call.parameters.get("action") {
		Some(Value::String(a)) if !a.trim().is_empty() => a.clone(),
		Some(_) => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"action must be a string".to_string(),
			))
		}
		None => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"missing required parameter: action".to_string(),
			))
		}
	};

	match action.as_str() {
		"list" => execute_list(call),
		"use" => execute_use(call).await,
		"forget" => execute_forget(call),
		other => Ok(McpToolResult::error(
			call.tool_name.clone(),
			call.tool_id.clone(),
			format!(
				"unknown action '{}'. Valid actions: list, use, forget",
				other
			),
		)),
	}
}

fn execute_list(call: &McpToolCall) -> Result<McpToolResult, String> {
	let pattern = match call.parameters.get("pattern") {
		Some(Value::String(p)) => Some(p.to_lowercase()),
		_ => None,
	};

	let offset = match call.parameters.get("offset") {
		Some(Value::Number(n)) => n.as_u64().unwrap_or(0) as usize,
		_ => 0,
	};

	let limit = match call.parameters.get("limit") {
		Some(Value::Number(n)) => n.as_u64().unwrap_or(20) as usize,
		_ => 20,
	};

	let all_skills = find_all_skills();

	// Filter by pattern (case-insensitive substring on name + description)
	let filtered: Vec<_> = all_skills
		.iter()
		.filter(|(meta, _)| {
			if let Some(ref pat) = pattern {
				meta.name.to_lowercase().contains(pat.as_str())
					|| meta.description.to_lowercase().contains(pat.as_str())
			} else {
				true
			}
		})
		.collect();

	let total = filtered.len();

	if total == 0 {
		let msg = if pattern.is_some() {
			"No skills found matching the pattern.".to_string()
		} else {
			"No skills found. Add skills to your tap under skills/<name>/SKILL.md".to_string()
		};
		return Ok(McpToolResult::success(
			call.tool_name.clone(),
			call.tool_id.clone(),
			msg,
		));
	}

	// Paginate
	let page: Vec<_> = filtered.iter().skip(offset).take(limit).collect();
	let page_len = page.len();

	// Get active skills for current session to mark them
	let active_skills = crate::session::context::current_session_id()
		.map(|sid| crate::session::context::get_active_skills(&sid))
		.unwrap_or_default();

	// Format as table
	let mut lines = vec![format!(
		"Found {} skill(s){}:",
		total,
		if pattern.is_some() {
			" matching pattern"
		} else {
			""
		}
	)];
	lines.push(String::new());

	for (meta, _) in page {
		let active_marker = if active_skills.contains(&meta.name) {
			" ✓ [active]"
		} else {
			""
		};
		// Check tool compatibility for this skill
		let unavailable = missing_tools(&meta.allowed_tools);
		let compat_marker = if unavailable.is_empty() {
			String::new()
		} else {
			format!(" ⚠️ [missing tools: {}]", unavailable.join(", "))
		};
		lines.push(format!(
			"**{}**{}{}",
			meta.name, active_marker, compat_marker
		));
		lines.push(format!("  {}", meta.description));
		if let Some(ref compat) = meta.compatibility {
			lines.push(format!("  Compatibility: {}", compat));
		}
		lines.push(String::new());
	}

	if offset + limit < total {
		lines.push(format!(
			"Showing {}-{} of {}. Use offset={} to see more.",
			offset + 1,
			offset + page_len,
			total,
			offset + limit
		));
	}

	Ok(McpToolResult::success(
		call.tool_name.clone(),
		call.tool_id.clone(),
		lines.join("\n"),
	))
}

async fn execute_use(call: &McpToolCall) -> Result<McpToolResult, String> {
	let name = match call.parameters.get("name") {
		Some(Value::String(n)) if !n.trim().is_empty() => n.clone(),
		Some(_) => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"name must be a non-empty string".to_string(),
			))
		}
		None => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"missing required parameter: name".to_string(),
			))
		}
	};

	let session_id = match crate::session::context::current_session_id() {
		Some(id) => id,
		None => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"skill use requires an active session".to_string(),
			))
		}
	};

	// Already active — inform but don't re-inject
	if crate::session::context::has_active_skill(&session_id, &name) {
		return Ok(McpToolResult::success(
			call.tool_name.clone(),
			call.tool_id.clone(),
			format!(
				"Skill '{}' is already active in this session. Use forget to remove it first.",
				name
			),
		));
	}

	// Find the skill across taps (reads SKILL.md once)
	let (meta, skill_dir, content) = match find_skill_by_name(&name) {
		Some(s) => s,
		None => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				format!(
					"Skill '{}' not found. Use skill(action=\"list\") to see available skills.",
					name
				),
			))
		}
	};

	// Tier 3: append resource catalog (scripts/, references/, assets/)
	let resources = build_resource_catalog(&skill_dir);

	// Tool compatibility: warn if required tools are missing in the current role
	let unavailable = missing_tools(&meta.allowed_tools);
	let tool_warning = if unavailable.is_empty() {
		String::new()
	} else {
		format!(
			"\n\n⚠️ This skill requires tools that are not available in the current role: {}\nSome skill functionality may not work. Consider switching to a role that includes these tools.",
			unavailable.join(", ")
		)
	};

	// Register as active
	crate::session::context::add_active_skill(&session_id, &name);

	// Queue skill content for injection as a system message (handled by tool_result_processor)
	let mut injection_content = content;
	if !resources.is_empty() {
		injection_content.push_str(&resources);
	}
	crate::session::context::push_pending_skill_injection(
		&session_id,
		name.clone(),
		injection_content,
	);

	crate::log_debug!(
		"skill: queued '{}' for injection in session {}",
		name,
		session_id
	);

	// Return short confirmation — the actual content is injected as a system message
	let msg = if tool_warning.is_empty() {
		format!("Skill '{}' is now active.", name)
	} else {
		format!("Skill '{}' is now active.{}", name, tool_warning)
	};

	Ok(McpToolResult::success(
		call.tool_name.clone(),
		call.tool_id.clone(),
		msg,
	))
}

fn execute_forget(call: &McpToolCall) -> Result<McpToolResult, String> {
	let name = match call.parameters.get("name") {
		Some(Value::String(n)) if !n.trim().is_empty() => n.clone(),
		Some(_) => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"name must be a non-empty string".to_string(),
			))
		}
		None => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"missing required parameter: name".to_string(),
			))
		}
	};

	let session_id = match crate::session::context::current_session_id() {
		Some(id) => id,
		None => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"skill forget requires an active session".to_string(),
			))
		}
	};

	if !crate::session::context::has_active_skill(&session_id, &name) {
		return Ok(McpToolResult::error(
			call.tool_name.clone(),
			call.tool_id.clone(),
			format!(
				"Skill '{}' is not currently active. Use skill(action=\"list\") to see active skills.",
				name
			),
		));
	}

	crate::session::context::remove_active_skill(&session_id, &name);
	// Signal the session to run forced compression so the injected skill content is cleaned up
	crate::session::context::request_skill_compression(&session_id);

	crate::log_debug!("skill: forgot '{}' from session {}", name, session_id);

	Ok(McpToolResult::success(
		call.tool_name.clone(),
		call.tool_id.clone(),
		format!(
			"Skill '{}' removed from context. Conversation will be compressed to clean up injected content.",
			name
		),
	))
}
