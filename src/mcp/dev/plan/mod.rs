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

//! Plan tool - structured task execution with step-by-step progression

pub mod core;
pub mod memory_storage;
pub mod storage;

pub use core::{clear_plan_data, execute_plan};
pub use memory_storage::MemoryPlanStorage;
pub use storage::{ExecutionPlan, PlanStatus, PlanStorage, PlanTask, TaskStatus};

use crate::mcp::McpFunction;
use serde_json::json;

/// Get plan function definition for MCP
pub fn get_plan_function() -> McpFunction {
	McpFunction {
        name: "plan".to_string(),
        description: "Execute structured plans with task breakdown and step-by-step progression. Supports creating task lists, tracking progress, and automatic continuation through plan execution.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The operation to perform: start, step, next, list, done, reset",
                    "enum": ["start", "step", "next", "list", "done", "reset"]
                },
                "title": {
                    "type": "string",
                    "description": "Plan title (required for 'start' command)"
                },
                "tasks": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "Array of task titles (required for 'start' command)"
                },
                "content": {
                    "type": "string",
                    "description": "Progress details for 'step' command (adds to current task without completing), completion summary for 'next' command (marks task as done), or final summary for 'done' command"
                }
            },
            "required": ["command"],
            "additionalProperties": false
        }),
    }
}
