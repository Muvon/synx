# ACP Protocol

The Agent Client Protocol (ACP) enables Octomind to run as a sub-agent over stdio, communicating via JSON-RPC. This is used for editor integration and agent-to-agent delegation.

## Overview

ACP provides:
- JSON-RPC over stdio communication
- Tool execution with streaming results
- Slash command support
- MCP server injection from the host
- Session lifecycle management

## Starting an ACP Agent

```bash
octomind acp [TAG] [--sandbox]
```

The agent reads JSON-RPC messages from stdin and writes responses to stdout. Stderr is reserved for logging (written to `~/.local/share/octomind/logs/acp-debug.log`).

## Protocol Flow

1. **Host starts** `octomind acp <role>` as a subprocess
2. **Initialization handshake**: host sends capabilities, agent responds with available features
3. **Message exchange**: host sends user messages, agent streams responses
4. **Tool execution**: agent announces tool calls, streams results
5. **Shutdown**: host closes stdin or sends shutdown message

## Use Cases

### Editor Integration

Editors (Neovim, Zed, JetBrains) use ACP to embed Octomind as an AI assistant. See [Editor Integration](../usage/12-editor-integration.md).

### Agent Delegation

Configured agents (`[[agents]]`) spawn ACP subprocesses to handle tasks:

```toml
[[agents]]
name = "context_gatherer"
description = "Gather codebase context"
command = "octomind acp context_gatherer"
workdir = "."
```

When the AI calls `agent_context_gatherer(task="...")`, Octomind:
1. Spawns `octomind acp context_gatherer` as a subprocess
2. Sends the task via JSON-RPC
3. Collects the agent's response (all `agent_message_chunk` text)
4. Returns the result as a tool output

### Custom ACP Servers

The `command` field in `[[agents]]` can point to any ACP-compatible binary, not just Octomind. This enables integration with custom tools and services.

## Error Handling

- Protocol errors are logged to `~/.local/share/octomind/logs/acp-errors.jsonl`
- Structured JSONL format for programmatic error analysis
- Stdout/stderr are separated to prevent protocol corruption

## MCP Server Injection

Hosts can inject additional MCP servers during the ACP initialization handshake. The injected servers become available to the agent's session alongside its configured servers.

This enables editors to provide project-specific tools (e.g., language servers, project databases) to the AI session.
