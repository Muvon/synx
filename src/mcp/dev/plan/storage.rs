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

//! Storage abstraction for plan tool

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
	pub title: String,
	pub tasks: Vec<PlanTask>,
	pub current_task_index: usize,
	pub created_at: DateTime<Utc>,
	pub status: PlanStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanTask {
	pub title: String,
	pub details: String,         // Progress details from `step` commands
	pub summary: Option<String>, // Final summary from `next` command
	pub status: TaskStatus,
	pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlanStatus {
	Active,
	Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskStatus {
	InProgress,
	Completed,
}

/// Storage abstraction for plan execution
pub trait PlanStorage {
	/// Create new execution plan with tasks
	fn create_plan(&mut self, title: String, tasks: Vec<String>) -> Result<()>;

	/// Add details to current task (can be called multiple times)
	fn add_step_details(&mut self, content: String) -> Result<()>;

	/// Get current task details
	fn get_current_step_details(&self) -> Result<String>;

	/// Complete current task with summary and move to next
	fn complete_current_task(&mut self, summary: String) -> Result<()>;

	/// Check if there are more tasks to complete
	fn has_more_tasks(&self) -> Result<bool>;

	/// Get task list with status (titles only)
	fn get_task_list(&self) -> Result<Vec<(String, TaskStatus)>>;

	/// Get current task info
	fn get_current_task_info(&self) -> Result<(usize, usize, String)>; // (current, total, title)

	/// Mark entire plan as completed
	fn complete_plan(&mut self, summary: String) -> Result<()>;

	/// Clear all plan data
	fn clear_plan(&mut self) -> Result<()>;

	/// Check if plan exists
	fn has_active_plan(&self) -> Result<bool>;

	/// Get plan title
	fn get_plan_title(&self) -> Result<String>;
}
