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

use anyhow::Result;
use clap::Args;
use colored::Colorize;
use glob::glob;
use octomind::config::Config;
use octomind::session::chat::markdown::{is_markdown_content, MarkdownRenderer};
use octomind::session::{
	chat_completion_with_provider, ChatCompletionProviderParams, Message, ProviderResponse,
};
use rustyline::error::ReadlineError;
use rustyline::{
	Cmd, CompletionType, ConditionalEventHandler, Config as RustylineConfig, EditMode, Editor,
	Event, EventHandler, KeyEvent, Modifiers, RepeatCount,
};
use std::fs::{self, OpenOptions};
use std::io::IsTerminal;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Args, Debug)]
pub struct AskArgs {
	/// Question or input to ask the AI
	#[arg(value_name = "INPUT")]
	pub input: Option<String>,

	/// Include files as context (supports glob patterns, can be used multiple times)
	#[arg(short = 'f', long = "file", value_name = "FILE_PATTERN")]
	pub files: Vec<String>,

	/// Use a specific model instead of the default (runtime only, not saved)
	#[arg(long)]
	pub model: Option<String>,

	/// Maximum tokens for the AI response (runtime only, not saved)
	#[arg(long)]
	pub max_tokens: Option<u32>,

	/// Temperature for the AI response (0.0 to 1.0, runtime only, not saved)
	#[arg(long)]
	pub temperature: Option<f32>,

	/// Output raw text without markdown rendering
	#[arg(long)]
	pub raw: bool,
}

// Helper function to print content with optional markdown rendering for ask command
fn print_response(content: &str, use_raw: bool, config: &Config) {
	if use_raw {
		// Use plain text output
		println!("{}", content);
	} else if is_markdown_content(content) {
		// Use markdown rendering with theme from config
		let theme = config.markdown_theme.parse().unwrap_or_default();
		let renderer = MarkdownRenderer::with_theme(theme);
		match renderer.render_and_print(content) {
			Ok(_) => {
				// Successfully rendered as markdown
			}
			Err(_) => {
				// Fallback to plain text if markdown rendering fails
				println!("{}", content);
			}
		}
	} else {
		// Use plain text with color for non-markdown content
		println!("{}", content.bright_green());
	}
}

// Helper function to validate file patterns and check if they exist
fn validate_file_patterns(file_patterns: &[String]) -> Result<()> {
	if file_patterns.is_empty() {
		return Ok(());
	}

	let mut has_errors = false;
	let mut total_files = 0;

	for pattern in file_patterns {
		match glob(pattern) {
			Ok(paths) => {
				let mut found_any = false;
				for path_result in paths {
					match path_result {
						Ok(path) => {
							found_any = true;
							total_files += 1;
							if !path.exists() {
								octomind::log_error!(
									"Error: File does not exist: {}",
									path.display()
								);
								has_errors = true;
							} else if !path.is_file() {
								octomind::log_error!(
									"Error: Path is not a file: {}",
									path.display()
								);
								has_errors = true;
							} else if let Err(e) = fs::metadata(&path) {
								octomind::log_error!(
									"Error: Cannot access file {}: {}",
									path.display(),
									e
								);
								has_errors = true;
							}
						}
						Err(e) => {
							octomind::log_error!(
								"Error: Invalid path in pattern '{}': {}",
								pattern,
								e
							);
							has_errors = true;
						}
					}
				}
				if !found_any {
					octomind::log_error!("Error: No files found matching pattern '{}'", pattern);
					has_errors = true;
				}
			}
			Err(e) => {
				octomind::log_error!("Error: Invalid glob pattern '{}': {}", pattern, e);
				has_errors = true;
			}
		}
	}

	if has_errors {
		return Err(anyhow::anyhow!(
			"File validation failed. Please check the file patterns and try again."
		));
	}

	if total_files > 50 {
		octomind::log_error!(
			"Warning: Including {} files as context. This may result in a very large prompt.",
			total_files
		);
	}

	Ok(())
}

// Helper function to read files from glob patterns and format them as context
fn read_files_as_context(file_patterns: &[String]) -> Result<String> {
	if file_patterns.is_empty() {
		return Ok(String::new());
	}

	let mut context = String::new();
	context.push_str("## File Context\n\n");

	for pattern in file_patterns {
		match glob(pattern) {
			Ok(paths) => {
				for path_result in paths {
					match path_result {
						Ok(path) => {
							if let Ok(content) = fs::read_to_string(&path) {
								context.push_str(&format!("### File: {}\n\n", path.display()));
								context.push_str("```\n");
								context.push_str(&content);
								if !content.ends_with('\n') {
									context.push('\n');
								}
								context.push_str("```\n\n");
							} else {
								// This shouldn't happen as we validated earlier, but handle gracefully
								context.push_str(&format!(
									"### File: {} (could not read)\n\n",
									path.display()
								));
							}
						}
						Err(_) => {
							// Skip errors as we already validated
						}
					}
				}
			}
			Err(_) => {
				// Skip errors as we already validated
			}
		}
	}

	Ok(context)
}

// Global mutex for ask history file operations to prevent race conditions
lazy_static::lazy_static! {
	static ref ASK_HISTORY_MUTEX: Mutex<()> = Mutex::new(());
}

// Custom event handler for smart Ctrl+E behavior in ask mode
struct AskSmartCtrlEHandler;

impl ConditionalEventHandler for AskSmartCtrlEHandler {
	fn handle(
		&self,
		_evt: &Event,
		_n: RepeatCount,
		_positive: bool,
		ctx: &rustyline::EventContext,
	) -> Option<Cmd> {
		if ctx.has_hint() {
			Some(Cmd::CompleteHint)
		} else {
			None
		}
	}
}

// Get the ask-specific history file path (in organized history directory)
fn get_ask_history_file_path() -> Result<PathBuf> {
	crate::session::history::get_ask_history_file_path()
}

// Encode/decode functions for ask history (same as session but separate)
fn encode_ask_history_line(line: &str) -> String {
	line.chars()
		.map(|c| match c {
			'\\' => "\\\\".to_string(),
			'\n' => "\\n".to_string(),
			c => c.to_string(),
		})
		.collect()
}

fn decode_ask_history_line(encoded: &str) -> String {
	let mut result = String::new();
	let mut chars = encoded.chars().peekable();

	while let Some(c) = chars.next() {
		if c == '\\' {
			match chars.peek() {
				Some('\\') => {
					chars.next();
					result.push('\\');
				}
				Some('n') => {
					chars.next();
					result.push('\n');
				}
				_ => result.push(c),
			}
		} else {
			result.push(c);
		}
	}
	result
}

// Thread-safe ask history file operations
fn append_to_ask_history_file(line: &str) -> Result<()> {
	let _lock = ASK_HISTORY_MUTEX.lock().unwrap();
	let history_path = get_ask_history_file_path()?;

	if !history_path.exists() {
		let mut file = OpenOptions::new()
			.create(true)
			.truncate(true)
			.write(true)
			.open(&history_path)?;
		file.flush()?;
	}

	let mut file = OpenOptions::new()
		.create(true)
		.append(true)
		.open(&history_path)?;

	let encoded_line = encode_ask_history_line(line);
	writeln!(file, "{}", encoded_line)?;
	file.flush()?;
	Ok(())
}

fn load_ask_history_from_file() -> Result<Vec<String>> {
	let _lock = ASK_HISTORY_MUTEX.lock().unwrap();
	let history_path = get_ask_history_file_path()?;

	if !history_path.exists() {
		return Ok(Vec::new());
	}

	let file = std::fs::File::open(&history_path)?;
	let reader = BufReader::new(file);

	let mut history = Vec::new();
	for line in reader.lines() {
		let line = line?;
		if line.trim().is_empty() || line.starts_with("#") {
			continue;
		}
		let decoded_line = decode_ask_history_line(&line);
		history.push(decoded_line);
	}
	Ok(history)
}

// Helper function to get single-line input interactively using rustyline with ask-specific features
// Matches session behavior exactly but with separate history
fn get_interactive_input() -> Result<String> {
	println!("{}", "Enter your question:".bright_blue());
	println!(
		"{}",
		"- Use Ctrl+J for multiline input, Enter to send".dimmed()
	);
	println!(
		"{}",
		"- Type '/exit' or '/quit' to cancel, or press Ctrl+D".dimmed()
	);
	println!();

	// Configure rustyline with proper completion behavior for ask mode
	let config = RustylineConfig::builder()
		.completion_type(CompletionType::Circular)
		.edit_mode(EditMode::Emacs)
		.auto_add_history(true)
		.bell_style(rustyline::config::BellStyle::None)
		.max_history_size(500)?
		.color_mode(rustyline::ColorMode::Enabled) // Enable proper ANSI color handling
		.build();

	let mut editor: Editor<(), rustyline::history::FileHistory> = Editor::with_config(config)?;

	// Note: CommandHelper is not publicly exported, so we use () for no helper
	// This still provides basic Rustyline functionality without command completion

	// Set up key bindings
	// Ctrl+E for smart hint completion
	editor.bind_sequence(
		Event::KeySeq(vec![KeyEvent::new('e', Modifiers::CTRL)]),
		EventHandler::Conditional(Box::new(AskSmartCtrlEHandler)),
	);
	// Tab for completion
	editor.bind_sequence(
		Event::KeySeq(vec![KeyEvent::new('\t', Modifiers::empty())]),
		EventHandler::Simple(Cmd::Complete),
	);
	// Right arrow to accept hint
	editor.bind_sequence(
		Event::KeySeq(vec![
			KeyEvent::new('\x1b', Modifiers::empty()),
			KeyEvent::new('[', Modifiers::empty()),
			KeyEvent::new('C', Modifiers::empty()),
		]),
		EventHandler::Simple(Cmd::CompleteHint),
	);
	// Ctrl+J to insert newline (for multiline input)
	editor.bind_sequence(
		Event::KeySeq(vec![KeyEvent::new('j', Modifiers::CTRL)]),
		EventHandler::Simple(Cmd::Newline),
	);
	// Enter sends the request (default behavior - no override needed)

	// Load persistent ask history
	match load_ask_history_from_file() {
		Ok(history_lines) => {
			for line in history_lines {
				let _ = editor.add_history_entry(line);
			}
		}
		Err(e) => {
			octomind::log_info!("Could not load ask history: {}", e);
		}
	}

	// Set prompt (no cost display initially, matches session behavior)
	let prompt = "> ".bright_blue().to_string();

	match editor.readline(&prompt) {
		Ok(line) => {
			let trimmed = line.trim();
			if trimmed == "/exit" || trimmed == "/quit" {
				return Err(anyhow::anyhow!("User cancelled input"));
			}

			if trimmed.is_empty() {
				return Err(anyhow::anyhow!("No input provided"));
			}

			// Save to ask-specific history file
			if let Err(e) = append_to_ask_history_file(&line) {
				octomind::log_info!("Could not append to ask history file: {}", e);
			}

			Ok(line)
		}
		Err(ReadlineError::Interrupted) => Err(anyhow::anyhow!("User cancelled input")),
		Err(ReadlineError::Eof) => Err(anyhow::anyhow!("User cancelled input")),
		Err(err) => Err(anyhow::anyhow!("Error reading input: {}", err)),
	}
}

pub async fn execute(args: &AskArgs, config: &Config) -> Result<()> {
	// Validate file patterns first, before any other processing
	if let Err(e) = validate_file_patterns(&args.files) {
		octomind::log_error!("{}", e);
		std::process::exit(1);
	}

	// Determine model to use: either from --model flag or effective config model
	let model = args
		.model
		.clone()
		.unwrap_or_else(|| config.get_effective_model());

	// Determine temperature to use: either from --temperature flag or config default
	let temperature = args.temperature.unwrap_or(config.ask.temperature);
	let top_p = config.ask.top_p;
	let top_k = config.ask.top_k;

	// Simple system prompt for ask command with placeholder processing
	let base_system_prompt = &config.ask.system;
	let current_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
	let system_prompt = crate::session::helper_functions::process_placeholders_async(
		base_system_prompt,
		&current_dir,
	)
	.await;

	// Create a clean config with no MCP servers for ask command
	// This ensures no tools are sent to the API
	let mut clean_config = config.clone();
	clean_config.mcp.servers.clear();

	// Read file context once (validation already done)
	let file_context = read_files_as_context(&args.files)?;

	// Get input from argument, stdin, or interactive mode
	if let Some(input) = &args.input {
		// Single execution mode - input provided via argument
		let full_input = if file_context.is_empty() {
			input.clone()
		} else {
			format!("{}\n\n{}", file_context, input)
		};

		// Execute once and return
		let response = execute_single_query(SingleQueryParams {
			input: &full_input,
			model: &model,
			temperature,
			top_p,
			top_k,
			max_tokens: args
				.max_tokens
				.unwrap_or_else(|| clean_config.get_effective_max_tokens()),
			system_prompt: &system_prompt,
			config: &clean_config,
		})
		.await?;
		print_response(&response.content, args.raw, config);
		Ok(())
	} else if !std::io::stdin().is_terminal() {
		// Read from stdin if it's being piped
		let mut buffer = String::new();
		io::stdin().read_to_string(&mut buffer)?;
		let input = buffer.trim().to_string();

		if input.is_empty() {
			octomind::log_error!("Error: No input provided.");
			std::process::exit(1);
		}

		let full_input = if file_context.is_empty() {
			input
		} else {
			format!("{}\n\n{}", file_context, input)
		};

		// Execute once and return
		let response = execute_single_query(SingleQueryParams {
			input: &full_input,
			model: &model,
			temperature,
			top_p,
			top_k,
			max_tokens: args
				.max_tokens
				.unwrap_or_else(|| clean_config.get_effective_max_tokens()),
			system_prompt: &system_prompt,
			config: &clean_config,
		})
		.await?;
		print_response(&response.content, args.raw, config);
		return Ok(());
	} else {
		// Interactive multimode - no argument provided and stdin is a terminal
		println!(
			"{}",
			"Entering multimode - ask questions continuously (no context preserved)".bright_green()
		);
		println!();

		loop {
			match get_interactive_input() {
				Ok(input) => {
					if input.is_empty() {
						octomind::log_error!("Error: No input provided.");
						continue;
					}

					// Combine input with file context for this query
					let full_input = if file_context.is_empty() {
						input.clone()
					} else {
						format!("{}\n\n{}", file_context, input)
					};

					// Show animation while processing (no cost display)
					let cancel_flag = Arc::new(AtomicBool::new(false));
					let animation_cancel = cancel_flag.clone();

					// Start animation task
					let animation_task = tokio::spawn(async move {
						use octomind::session::chat::show_smart_animation;
						let _ = show_smart_animation(animation_cancel, 0.0).await;
					});

					// Execute the query
					let query_result = execute_single_query(SingleQueryParams {
						input: &full_input,
						model: &model,
						temperature,
						top_p,
						top_k,
						max_tokens: args
							.max_tokens
							.unwrap_or_else(|| clean_config.get_effective_max_tokens()),
						system_prompt: &system_prompt,
						config: &clean_config,
					})
					.await;

					// Cancel animation
					cancel_flag.store(true, Ordering::SeqCst);
					let _ = animation_task.await;

					match query_result {
						Ok(response) => {
							print_response(&response.content, args.raw, config);
							println!(); // Add spacing between responses
						}
						Err(e) => {
							octomind::log_error!("Error: {}", e);
						}
					}
				}
				Err(e) => {
					if e.to_string().contains("User cancelled") {
						println!("Exiting multimode.");
						break;
					} else {
						octomind::log_error!("Error: {}", e);
						continue;
					}
				}
			}
		}

		return Ok(());
	}
}

/// Parameters for executing a single query
struct SingleQueryParams<'a> {
	input: &'a str,
	model: &'a str,
	temperature: f32,
	top_p: f32,
	top_k: u32,
	max_tokens: u32,
	system_prompt: &'a str,
	config: &'a Config,
}

// Helper function to execute a single query
async fn execute_single_query(params: SingleQueryParams<'_>) -> Result<ProviderResponse> {
	// Create messages
	let messages = vec![
		Message {
			role: "system".to_string(),
			content: params.system_prompt.to_string(),
			timestamp: std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap_or_default()
				.as_secs(),
			cached: false,
			tool_call_id: None,
			name: None,
			tool_calls: None,
			images: None,
		},
		Message {
			role: "user".to_string(),
			content: params.input.to_string(),
			timestamp: std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap_or_default()
				.as_secs(),
			cached: false,
			tool_call_id: None,
			name: None,
			tool_calls: None,
			images: None,
		},
	];

	// Call the AI provider
	chat_completion_with_provider(ChatCompletionProviderParams {
		messages: &messages,
		model: params.model,
		temperature: params.temperature,
		top_p: params.top_p,
		top_k: params.top_k,
		max_tokens: params.max_tokens,
		config: params.config,
		max_retries: 0,
	})
	.await
}
