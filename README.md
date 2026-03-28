<div align="center">
  <a href="https://octomind.run" target="_blank">
    <img src="doc/assets/banner.png" width="720" alt="Octomind — AI Agent Runtime" />
  </a>
  <br /><br />
  <strong>Specialist AI agents that just work.</strong><br />
  The open-source runtime for community-built experts — any domain, zero setup, no vendor lock-in.
  <br /><br />

  [![License](https://img.shields.io/badge/license-Apache%202.0-7c3aed?style=flat-square)](LICENSE)
  [![GitHub stars](https://img.shields.io/github/stars/muvon/octomind?style=flat-square&color=7c3aed)](https://github.com/muvon/octomind/stargazers)
  [![Website](https://img.shields.io/badge/website-octomind.run-7c3aed?style=flat-square)](https://octomind.run)

  [Documentation](https://octomind.run/docs/) · [Community Tap](https://github.com/muvon/octomind-tap) · [Website](https://octomind.run)
</div>

---

## The Problem

You want an AI that actually knows your domain. Instead you get:

- **45 minutes of setup** — MCP servers, system prompts, tool configs, wiring everything together
- **Rate limit walls mid-task** — Claude Code throttles you, Cursor burns your budget, you lose the thread
- **Context rot** — session fills up, agent forgets decisions from an hour ago, you restart from zero
- **One-size-fits-all** — the same generic assistant whether you're debugging Rust, interpreting a blood test, or reviewing a contract

Every AI tool in 2026 is a coding assistant that lets you swap models. **That's it.**

Octomind is different. It's not a framework you configure. It's a **runtime** for specialist agents — any domain — where the community has already done the hard work. You just run it.

---

## This Is What "Just Works" Looks Like

```bash
# Install once
curl -fsSL https://raw.githubusercontent.com/muvon/octomind/master/install.sh | bash
export OPENROUTER_API_KEY="your_key"

# Run any community-built specialist — zero setup
octomind run developer:rust       # Senior Rust dev, full toolchain pre-wired
octomind run doctor:blood         # Medical lab analyst, reads your actual results
octomind run devops:kubernetes    # K8s operator with kubectl + helm ready
octomind run security:pentest     # Security specialist, offensive tooling attached
octomind run legal:contracts      # Contract reviewer, jurisdiction-aware
octomind run finance:analyst      # Financial analyst, wired to your data sources
```

What happens when you run `octomind run doctor:blood`:

```
→ Fetches the agent manifest from the tap registry
→ Installs required binaries automatically (skips if already present)
→ Prompts once for any credentials → saves permanently, never asks again
→ Spins up the right MCP servers for this domain
→ Loads specialist model config, system prompt, tool permissions
→ Ready in ~5 seconds. Not 45 minutes.
```

This isn't a prompt or a skill. It's **packaged expertise** — ready to run.

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

---

## The Tap: Community-Packaged Expertise

The tap is a community-driven Git registry. Each agent is a complete, battle-tested configuration built by a domain expert:

- ✅ The optimal model for that field
- ✅ The right MCP servers pre-wired (databases, APIs, domain tools)
- ✅ A specialist system prompt written by someone who knows the domain
- ✅ Tool permissions scoped correctly
- ✅ Dependencies that auto-install on first run
- ✅ Credential management — asks once, stores permanently

Not a prompt file. Not a skill injection. **The full stack, configured by the community, ready to run.**

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

> Want to add your expertise? A `developer:golang` agent, a `doctor:ecg` agent, a `lawyer:gdpr` agent — one file, and everyone benefits. [How to write a tap agent →](https://github.com/muvon/octomind-tap)

---

## Agents That Grow Beyond Their Configuration

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

## Zero Config. Infinitely Configurable.

For most people: install, run, done. No config file needed.

For power users and teams: Octomind has the deepest configuration system in the space — and **it's all TOML, no code required**.

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

Every role, every layer, every workflow gets its own model, tools, and temperature. Mix providers freely. Build multi-model pipelines. Customize everything — with just a config file, no code.

---

## Infinite Sessions With Adaptive Compression

Context rot is the silent productivity killer. Session fills up → quality drops → agent forgets what it decided an hour ago → you restart and lose everything.

Octomind's adaptive compression engine runs automatically in the background:

- **Cache-aware** — calculates if compression is worth it *before* paying for it
- **Pressure-level system** — compresses more aggressively as context grows
- **Structured preservation** — keeps decisions, file references, architectural choices; discards noise
- **Fully automatic** — you never think about it

Work on a hard problem for 4 hours. The agent still knows what it decided in hour one.

---

## 13+ Providers. Switch Instantly. Zero Lock-in.

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

---

## Works Everywhere — Plug Into Anything

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

## Built-in Tools

| Category | Tools |
|----------|-------|
| **Search** | `ast_grep` — structural code search (not regex), `view` — smart file/dir reader |
| **Editing** | `text_editor`, `batch_edit` — atomic multi-file edits with rollback |
| **Execution** | `shell` — full command execution with output capture |
| **Planning** | `plan` — structured multi-step task tracking |
| **Self-extension** | `mcp` — enable/disable MCP servers at runtime, `agent` — spawn sub-agents on demand |

---

## Installation

```bash
# One-line install
curl -fsSL https://raw.githubusercontent.com/muvon/octomind/master/install.sh | bash

# Build from source
git clone https://github.com/muvon/octomind.git
cd octomind && cargo build --release
```

**macOS and Linux.** Single Rust binary. Fast startup. No runtime dependencies.

---

## Documentation

- [Installation & Setup](doc/01-installation.md)
- [The Tap — Using & Publishing Agents](doc/tap-guide.md)
- [Configuration Reference](doc/03-configuration.md)
- [Providers & Models](doc/04-providers.md)
- [Sessions & Adaptive Compression](doc/05-sessions.md)
- [Layers, Workflows & Sub-agents](doc/07-command-layers.md)
- [WebSocket & ACP Protocol](doc/09-websocket-server.md)

---

## Contributing

The most impactful contribution isn't code — **it's agents.**

Every domain expert who publishes a specialist makes Octomind useful for an entirely new audience. A cardiologist publishing `doctor:ecg`. A tax attorney publishing `lawyer:tax`. A genomics researcher publishing `scientist:genomics`. One TOML file — and everyone with that problem gets a specialist-grade AI instantly.

`accountant:tax`, `devops:terraform`, `designer:ux-review`, `scientist:genomics` — the registry grows one file at a time.

[How to write a tap agent →](doc/tap-guide.md) | [Open issues →](https://github.com/muvon/octomind/issues)

---

**Octomind** by [Muvon](https://muvon.io) | [Website](https://octomind.run) | [Issues](https://github.com/muvon/octomind/issues)

**License:** Apache 2.0 — © 2025–2026 Muvon Un Limited
