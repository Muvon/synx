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

use serde::{Deserialize, Serialize};

// Type-specific MCP server configuration using tagged enums
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum McpServerConfig {
	#[serde(rename = "builtin")]
	Builtin {
		name: String,
		timeout_seconds: u64,
		tools: Vec<String>,
	},
	#[serde(rename = "http")]
	Http {
		name: String,
		#[serde(flatten)]
		connection: HttpConnection,
		timeout_seconds: u64,
		tools: Vec<String>,
	},
	#[serde(rename = "stdin")]
	Stdin {
		name: String,
		command: String,
		args: Vec<String>,
		timeout_seconds: u64,
		tools: Vec<String>,
	},
}

// HTTP connection variants - remote vs local
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum HttpConnection {
	Remote {
		url: String,
		#[serde(skip_serializing_if = "Option::is_none")]
		auth_token: Option<String>,
	},
	Local {
		command: String,
		#[serde(default)]
		args: Vec<String>,
		#[serde(skip_serializing_if = "Option::is_none")]
		auth_token: Option<String>,
	},
}

// Legacy connection type enum for backward compatibility in some functions
#[derive(Debug, Clone, Copy, PartialEq, Hash)]
pub enum McpConnectionType {
	Builtin,
	Stdin,
	Http,
}

impl McpServerConfig {
	/// Get the server name regardless of variant
	pub fn name(&self) -> &str {
		match self {
			McpServerConfig::Builtin { name, .. } => name,
			McpServerConfig::Http { name, .. } => name,
			McpServerConfig::Stdin { name, .. } => name,
		}
	}

	/// Get the connection type for compatibility
	pub fn connection_type(&self) -> McpConnectionType {
		match self {
			McpServerConfig::Builtin { .. } => McpConnectionType::Builtin,
			McpServerConfig::Http { .. } => McpConnectionType::Http,
			McpServerConfig::Stdin { .. } => McpConnectionType::Stdin,
		}
	}

	/// Get timeout seconds regardless of variant
	pub fn timeout_seconds(&self) -> u64 {
		match self {
			McpServerConfig::Builtin {
				timeout_seconds, ..
			} => *timeout_seconds,
			McpServerConfig::Http {
				timeout_seconds, ..
			} => *timeout_seconds,
			McpServerConfig::Stdin {
				timeout_seconds, ..
			} => *timeout_seconds,
		}
	}

	/// Get tools list regardless of variant
	pub fn tools(&self) -> &[String] {
		match self {
			McpServerConfig::Builtin { tools, .. } => tools,
			McpServerConfig::Http { tools, .. } => tools,
			McpServerConfig::Stdin { tools, .. } => tools,
		}
	}

	/// Get URL for HTTP servers (if available)
	pub fn url(&self) -> Option<&str> {
		match self {
			McpServerConfig::Http {
				connection: HttpConnection::Remote { url, .. },
				..
			} => Some(url),
			_ => None,
		}
	}

	/// Get auth token for HTTP servers (if available)
	pub fn auth_token(&self) -> Option<&str> {
		match self {
			McpServerConfig::Http {
				connection: HttpConnection::Remote { auth_token, .. },
				..
			}
			| McpServerConfig::Http {
				connection: HttpConnection::Local { auth_token, .. },
				..
			} => auth_token.as_deref(),
			_ => None,
		}
	}

	/// Get command for command-based servers (if available)
	pub fn command(&self) -> Option<&str> {
		match self {
			McpServerConfig::Stdin { command, .. } => Some(command),
			McpServerConfig::Http {
				connection: HttpConnection::Local { command, .. },
				..
			} => Some(command),
			_ => None,
		}
	}

	/// Get args for command-based servers (if available)
	pub fn args(&self) -> &[String] {
		match self {
			McpServerConfig::Stdin { args, .. } => args,
			McpServerConfig::Http {
				connection: HttpConnection::Local { args, .. },
				..
			} => args,
			_ => &[],
		}
	}

	/// Create a builtin server configuration
	pub fn builtin(name: &str, timeout_seconds: u64, tools: Vec<String>) -> Self {
		Self::Builtin {
			name: name.to_string(),
			timeout_seconds,
			tools,
		}
	}

	/// Create a remote HTTP server configuration
	pub fn remote_http(
		name: &str,
		url: &str,
		timeout_seconds: u64,
		tools: Vec<String>,
		auth_token: Option<String>,
	) -> Self {
		Self::Http {
			name: name.to_string(),
			connection: HttpConnection::Remote {
				url: url.to_string(),
				auth_token,
			},
			timeout_seconds,
			tools,
		}
	}

	/// Create a local HTTP server configuration
	pub fn local_http(
		name: &str,
		command: &str,
		args: Vec<String>,
		timeout_seconds: u64,
		tools: Vec<String>,
		auth_token: Option<String>,
	) -> Self {
		Self::Http {
			name: name.to_string(),
			connection: HttpConnection::Local {
				command: command.to_string(),
				args,
				auth_token,
			},
			timeout_seconds,
			tools,
		}
	}

	/// Create a stdin server configuration
	pub fn stdin(
		name: &str,
		command: &str,
		args: Vec<String>,
		timeout_seconds: u64,
		tools: Vec<String>,
	) -> Self {
		Self::Stdin {
			name: name.to_string(),
			command: command.to_string(),
			args,
			timeout_seconds,
			tools,
		}
	}

	/// Validate the server configuration
	pub fn validate(&self) -> Result<(), String> {
		match self {
			McpServerConfig::Builtin { name, .. } => {
				if name.is_empty() {
					return Err("Builtin server name cannot be empty".to_string());
				}
			}
			McpServerConfig::Http {
				name, connection, ..
			} => {
				if name.is_empty() {
					return Err("HTTP server name cannot be empty".to_string());
				}
				match connection {
					HttpConnection::Remote { url, .. } => {
						if url.is_empty() {
							return Err("Remote HTTP server URL cannot be empty".to_string());
						}
					}
					HttpConnection::Local { command, .. } => {
						if command.is_empty() {
							return Err("Local HTTP server command cannot be empty".to_string());
						}
					}
				}
			}
			McpServerConfig::Stdin { name, command, .. } => {
				if name.is_empty() {
					return Err("Stdin server name cannot be empty".to_string());
				}
				if command.is_empty() {
					return Err("Stdin server command cannot be empty".to_string());
				}
			}
		}
		Ok(())
	}
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct McpConfig {
	// Server registry - array of server configurations (consistent with layers)
	pub servers: Vec<McpServerConfig>,

	// Tool filtering - allows limiting tools across all enabled servers
	pub allowed_tools: Vec<String>,
}

// Role-specific MCP configuration with server_refs
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct RoleMcpConfig {
	// Server references - list of server names from the global registry to use for this role
	// Empty list means MCP is disabled for this role
	pub server_refs: Vec<String>,

	// Tool filtering - allows limiting tools across all enabled servers for this role
	pub allowed_tools: Vec<String>,
}

// REMOVED: Default implementations - all config must be explicit

impl RoleMcpConfig {
	/// Check if MCP is enabled for this role (has any server references)
	pub fn is_enabled(&self) -> bool {
		!self.server_refs.is_empty()
	}

	/// Get enabled servers from the global registry for this role
	/// Now works with array format (consistent with layers)
	pub fn get_enabled_servers(&self, global_servers: &[McpServerConfig]) -> Vec<McpServerConfig> {
		if self.server_refs.is_empty() {
			return Vec::new();
		}

		let mut result = Vec::new();
		for server_name in &self.server_refs {
			// Find server by name in the array
			if let Some(server_config) = global_servers.iter().find(|s| s.name() == *server_name) {
				let mut server = server_config.clone();
				// Apply role-specific tool filtering if specified
				if !self.allowed_tools.is_empty() {
					// Convert patterns to actual tool names for this server
					let filtered_tools = self.expand_patterns_for_server(server_name);
					// Update tools based on server type
					server = match server {
						McpServerConfig::Builtin {
							name,
							timeout_seconds,
							..
						} => McpServerConfig::Builtin {
							name,
							timeout_seconds,
							tools: filtered_tools,
						},
						McpServerConfig::Http {
							name,
							connection,
							timeout_seconds,
							..
						} => McpServerConfig::Http {
							name,
							connection,
							timeout_seconds,
							tools: filtered_tools,
						},
						McpServerConfig::Stdin {
							name,
							command,
							args,
							timeout_seconds,
							..
						} => McpServerConfig::Stdin {
							name,
							command,
							args,
							timeout_seconds,
							tools: filtered_tools,
						},
					};
				}
				result.push(server);
			} else {
				// Note: Using println instead of log_debug since we're in a module
				// The log_debug macro would need to be imported
				println!(
					"DEBUG: Server '{server_name}' referenced by role but not found in global registry"
				);
			}
		}

		result
	}

	/// Expand allowed_tools patterns into actual tool names for a specific server
	/// This converts patterns like "filesystem:*" or "filesystem:text_*" into concrete tool lists
	fn expand_patterns_for_server(&self, server_name: &str) -> Vec<String> {
		let mut expanded_tools = Vec::new();

		for pattern in &self.allowed_tools {
			// Check for server group pattern (e.g., "filesystem:*" or "filesystem:text_*")
			if let Some((server_prefix, tool_pattern)) = pattern.split_once(':') {
				// Check if server matches
				if server_prefix == server_name {
					if tool_pattern == "*" {
						// All tools from this server - return empty to indicate "all tools"
						return Vec::new();
					} else if tool_pattern.ends_with('*') {
						// Prefix matching (e.g., "text_*") - we'll need to get actual tools and filter
						// For now, store the pattern and let the existing filtering handle it
						expanded_tools.push(tool_pattern.to_string());
					} else {
						// Exact tool name within server namespace
						expanded_tools.push(tool_pattern.to_string());
					}
				}
			} else {
				// Exact tool name match (backward compatibility) - include for all servers
				expanded_tools.push(pattern.clone());
			}
		}

		expanded_tools
	}
}

// Note: Core server configurations are now defined in the config file
// The get_core_server_config function is removed as we rely entirely on config
