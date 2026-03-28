# Use Case: AI Self-Configuring MCP Servers at Runtime

Let the AI discover what tools it needs and connect new MCP servers on the fly -- no config changes, no restarts.

## The Problem

You don't always know upfront which tools a session will need. A developer asks "help me with the GitHub issues" but the GitHub MCP server isn't configured. Or the AI realizes mid-task it needs a database tool. Traditionally, you'd stop, edit config, restart. Octomind lets the AI handle this itself.

## Solution

The built-in `mcp` tool lets the AI add, enable, and remove MCP servers during a live session.

### How It Works

The AI can manage its own tool ecosystem at runtime:

```
User: "Check the open GitHub issues for this repo"

AI (thinking): "I don't have GitHub tools. Let me add the GitHub MCP server."

AI calls: mcp(action="add", name="github", server_type="http",
          url="https://api.github.com/mcp", timeout_seconds=30)

AI calls: mcp(action="enable", name="github")

AI: "Connected to GitHub MCP server. Found 12 open issues..."
```

No config changes. No restart. The AI recognized a gap and filled it.

### Example: Full Dynamic Tool Discovery

```
User: "I need to analyze our database schema and cross-reference with GitHub issues"

AI:
  1. mcp(action="list")
     -> Shows: core (builtin), filesystem (stdio) -- no GitHub, no DB tools

  2. mcp(action="add", name="github", server_type="http",
         url="https://api.github.com/mcp", timeout_seconds=30)
     -> "Server 'github' registered"

  3. mcp(action="enable", name="github")
     -> "Server 'github' enabled. Available tools: github_list_issues, github_search_code, ..."

  4. mcp(action="add", name="db", server_type="stdio",
         command="mcp-postgres", args=["--connection", "postgresql://..."])
     -> "Server 'db' registered"

  5. mcp(action="enable", name="db")
     -> "Server 'db' enabled. Available tools: query, list_tables, describe_table, ..."

  6. Now uses both servers to cross-reference issues with schema

  7. When done:
     mcp(action="disable", name="db")  # Clean up sensitive connection
```

### Persisting Dynamic Servers

If a dynamically added server proves useful, persist it to config:

```
AI calls: mcp(action="persist", name="github")
-> Saved to ~/.local/share/octomind/config/mcp-github.toml
-> Auto-binds to current role for future sessions
```

Remove persisted config:
```
AI calls: mcp(action="unpersist", name="github")
-> Removed from config directory
```

### Stdio Servers (Local Tools)

The AI can spin up local tool servers too:

```
AI calls: mcp(action="add", name="octocode", server_type="stdio",
          command="octocode", args=["mcp", "--path=."], timeout_seconds=240)

AI calls: mcp(action="enable", name="octocode")
-> "Server 'octocode' enabled. Available tools: semantic_search, view_signatures, graphrag"
```

### Tool Filtering

Only expose specific tools from a server:

```
AI calls: mcp(action="add", name="github", server_type="http",
          url="https://api.github.com/mcp",
          tools=["github_list_issues", "github_create_issue"])
```

Or use wildcards:
```
tools=["github_*"]
```

### Dynamic Agents Too

The same pattern works for agents. The AI can create specialized agents at runtime:

```
AI calls: agent(action="add", name="db_expert",
           description="Analyzes database schemas and queries",
           system="You are a database expert...",
           server_refs=["filesystem", "db"],
           allowed_tools=["filesystem:view", "db:*"])

AI calls: agent(action="enable", name="db_expert")

AI calls: agent_db_expert(task="Analyze the schema for N+1 query risks")
```

## Practical Scenario: Self-Assembling Toolkit

Prompt the AI to assess what it needs:

```
User: "You have access to the filesystem. Before you start working on this project,
       check what tools would be useful and set them up."

AI:
  1. Reads project files: package.json, Cargo.toml, docker-compose.yml
  2. Discovers: Rust project with PostgreSQL, uses GitHub for issues
  3. Sets up:
     - octocode (code search)
     - postgres MCP (database)
     - github MCP (issues/PRs)
  4. Reports: "I've connected 3 additional tool servers. Ready to work."
```

## MCP Management Commands

| Action | Description |
|--------|-------------|
| `list` | Show all servers with status |
| `add` | Register new server (doesn't connect) |
| `enable` | Connect and activate tools |
| `disable` | Deactivate (keep config) |
| `remove` | Unregister entirely |
| `persist` | Save to config for future sessions |
| `unpersist` | Remove from saved config |

Session command equivalent: `/mcp list`, `/mcp add ...`, etc.

## Key Points

- The AI can add/remove MCP servers without human intervention
- Supports both HTTP (remote) and stdio (local process) servers
- `persist` saves a dynamic server to config for future sessions
- Tool filtering prevents exposing unnecessary capabilities
- Dynamic agents follow the same add/enable/disable/remove pattern
- This is unique to Octomind -- most AI tools require static tool configuration
- The AI can assess a project and self-configure its toolkit
