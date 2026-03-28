# Use Case: Custom Hooks -- Build Any Integration in Any Language

Hooks are HTTP listeners backed by scripts you write in any language. You have full control: parse any payload, filter events, transform data, and inject precisely crafted messages into a running AI agent session.

## The Problem

Every team has unique infrastructure -- internal APIs, custom CI systems, proprietary monitoring, Slack bots, Jira workflows. Pre-built integrations never fit. You need to wire arbitrary external events into an AI agent that understands your specific context.

## Solution

A hook is: HTTP endpoint + your script + AI session. That's it. The script is the glue -- write it in Bash, Python, Ruby, Go, Node, Rust, whatever runs on your machine.

### How Hooks Work

```
External System (any HTTP POST)
    |
    v
Hook HTTP Listener (bind address:port)
    |
    | passes raw HTTP body to stdin
    | passes headers as HOOK_HEADER_* env vars
    | passes method, path, query as env vars
    v
Your Script (any language, any logic)
    |
    | exit 0 + stdout → inject message into AI session
    | exit non-zero → ignore (stderr logged)
    v
AI Agent Session (processes message with full tool access)
```

You control everything between the HTTP request and what the AI sees.

### Configuration

```toml
[[hooks]]
name = "my-hook"
bind = "0.0.0.0:9876"
script = "/opt/hooks/my-hook.py"
timeout = 30  # seconds (1-3600)
```

Activate when starting the agent:
```bash
octomind run --name my-agent --daemon --format jsonl --hook my-hook
```

## Examples in Different Languages

### Python: Jira Issue Tracker

```python
#!/usr/bin/env python3
"""Process Jira webhook events and create actionable AI tasks."""
import json, sys, os

payload = json.load(sys.stdin)
event = os.environ.get("HOOK_HEADER_X_ATLASSIAN_WEBHOOK_EVENT", "")

if event == "jira:issue_created":
    issue = payload["issue"]
    key = issue["key"]
    summary = issue["fields"]["summary"]
    description = issue["fields"].get("description", "No description")
    priority = issue["fields"]["priority"]["name"]
    assignee = issue["fields"].get("assignee", {}).get("displayName", "Unassigned")

    print(f"""New Jira issue {key} ({priority}): {summary}
Assigned to: {assignee}

Description:
{description}

Please:
1. Analyze if this issue relates to any recent code changes
2. Identify the relevant source files
3. Suggest an implementation approach if it's a feature, or root cause if it's a bug""")

elif event == "jira:issue_updated":
    changelog = payload.get("changelog", {}).get("items", [])
    status_change = next((c for c in changelog if c["field"] == "status"), None)
    if status_change and status_change["toString"] == "In Review":
        key = payload["issue"]["key"]
        print(f"Issue {key} moved to In Review. Please review the associated code changes.")
    else:
        sys.exit(1)  # Ignore other updates
else:
    sys.exit(1)  # Ignore unknown events
```

### Node.js: Slack Bot

```javascript
#!/usr/bin/env node
const payload = JSON.parse(require('fs').readFileSync('/dev/stdin', 'utf8'));

// Slack sends URL verification challenges
if (payload.type === 'url_verification') {
  // Can't respond directly (hook is one-way), handle elsewhere
  process.exit(1);
}

// Only react to app mentions
if (payload.event?.type !== 'app_mention') {
  process.exit(1);
}

const user = payload.event.user;
const text = payload.event.text.replace(/<@[A-Z0-9]+>/g, '').trim();
const channel = payload.event.channel;

console.log(`Slack request from <@${user}> in #${channel}:

${text}

Respond concisely. Format for Slack (no markdown headers, use *bold* and \`code\`).`);
```

### Bash: Simple Git Post-Receive

```bash
#!/bin/bash
# Minimal hook: extract essentials, let the AI figure out the rest

payload=$(cat)
branch=$(echo "$payload" | jq -r '.ref' | sed 's|refs/heads/||')

# Only care about main and develop
case "$branch" in
  main|develop) ;;
  *) exit 1 ;;
esac

commits=$(echo "$payload" | jq -r '.commits[] | "- \(.message) (\(.author.name))"')
files=$(echo "$payload" | jq -r '.commits[].modified[]' | sort -u)

echo "Push to $branch:
$commits

Files changed:
$files

Review these changes for issues."
```

### Ruby: Custom Monitoring Alert

```ruby
#!/usr/bin/env ruby
require 'json'

payload = JSON.parse($stdin.read)
severity = payload['severity']
service = payload['service']
message = payload['message']
metrics = payload['metrics'] || {}

# Only alert on warning and critical
exit 1 unless %w[warning critical].include?(severity)

puts <<~MSG
  #{severity.upcase} alert from #{service}: #{message}

  Metrics: #{metrics.map { |k, v| "#{k}=#{v}" }.join(', ')}

  Please:
  1. Check the #{service} source code for potential causes
  2. Look at recent changes that might have caused this
  3. Suggest immediate mitigation steps
MSG
```

### Go: High-Performance Webhook Processor

```go
#!/usr/bin/env -S go run
package main

import (
    "encoding/json"
    "fmt"
    "io"
    "os"
)

type DeployEvent struct {
    Environment string `json:"environment"`
    Version     string `json:"version"`
    Status      string `json:"status"`
    Services    []struct {
        Name   string `json:"name"`
        Health string `json:"health"`
    } `json:"services"`
}

func main() {
    data, _ := io.ReadAll(os.Stdin)
    var event DeployEvent
    if err := json.Unmarshal(data, &event); err != nil {
        os.Exit(1)
    }

    if event.Status != "completed" {
        os.Exit(1)
    }

    unhealthy := []string{}
    for _, s := range event.Services {
        if s.Health != "healthy" {
            unhealthy = append(unhealthy, s.Name)
        }
    }

    if len(unhealthy) > 0 {
        fmt.Printf("Deploy %s to %s completed but %d services unhealthy: %v\n",
            event.Version, event.Environment, len(unhealthy), unhealthy)
        fmt.Println("\nInvestigate the unhealthy services and suggest fixes.")
    } else {
        fmt.Printf("Deploy %s to %s successful. All %d services healthy.\n",
            event.Version, event.Environment, len(event.Services))
        fmt.Println("\nRun a quick smoke test on the key API endpoints.")
    }
}
```

## Environment Variables Available

Every hook script gets rich context about the incoming request:

| Variable | Example | Description |
|----------|---------|-------------|
| `HOOK_NAME` | `jira-webhook` | Which hook triggered |
| `HOOK_METHOD` | `POST` | HTTP method |
| `HOOK_PATH` | `/webhook/jira` | Request path |
| `HOOK_QUERY` | `token=abc` | Query string |
| `HOOK_CONTENT_TYPE` | `application/json` | Content-Type header |
| `HOOK_SESSION` | `my-agent` | Session name |
| `HOOK_HEADER_X_GITHUB_EVENT` | `push` | Any header as `HOOK_HEADER_*` |

Use these to route different event types in a single script, validate signatures, or filter by source.

## Multi-Hook Agent Architecture

Run a single agent that reacts to multiple event sources:

```toml
[[hooks]]
name = "github"
bind = "0.0.0.0:9001"
script = "/opt/hooks/github.py"
timeout = 30

[[hooks]]
name = "jira"
bind = "0.0.0.0:9002"
script = "/opt/hooks/jira.py"
timeout = 30

[[hooks]]
name = "monitoring"
bind = "0.0.0.0:9003"
script = "/opt/hooks/alerts.rb"
timeout = 15

[[hooks]]
name = "slack"
bind = "0.0.0.0:9004"
script = "/opt/hooks/slack.js"
timeout = 10
```

```bash
octomind run --name ops-agent --daemon --format jsonl \
  --hook github \
  --hook jira \
  --hook monitoring \
  --hook slack
```

One AI agent, four event sources, each with its own script in its own language. The AI maintains context across all events -- it knows about the GitHub push when the monitoring alert fires 5 minutes later.

## Script Design Patterns

### Filter Early

```bash
# Exit non-zero to ignore events cheaply
[ "$HOOK_HEADER_X_GITHUB_EVENT" = "push" ] || exit 1
```

### Validate Signatures

```python
import hmac, hashlib, os, sys
secret = os.environ.get("GITHUB_WEBHOOK_SECRET", "")
signature = os.environ.get("HOOK_HEADER_X_HUB_SIGNATURE_256", "")
body = sys.stdin.buffer.read()
expected = "sha256=" + hmac.new(secret.encode(), body, hashlib.sha256).hexdigest()
if not hmac.compare_digest(signature, expected):
    sys.exit(1)
```

### Craft Targeted Prompts

The message you print to stdout IS the user message the AI processes. Be specific:

```bash
# Bad: dumps raw JSON
cat  # AI wastes tokens parsing irrelevant fields

# Good: extract what matters, tell AI what to do
echo "PR #${pr_number} ready for review: ${title}
Changed files: ${files}
Please review for security issues and respond with approve/reject."
```

### Timeout for Heavy Processing

```toml
[[hooks]]
name = "heavy-processor"
bind = "0.0.0.0:9876"
script = "/opt/hooks/process.py"
timeout = 120  # 2 minutes for complex payload processing
```

Max timeout is 3600 seconds (1 hour).

## Key Points

- Scripts can be written in **any language** -- Bash, Python, Node, Ruby, Go, Rust, anything executable
- You have **full control**: parse payloads, filter events, validate signatures, transform data
- **stdout** becomes the AI's input; **exit code** controls whether to inject or ignore
- **Rich environment**: HTTP method, path, headers, session name all available as env vars
- **Multiple hooks** on different ports feed into one agent session
- The AI maintains **cross-event context** -- it connects the dots between GitHub pushes, Jira tickets, and monitoring alerts
- Combine with **daemon mode** for persistent, always-on agents
