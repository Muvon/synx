<div align="center">
  <a href="https://octomind.run" target="_blank">
    <img src="doc/assets/banner.png" width="720" alt="Octomind — AI Agent Runtime" />
  </a>
  <br /><br />
  <strong>Specialist AI agents that just work.</strong><br />
  <em>The open-source runtime for community-built experts — any domain, zero setup.</em>
  <br /><br />

  [![License](https://img.shields.io/badge/license-Apache%202.0-7c3aed?style=flat-square)](LICENSE)
  [![GitHub stars](https://img.shields.io/github/stars/muvon/octomind?style=flat-square&color=7c3aed)](https://github.com/muvon/octomind/stargazers)
  [![Website](https://img.shields.io/badge/website-octomind.run-7c3aed?style=flat-square)](https://octomind.run)

  <br />

  [Documentation](https://octomind.run/docs/) · [Community Tap](https://github.com/muvon/octomind-tap) · [Website](https://octomind.run)
</div>

---

## Table of Contents

- [The Problem](#the-problem)
- [The Solution: Tap Agents](#the-solution-tap-agents)
- [Quick Start](#quick-start)
- [How It Works](#how-it-works)
- [Runtime Self-Extension](#runtime-self-extension)
- [Key Features](#key-features)
- [Installation](#installation)
- [Usage](#usage)
- [Configuration](#configuration)
- [Architecture](#architecture)
- [Contributing](#contributing)
- [License](#license)

---

## The Problem

You want an AI that actually knows your domain. Instead you get:

- **45 minutes of setup** — MCP servers, system prompts, tool configs, wiring everything together
- **Rate limit walls mid-task** — Claude Code throttles you, Cursor burns your budget, you lose the thread
- **Context rot** — session fills up, agent forgets decisions from an hour ago, you restart from zero
- **One-size-fits-all** — the same generic assistant whether you're debugging Rust, interpreting a blood test, or reviewing a contract

Every AI tool in 2026 is a coding assistant that lets you swap models. **That's it.**

---

## The Solution: Tap Agents

Octomind is different. It's not a framework you configure. It's a **runtime** for specialist agents — any domain — where the community has already done the hard work.

**You just run it.**

```bash
# Install once
curl -fsSL https://raw.githubusercontent.com/muvon/octomind/master/install.sh | bash
export OPENROUTER_API_KEY="your_key"

# Run any community-built specialist — zero setup
octomind run developer:rust       # Senior Rust dev, full toolchain pre-wired
octomind run doctor:blood          # Medical lab analyst, reads your actual results
octomind run devops:kubernetes     # K8s operator with kubectl + helm ready
octomind run security:pentest      # Security specialist, offensive tooling attached
octomind run legal:contracts       # Contract reviewer, jurisdiction-aware
octomind run finance:analyst       # Financial analyst, wired to your data sources
```

### What Happens When You Run `octomind run doctor:blood`

```
→ Fetches the agent manifest from the tap registry
→ Installs required binaries automatically (skips if already present)
→ Prompts once for any credentials → saves permanently, never asks again
→ Spins up the right MCP servers for this domain
→ Loads specialist model config, system prompt, tool permissions
→ Ready in ~5 seconds. Not 45 minutes.
```

This isn't a prompt or a skill. It's **packaged expertise** — ready to run.

### The Tap Registry

The tap is a community-driven Git registry. Each agent is a complete, battle-tested configuration built by a domain expert:

- ✅ The optimal model for that field
- ✅ The right MCP servers pre-wired (databases, APIs, domain tools)
- ✅ A specialist system prompt written by someone who knows the domain
- ✅ Tool permissions scoped correctly
- ✅ Dependencies that auto-install on first run
- ✅ Credential management — asks once, stores permanently

**Not a prompt file. Not a skill injection. The full stack, configured by the community, ready to run.**

```bash
# Official tap included by default — just run
octomind run developer:rust
octomind run doctor:blood
octomind run doctor:nutrition

# Add any community or team tap
octomind tap yourteam/tap              # clones github.com/yourteam/octomind-tap
octomind tap yourteam/internal ~/path  # local tap for private agents

# Agents from your new tap are immediately available
octomind run finance:analyst
octomind run legal:contracts
```

Each tap is a Git repo. Each agent is one TOML file. Pull requests are contributions.

> **Want to add your expertise?** A `developer:golang` agent, a `doctor:ecg` agent, a `lawyer:gdpr` agent — one file, and everyone benefits. [How to write a tap agent →](https://github.com/muvon/octomind-tap)

---

## Quick Start

```bash
# Install (macOS & Linux)
curl -fsSL https://raw.githubusercontent.com/muvon/octomind/master/install.sh | bash

# One API key gets you all providers (or use any directly)
export OPENROUTER_API_KEY="your_key"

# Start with a specialist agent — no setup required
octomind run developer:rust
```

That's it. You're in an interactive session with a Rust specialist that can read your code, run commands, and edit files.

---

## How It Works

### Core MCP Tools

Octomind has 5 built-in MCP tools that every agent has access to:

| Tool | Purpose |
|------|---------|
| `plan` | Structured multi-step task tracking for complex work |
| `mcp` | Enable/disable MCP servers at runtime — agents can acquire new capabilities on demand |
| `agent` | Spawn specialist sub-agents mid-session — delegate to the right expert |
| `schedule` | Schedule messages to be injected at future times |
| `skill` | Inject reusable instruction packs from taps |

### Filesystem Tools (via octofs)

File operations come from [octofs](https://github.com/muvon/octofs) — a companion MCP server for file read/write/edit:

| Tool | Purpose |
|------|---------|
| `view` | Read files, directories, search content |
| `text_editor` | Create files, replace text, undo edits |
| `batch_edit` | Atomic multi-file edits with rollback |
| `shell` | Execute commands with output capture |
| `semantic_search` | Search codebase by meaning |
| `structural_search` | Search code by AST patterns |
| `workdir` | Get/set working directory |

**octofs is external but recommended.** Tap formulas include it by default — plug and play.

### Memory & Knowledge Tools (via octomem)

Long-term memory and knowledge indexing:

| Tool | Purpose |
|------|---------|
| `remember` | Semantic search over stored memories |
| `memorize` | Store information for future sessions |
| `knowledge` | Index and search documents, URLs, files |
| `graphrag` | Knowledge graph queries over codebase |

**octomem is external but recommended.** Included in tap formulas that need persistent context.

---

## Runtime Self-Extension

This is the capability nobody else has.

Octomind agents have two built-in power tools — `mcp` and `agent` — that let them **acquire new capabilities and spawn specialist sub-agents mid-session**, without any restart or config change.

```
User: "Cross-reference our Postgres metrics with the deployment log and find the anomaly"

Agent:
  → Uses `mcp` tool: registers + enables a Postgres MCP server on the fly
  → Uses `agent` tool: spawns a log_reader sub-agent for the deployment log
  → Both run in parallel, results merged
  → Agent disables the Postgres MCP when done
  → Presents the analysis
```

The tap gives the agent its starting configuration. The `mcp` and `agent` tools give it room to go beyond — acquiring exactly what it needs, when it needs it, and nothing more.

**No other tool lets the AI extend its own capabilities at runtime.** This is the octopus advantage: eight arms, infinite domains, one coordinated mind.

---

## Key Features

### 🎯 Zero Config. Infinitely Configurable.

For most people: install, run, done. No config file needed.

For power users and teams: Octomind has the deepest configuration system in the space — **all TOML, no code required**.

```toml
# Per-role: independent model, temperature, MCP servers, tools, system prompt
[[roles]]
name = "senior-reviewer"
model = "anthropic:claude-opus-4"
temperature = 0.2
[roles.mcp]
server_refs = ["filesystem", "github"]
allowed_tools = ["view", "ast_grep", "create_pr"]

# Multi-step workflows: each step its own model + toolset
[[workflows]]
name = "deep_review"
[[workflows.steps]]
name = "analyze"   # gemini-2.5-flash for broad context gathering
layer = "context_researcher"
[[workflows.steps]]
name = "critique"  # claude-opus for precision judgment
layer = "senior_reviewer"

# Spending limits — never get surprised
max_request_spending_threshold = 0.50
max_session_spending_threshold = 5.00

# Sandbox: lock all writes to current directory
sandbox = true
```

### 🧠 Infinite Sessions With Adaptive Compression

Context rot is the silent productivity killer. Session fills up → quality drops → agent forgets what it decided an hour ago → you restart and lose everything.

Octomind's adaptive compression engine runs automatically in the background:

- **Cache-aware** — calculates if compression is worth it *before* paying for it
- **Pressure-level system** — compresses more aggressively as context grows
- **Structured preservation** — keeps decisions, file references, architectural choices; discards noise
- **Fully automatic** — you never think about it

Work on a hard problem for 4 hours. The agent still knows what it decided in hour one.

### 🔄 13+ Providers. Switch Instantly. Zero Lock-in.

```bash
# Hit a rate limit? Switch mid-session — no restart, no lost context
/model deepseek:v3

# Override for one session
octomind run --model openai:gpt-4o

# Mix providers across workflow layers
# cheap model for research → best model for execution
```

| Provider | Notes |
|----------|-------|
| **OpenRouter** | Every frontier model, one API key |
| **OpenAI** | GPT-4o, o3, Codex |
| **Anthropic** | Claude Opus, Sonnet, Haiku |
| **Google** | Gemini 2.5 Pro/Flash |
| **Amazon Bedrock** | Claude + Titan on AWS |
| **Cloudflare** | Workers AI |
| **DeepSeek** | V3, R1 — best cost/performance ratio |

Real-time cost tracking per session and per request. Know exactly what you're spending.

### 🌐 Works Everywhere — Plug Into Anything

Octomind isn't just an interactive terminal tool. It runs in every context you need:

```bash
# Interactive — daily driver
octomind run developer:rust

# Non-interactive — pipe tasks directly from scripts or CI/CD
echo "review this PR for security issues" | octomind run --format jsonl

# Daemon mode — long-running background agent
octomind run --name myagent --daemon --format plain

# Send messages to running daemon from anywhere
echo "check the build status" | octomind send --name myagent

# WebSocket server — connect IDE plugins, dashboards, automation
octomind server --port 8080

# ACP protocol — drop into any multi-agent system as a sub-agent
octomind acp developer:rust
```

| Mode | Use For |
|------|---------|
| Interactive CLI | Daily work, any domain |
| `--format jsonl` pipe | CI/CD pipelines, shell scripts, automation |
| `--daemon` + `send` | Background agents, continuous monitoring, long-running tasks |
| WebSocket server | IDE plugins, web dashboards, external integrations |
| ACP protocol | Multi-agent orchestration, being called by other agents |

One binary. Every workflow. Eight arms, infinite domains.

---

## Installation

### One-Line Install

```bash
curl -fsSL https://raw.githubusercontent.com/muvon/octomind/master/install.sh | bash
```

Detects your OS and architecture, downloads the latest release, installs to `~/.local/bin/`.

### Package Managers

```bash
# Cargo (Rust)
cargo install octomind
```

Requires Rust 1.82+. See [Building from Source](doc/dev/01-building-from-source.md) for development setup.

### Build from Source

```bash
git clone https://github.com/muvon/octomind.git
cd octomind
cargo build --release
```

### API Key Setup

Set at least one provider API key:

```bash
# OpenRouter (recommended — access to many providers)
export OPENROUTER_API_KEY="your_key"

# Or use a specific provider
export OPENAI_API_KEY="your_key"
export ANTHROPIC_API_KEY="your_key"
export DEEPSEEK_API_KEY="your_key"
```

Add to your shell profile (`~/.bashrc`, `~/.zshrc`) for persistence.

### Verification

```bash
octomind --version
octomind config          # generate default config
octomind run             # start your first session
```

**macOS and Linux supported.** Single Rust binary. Fast startup. No runtime dependencies.

---

## Usage

### Interactive Session

```bash
# Default: interactive with tools
octomind run

# Named session (resume later)
octomind run --name my-feature

# Resume a session
octomind run --resume my-feature

# Tap agent (fetches specialized config)
octomind run developer:rust
```

### Non-Interactive Mode

```bash
# Single message, plain output
octomind run developer "Explain the auth module" --format plain

# Structured JSON output for pipelines
octomind run developer "List TODO items" --schema todos.json --format jsonl
```

### Session Commands

| Command | Description |
|---------|-------------|
| `/help` | Show all commands |
| `/info` | Token usage and costs |
| `/model <provider:model>` | Switch AI model mid-session |
| `/role <name>` | Switch role mid-session |
| `/save` | Save current session |
| `/exit` | Exit session |

See [Session Commands](doc/reference/02-session-commands.md) for the full list.

---

## Configuration

Configuration lives at `~/.local/share/octomind/config/config.toml`.

```bash
# View current config
octomind config --show

# Validate config
octomind config --validate
```

Key configuration areas:

- **Roles** — model, temperature, system prompt, MCP servers, tool permissions
- **Workflows** — multi-step AI processing with validation loops
- **Pipelines** — deterministic script-driven pre-processing
- **MCP Servers** — external tools and capabilities
- **Spending Limits** — per-request and per-session thresholds

See [Configuration Reference](doc/reference/03-config-reference.md) for all options.

---

## Architecture

```
CLI / WebSocket
      |
      v
 ChatSession          <- src/session/chat/session/ (core.rs, main_loop.rs)
      |
      +-- Roles       <- src/config/roles.rs (model, system prompt, MCP servers per role)
      +-- Layers      <- src/session/layers/ (chained AI sub-agents, run after each response)
      +-- Pipelines   <- src/session/pipelines/ (deterministic script steps, run before workflows)
      +-- Workflows   <- src/session/workflows/ (multi-step orchestrated task runners)
      +-- Learning    <- src/learning/ (cross-session lesson extraction and injection)
      |
      +-- MCP servers <- src/mcp/
            +-- core/     plan, mcp, agent, schedule, skill (built-in)
            +-- (filesystem tools provided by external octofs MCP server)
            +-- (memory tools provided by external octomem MCP server)
            +-- agent/    agent_* tools -> route tasks to configured layers
```

**Config is the single source of truth.** All defaults live in `config-templates/default.toml`. The resolved config drives everything: which model, which MCP servers, which layers, which role. No hardcoded values anywhere in code.

See [Architecture](doc/dev/02-architecture.md) for detailed internals.

---

## Contributing

The most impactful contribution isn't code — **it's agents.**

Every domain expert who publishes a specialist makes Octomind useful for an entirely new audience. A cardiologist publishing `doctor:ecg`. A tax attorney publishing `lawyer:tax`. A genomics researcher publishing `scientist:genomics`. One TOML file — and everyone with that problem gets a specialist-grade AI instantly.

`accountant:tax`, `devops:terraform`, `designer:ux-review`, `scientist:genomics` — the registry grows one file at a time.

- [How to write a tap agent](https://github.com/muvon/octomind-tap)
- [Open issues](https://github.com/muvon/octomind/issues)
- [Building from source](doc/dev/01-building-from-source.md)

---

## Documentation

- [Installation & Setup](doc/usage/01-installation.md)
- [Quickstart](doc/usage/02-quickstart.md)
- [Configuration](doc/usage/03-configuration.md)
- [Providers & Models](doc/usage/04-providers.md)
- [Sessions & Compression](doc/usage/05-sessions.md)
- [Roles](doc/usage/06-roles.md)
- [MCP Tools](doc/usage/07-mcp-tools.md)
- [Workflows](doc/usage/09-workflows.md)
- [Pipelines](doc/usage/14-pipelines.md)
- [Learning](doc/usage/13-learning.md)
- [WebSocket & ACP](doc/integration/01-websocket-server.md)
- [CLI Reference](doc/reference/01-cli-reference.md)
- [Config Reference](doc/reference/03-config-reference.md)

Full documentation index: [doc/README.md](doc/README.md)

---

## License

Apache License 2.0 — see [LICENSE](LICENSE).

---

**Octomind** by [Muvon](https://muvon.io) | [Website](https://octomind.run) | [Documentation](https://octomind.run/docs/)
