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

// Thinking block display utilities

use crate::providers::ThinkingBlock;
use colored::Colorize;

/// Display thinking block in CLI with proper formatting (matching tool display style)
pub fn display_thinking(thinking: &ThinkingBlock) {
	let title = "thinking".bright_cyan();
	let separator_length = 70.max(title.len() + 4);
	let dashes = "─".repeat(separator_length - title.len());
	let separator = format!("──{}{}──", title, dashes.dimmed());

	println!("{}", separator);
	println!("{}", thinking.content.dimmed());
}
