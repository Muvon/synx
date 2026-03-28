# Migration Guide

## Provider Format

**Old format:**
```toml
model = "anthropic/claude-sonnet-4"
```

**Current format:**
```toml
model = "openrouter:anthropic/claude-sonnet-4"
```

All models require `provider:model` format. The provider prefix tells Octomind which API to use.

## MCP Configuration

**Old format:**
```toml
[mcp]
enabled = true
providers = ["core"]
```

**Current format:**
```toml
[mcp]
allowed_tools = []

[[mcp.servers]]
name = "core"
type = "builtin"
timeout_seconds = 30
tools = []
```

Each server is now an explicit entry in `[[mcp.servers]]` with type, timeout, and tool filtering.

## Role Configuration

**Old format:**
```toml
[developer]
model = "openrouter:anthropic/claude-sonnet-4"
enable_layers = true

[developer.mcp]
enabled = true
server_refs = ["core"]
```

**Current format:**
```toml
[[roles]]
name = "developer"
model = "openrouter:anthropic/claude-sonnet-4"

[roles.mcp]
server_refs = ["core", "filesystem", "agent"]
allowed_tools = ["core:*", "filesystem:*", "agent:*"]
```

Key changes:
- Roles use `[[roles]]` array format (not `[role_name]` sections)
- `enabled` field removed (roles are always available if defined)
- `enable_layers` removed (layers are controlled by workflows)
- Tool permissions use `allowed_tools` patterns

## Layer Configuration

**Old format:**
```toml
[[layers]]
name = "task_refiner"
builtin = true
enabled = true
enable_tools = true
```

**Current format:**
```toml
[[layers]]
name = "task_refiner"
description = "Refines and clarifies user requests"
model = "openrouter:openai/gpt-4.1-mini"
max_tokens = 2048
```

Key changes:
- `builtin`, `enabled`, `enable_tools` fields removed
- `description` is now required
- Layer is active when referenced by a workflow or command

## Config File Location

**Old location:** `~/.config/octomind/config.toml` or `~/.octomind/config.toml`

**Current location:** `~/.local/share/octomind/config/config.toml`

Override: `OCTOMIND_CONFIG_PATH` environment variable.

## Automatic Upgrade

```bash
octomind config --upgrade
```

This attempts to migrate your config to the latest version. Review the result and adjust manually if needed.

## MCP Server Type

**Old (incorrect in some docs):** `type = "stdin"`

**Correct:** `type = "stdio"`

The server type for local process-based MCP servers is `"stdio"`, not `"stdin"`.
