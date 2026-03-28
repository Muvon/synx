# Use Case: Event-Driven Agent with Webhooks

Run Octomind as a persistent daemon that reacts to external events -- GitHub pushes, Slack messages, monitoring alerts.

## The Problem

You want an AI agent that monitors events and acts autonomously: reviewing pushed code, responding to incidents, or summarizing deployments. Polling is wasteful; you need event-driven reactions.

## Solution

Combine daemon mode with webhook hooks.

### Architecture

```
GitHub/Slack/PagerDuty
    |
    | HTTP POST
    v
Webhook Hook (HTTP listener on port 9876)
    |
    | stdin: raw HTTP body
    v
Hook Script (process-event.sh)
    |
    | stdout: message for AI
    v
Daemon Session Inbox
    |
    v
AI processes event, takes action
```

### Step 1: Write a Hook Script

```bash
#!/bin/bash
# /opt/hooks/github-push.sh

payload=$(cat)

repo=$(echo "$payload" | jq -r '.repository.full_name')
branch=$(echo "$payload" | jq -r '.ref' | sed 's|refs/heads/||')
pusher=$(echo "$payload" | jq -r '.pusher.name')
commits=$(echo "$payload" | jq -r '.commits | length')

# Only react to main branch
if [ "$branch" != "main" ]; then
  exit 1  # Non-zero = ignore
fi

# Files changed
files=$(echo "$payload" | jq -r '.commits[].modified[]' | sort -u | head -20)

cat <<EOF
Push to $repo/$branch by $pusher ($commits commits).

Changed files:
$files

Please:
1. Review the changes for potential issues
2. Check if any tests might be affected
3. Summarize the changes in 2-3 sentences
EOF
```

Make it executable:
```bash
chmod +x /opt/hooks/github-push.sh
```

### Step 2: Configure the Hook

```toml
[[hooks]]
name = "github-push"
bind = "0.0.0.0:9876"
script = "/opt/hooks/github-push.sh"
timeout = 30
```

### Step 3: Start the Daemon

```bash
octomind run --name code-monitor --daemon --format jsonl --hook github-push
```

### Step 4: Point GitHub Webhook

In your GitHub repo settings:
- Webhook URL: `http://your-server:9876/`
- Content type: `application/json`
- Events: "Push"

Now every push to `main` triggers an AI review.

### Multiple Hooks

Monitor different event sources simultaneously:

```toml
[[hooks]]
name = "github-push"
bind = "0.0.0.0:9876"
script = "/opt/hooks/github-push.sh"
timeout = 30

[[hooks]]
name = "slack-mention"
bind = "0.0.0.0:9877"
script = "/opt/hooks/slack-mention.sh"
timeout = 15

[[hooks]]
name = "pagerduty-alert"
bind = "0.0.0.0:9878"
script = "/opt/hooks/pagerduty-alert.sh"
timeout = 30
```

```bash
octomind run --name ops-agent --daemon --format jsonl \
  --hook github-push \
  --hook slack-mention \
  --hook pagerduty-alert
```

### Injecting Messages Manually

Besides webhooks, you can inject messages directly:

```bash
echo "Summarize what happened in the last hour" | octomind send --name ops-agent
```

## Hook Script Environment

Your script receives rich context via environment variables:

| Variable | Example |
|----------|---------|
| `HOOK_NAME` | `github-push` |
| `HOOK_METHOD` | `POST` |
| `HOOK_PATH` | `/webhook` |
| `HOOK_CONTENT_TYPE` | `application/json` |
| `HOOK_SESSION` | `code-monitor` |
| `HOOK_HEADER_X_GITHUB_EVENT` | `push` |

Use these to route different event types in a single script:

```bash
#!/bin/bash
event="$HOOK_HEADER_X_GITHUB_EVENT"

case "$event" in
  push)
    echo "Code pushed: $(cat | jq -r '.commits | length') commits"
    ;;
  pull_request)
    echo "PR $(cat | jq -r '.action'): $(cat | jq -r '.pull_request.title')"
    ;;
  *)
    exit 1  # Ignore unknown events
    ;;
esac
```

## Key Points

- `--daemon` keeps the session alive between events
- `--hook` activates webhook listeners (multiple allowed)
- Scripts exit 0 = inject stdout as user message; non-zero = ignore
- `octomind send --name X` for manual message injection
- JSONL output is parseable by downstream tools
- The AI has full tool access -- it can read the actual files, not just the webhook payload
