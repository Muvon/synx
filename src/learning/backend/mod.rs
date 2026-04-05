// Copyright 2026 Muvon Un Limited
//
// Licensed under the Apache License, Version 2.0 (the "License")

pub mod file;
pub mod mcp;

use super::Lesson;
use crate::config::Config;
use anyhow::Result;
use async_trait::async_trait;

/// Backend trait for learning storage and retrieval.
///
/// File backend: writes `.md` files, grep-based retrieval.
/// MCP backend: calls external tools with field mapping.
#[async_trait]
pub trait LearningBackend: Send + Sync {
	/// Store a lesson.
	async fn store(&self, lesson: &Lesson, config: &Config) -> Result<()>;

	/// Retrieve lessons matching the given patterns/query.
	/// For file backend: `patterns` are grep keywords.
	/// For MCP backend: `patterns` is a semantic query string (single element).
	async fn retrieve(
		&self,
		patterns: &[String],
		role: &str,
		project: &str,
		limit: usize,
		config: &Config,
	) -> Result<Vec<Lesson>>;

	/// Retrieve ALL lessons for a role/project (used for dedup during extraction).
	async fn retrieve_all(&self, role: &str, project: &str, config: &Config)
		-> Result<Vec<Lesson>>;
}

/// Create a backend based on config.
pub fn create_backend(config: &super::LearningConfig) -> Box<dyn LearningBackend> {
	match config.backend.as_str() {
		"mcp" => Box::new(mcp::McpBackend::new(config)),
		_ => Box::new(file::FileBackend),
	}
}
