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

// HTML to Markdown converter module using html2text

use super::super::{McpToolCall, McpToolResult};
use anyhow::{anyhow, Result};
use html2text::from_read;
use reqwest;
use serde_json::{json, Value};
use std::io::Cursor;
use std::path::Path;
use tokio::fs as tokio_fs;
use url::Url;

// Execute HTML to Markdown conversion
pub async fn execute_read_html(call: &McpToolCall) -> Result<McpToolResult> {
	// Extract sources parameter
	let sources_value = match call.parameters.get("sources") {
		Some(value) => value,
		_ => {
			return Ok(McpToolResult::error(
				call.tool_name.clone(),
				call.tool_id.clone(),
				"Missing 'sources' parameter".to_string(),
			))
		}
	};

	// Support either a single source string or an array of sources
	match sources_value {
		Value::String(source) => {
			// Check if this is a stringified JSON array (workaround for AI misunderstanding)
			// Example: sources: "[\"url1\", \"url2\"]" instead of sources: ["url1", "url2"]
			let trimmed = source.trim();
			if trimmed.starts_with('[') && trimmed.ends_with(']') {
				// Try to parse as JSON array
				match serde_json::from_str::<Vec<String>>(trimmed) {
					Ok(parsed_sources) => {
						// Successfully parsed stringified array - convert multiple sources
						convert_multiple_html_to_md(call, &parsed_sources).await
					}
					Err(_) => {
						// Not a valid JSON array, treat as single URL/path
						convert_single_html_to_md(call, source).await
					}
				}
			} else {
				// Regular single source conversion
				convert_single_html_to_md(call, source).await
			}
		}
		Value::Array(sources) => {
			// Multiple sources conversion
			let mut source_strings = Vec::new();
			for source in sources {
				match source.as_str() {
					Some(s) => source_strings.push(s.to_string()),
					None => {
						return Ok(McpToolResult::error(
							call.tool_name.clone(),
							call.tool_id.clone(),
							"Invalid source in array - all sources must be strings".to_string(),
						))
					}
				}
			}

			convert_multiple_html_to_md(call, &source_strings).await
		}
		_ => Ok(McpToolResult::error(
			call.tool_name.clone(),
			call.tool_id.clone(),
			"'sources' parameter must be a string or array of strings".to_string(),
		)),
	}
}

// Convert a single HTML source to Markdown
async fn convert_single_html_to_md(call: &McpToolCall, source: &str) -> Result<McpToolResult> {
	let (html_content, source_type) = fetch_html_content(source).await?;
	let markdown = html_to_markdown(&html_content)?;

	Ok(McpToolResult {
		tool_name: "read_html".to_string(),
		tool_id: call.tool_id.clone(),
		result: json!({
			"success": true,
			"conversions": [{
				"source": source,
				"type": source_type,
				"markdown": markdown,
				"size": markdown.len()
			}],
			"count": 1
		}),
	})
}

// Convert multiple HTML sources to Markdown
async fn convert_multiple_html_to_md(
	call: &McpToolCall,
	sources: &[String],
) -> Result<McpToolResult> {
	let mut conversions = Vec::with_capacity(sources.len());
	let mut failures = Vec::new();

	for source in sources {
		match fetch_html_content(source).await {
			Ok((html_content, source_type)) => match html_to_markdown(&html_content) {
				Ok(markdown) => {
					conversions.push(json!({
						"source": source,
						"type": source_type,
						"markdown": markdown,
						"size": markdown.len()
					}));
				}
				Err(e) => {
					failures.push(format!("Failed to convert {} to markdown: {}", source, e));
				}
			},
			Err(e) => {
				failures.push(format!("Failed to fetch {}: {}", source, e));
			}
		}
	}

	Ok(McpToolResult {
		tool_name: "read_html".to_string(),
		tool_id: call.tool_id.clone(),
		result: json!({
			"success": !conversions.is_empty(),
			"conversions": conversions,
			"count": conversions.len(),
			"failed": failures
		}),
	})
}

// Fetch HTML content from URL or local file
async fn fetch_html_content(source: &str) -> Result<(String, &'static str)> {
	// Check if source is a URL or file path
	if let Ok(url) = Url::parse(source) {
		if url.scheme() == "http" || url.scheme() == "https" {
			// Fetch from URL
			let response = reqwest::get(source).await?;
			if !response.status().is_success() {
				return Err(anyhow!("HTTP error {}: {}", response.status(), source));
			}
			let html = response.text().await?;
			Ok((html, "url"))
		} else if url.scheme() == "file" {
			// Handle file:// URLs
			let path = url
				.to_file_path()
				.map_err(|_| anyhow!("Invalid file URL: {}", source))?;
			let html = tokio_fs::read_to_string(&path).await?;
			Ok((html, "file"))
		} else {
			Err(anyhow!("Unsupported URL scheme: {}", url.scheme()))
		}
	} else {
		// Treat as file path
		let path = Path::new(source);
		if !path.exists() {
			return Err(anyhow!("File does not exist: {}", source));
		}
		if !path.is_file() {
			return Err(anyhow!("Path is not a file: {}", source));
		}
		let html = tokio_fs::read_to_string(path).await?;
		Ok((html, "file"))
	}
}

// Convert HTML to plain text using html2text
fn html_to_markdown(html: &str) -> Result<String> {
	// html2text converts HTML to plain text (not markdown, but similar)
	// We use width=180 to minimize wrapping
	from_read(Cursor::new(html), 180).map_err(|e| anyhow::anyhow!("HTML conversion error: {}", e))
}
