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

// Result formatters for different search types

use anyhow::{anyhow, Result};
use serde_json::Value;

// Format search results as simple, token-efficient text
pub fn format_search_results(search_result: &Value, query: &str) -> Result<String> {
	// Debug: log the structure we received
	crate::log_debug!(
		"Received search result structure: {}",
		serde_json::to_string_pretty(search_result)
			.unwrap_or_else(|_| "Failed to serialize".to_string())
	);

	// Check if we have web results
	let web_results = search_result
		.get("web")
		.and_then(|w| w.get("results"))
		.and_then(|r| r.as_array())
		.ok_or_else(|| anyhow!("No web results found in search response"))?;

	if web_results.is_empty() {
		return Ok(format!(
			"No web search results found for query: \"{query}\""
		));
	}

	let mut result_text = format!("Web search results for \"{query}\":\n\n");

	for (index, result) in web_results.iter().enumerate() {
		let rank = index + 1;
		let title = result
			.get("title")
			.and_then(|t| t.as_str())
			.unwrap_or("No title");
		let url = result
			.get("url")
			.and_then(|u| u.as_str())
			.unwrap_or("No URL");
		let description = result
			.get("description")
			.and_then(|d| d.as_str())
			.unwrap_or("No description");

		result_text.push_str(&format!(
			"[{}] {} | {} | {}\n",
			rank, title, url, description
		));
	}

	Ok(result_text)
}

// Format image search results as simple, token-efficient text
pub fn format_image_results(search_result: &Value, query: &str) -> Result<String> {
	// Check if we have image results
	let image_results = search_result
		.get("images")
		.and_then(|i| i.get("results"))
		.and_then(|r| r.as_array())
		.ok_or_else(|| anyhow!("No image results found in search response"))?;

	if image_results.is_empty() {
		return Ok(format!(
			"No image search results found for query: \"{}\"",
			query
		));
	}

	let mut result_text = format!("Image search results for \"{}\":\n\n", query);

	for (index, result) in image_results.iter().enumerate() {
		let rank = index + 1;
		let title = result
			.get("title")
			.and_then(|t| t.as_str())
			.unwrap_or("No title");
		let source_url = result
			.get("source")
			.and_then(|s| s.get("url"))
			.and_then(|u| u.as_str())
			.unwrap_or("No source URL");
		let image_url = result
			.get("url")
			.and_then(|u| u.as_str())
			.unwrap_or("No image URL");
		let thumbnail_url = result
			.get("thumbnail")
			.and_then(|t| t.get("url"))
			.and_then(|u| u.as_str())
			.unwrap_or("No thumbnail");

		result_text.push_str(&format!(
			"[{}] {} | {} | {} | {}\n",
			rank, title, source_url, image_url, thumbnail_url
		));
	}

	Ok(result_text)
}

// Format video search results as simple, token-efficient text
pub fn format_video_results(search_result: &Value, query: &str) -> Result<String> {
	// Check if we have video results
	let video_results = search_result
		.get("videos")
		.and_then(|v| v.get("results"))
		.and_then(|r| r.as_array())
		.ok_or_else(|| anyhow!("No video results found in search response"))?;

	if video_results.is_empty() {
		return Ok(format!(
			"No video search results found for query: \"{}\"",
			query
		));
	}

	let mut result_text = format!("Video search results for \"{}\":\n\n", query);

	for (index, result) in video_results.iter().enumerate() {
		let rank = index + 1;
		let title = result
			.get("title")
			.and_then(|t| t.as_str())
			.unwrap_or("No title");
		let url = result
			.get("url")
			.and_then(|u| u.as_str())
			.unwrap_or("No URL");
		let description = result
			.get("description")
			.and_then(|d| d.as_str())
			.unwrap_or("No description");
		let duration = result
			.get("duration")
			.and_then(|d| d.as_str())
			.unwrap_or("Unknown duration");
		let views = result
			.get("views")
			.and_then(|v| v.as_str())
			.unwrap_or("Unknown views");

		result_text.push_str(&format!(
			"[{}] {} | {} | {} | Duration: {} | Views: {}\n",
			rank, title, url, description, duration, views
		));
	}

	Ok(result_text)
}

// Format news search results as simple, token-efficient text
pub fn format_news_results(search_result: &Value, query: &str) -> Result<String> {
	// Check if we have news results
	let news_results = search_result
		.get("news")
		.and_then(|n| n.get("results"))
		.and_then(|r| r.as_array())
		.ok_or_else(|| anyhow!("No news results found in search response"))?;

	if news_results.is_empty() {
		return Ok(format!(
			"No news search results found for query: \"{}\"",
			query
		));
	}

	let mut result_text = format!("News search results for \"{}\":\n\n", query);

	for (index, result) in news_results.iter().enumerate() {
		let rank = index + 1;
		let title = result
			.get("title")
			.and_then(|t| t.as_str())
			.unwrap_or("No title");
		let url = result
			.get("url")
			.and_then(|u| u.as_str())
			.unwrap_or("No URL");
		let description = result
			.get("description")
			.and_then(|d| d.as_str())
			.unwrap_or("No description");
		let age = result
			.get("age")
			.and_then(|a| a.as_str())
			.unwrap_or("Unknown age");
		let source = result
			.get("source")
			.and_then(|s| s.as_str())
			.unwrap_or("Unknown source");

		result_text.push_str(&format!(
			"[{}] {} | {} | {} | {} | Source: {}\n",
			rank, title, url, description, age, source
		));
	}

	Ok(result_text)
}
