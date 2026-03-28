# Use Case: Web Dashboard with AI Assistant

Embed Octomind into a web application as a real-time AI coding assistant using the WebSocket server.

## The Problem

Your team wants an AI assistant accessible from a web dashboard -- no terminal required. Developers should be able to ask questions about the codebase, request code reviews, and get help directly from a browser.

## Solution

Run the WebSocket server and connect from your web frontend.

### Step 1: Start the Server

```bash
octomind server developer --host 127.0.0.1 --port 8080
```

For production, bind to `0.0.0.0` behind a reverse proxy with TLS:

```bash
octomind server developer --host 0.0.0.0 --port 8080
```

### Step 2: Connect from JavaScript

```typescript
class OctomindClient {
  private ws: WebSocket;
  private sessionId: string;
  private handlers: {
    onMessage: (content: string) => void;
    onToolUse: (tool: string, params: any) => void;
    onCost: (cost: number) => void;
    onError: (error: string) => void;
  };

  constructor(url: string, sessionId: string, handlers: typeof this.handlers) {
    this.sessionId = sessionId;
    this.handlers = handlers;
    this.ws = new WebSocket(url);

    this.ws.onopen = () => {
      // Initialize session
      this.ws.send(JSON.stringify({
        type: 'session',
        session_id: this.sessionId
      }));
    };

    this.ws.onmessage = (event) => {
      const msg = JSON.parse(event.data);
      switch (msg.type) {
        case 'assistant':
          this.handlers.onMessage(msg.content);
          break;
        case 'tool_use':
          this.handlers.onToolUse(msg.tool_name, msg.parameters);
          break;
        case 'cost':
          this.handlers.onCost(msg.session_cost);
          break;
        case 'error':
          this.handlers.onError(msg.content);
          break;
      }
    };
  }

  send(message: string) {
    this.ws.send(JSON.stringify({
      type: 'message',
      session_id: this.sessionId,
      content: message
    }));
  }

  command(cmd: string) {
    this.ws.send(JSON.stringify({
      type: 'command',
      session_id: this.sessionId,
      command: cmd
    }));
  }
}

// Usage
const ai = new OctomindClient('ws://localhost:8080', 'dev-session', {
  onMessage: (content) => appendToChat(content),
  onToolUse: (tool, params) => showToolActivity(tool),
  onCost: (cost) => updateCostDisplay(cost),
  onError: (error) => showError(error),
});

ai.send('Explain how authentication works in this project');
ai.command('/info');  // Get session stats
```

### Step 3: Production Setup with nginx

```nginx
# /etc/nginx/sites-available/octomind
server {
    listen 443 ssl;
    server_name ai.yourcompany.com;

    ssl_certificate /etc/letsencrypt/live/ai.yourcompany.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/ai.yourcompany.com/privkey.pem;

    # WebSocket proxy
    location /ws {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
    }

    # Static frontend
    location / {
        root /var/www/dashboard;
        try_files $uri /index.html;
    }
}
```

### Python Client

```python
import asyncio
import json
import websockets

async def ask_octomind(question: str) -> str:
    async with websockets.connect('ws://127.0.0.1:8080') as ws:
        await ws.send(json.dumps({
            'type': 'session',
            'session_id': 'api-session'
        }))

        await ws.send(json.dumps({
            'type': 'message',
            'session_id': 'api-session',
            'content': question
        }))

        response_parts = []
        async for message in ws:
            msg = json.loads(message)
            if msg['type'] == 'assistant':
                response_parts.append(msg['content'])
            elif msg['type'] == 'status' and 'complete' in msg.get('content', ''):
                break

        return ''.join(response_parts)

# Usage
answer = asyncio.run(ask_octomind("What does the login function do?"))
```

## Protocol Messages

| Direction | Type | Purpose |
|-----------|------|---------|
| Client -> Server | `session` | Create/resume session |
| Client -> Server | `message` | Send user input |
| Client -> Server | `command` | Execute session command |
| Server -> Client | `assistant` | AI response text |
| Server -> Client | `thinking` | Extended thinking (if model supports) |
| Server -> Client | `tool_use` | Tool being called |
| Server -> Client | `tool_result` | Tool execution result |
| Server -> Client | `cost` | Token usage and cost |
| Server -> Client | `status` | Progress updates |
| Server -> Client | `error` | Error messages |

## Multi-Session Support

Each `session_id` is independent. Multiple users can have concurrent sessions:

```typescript
const alice = new OctomindClient(url, 'alice-session', handlers);
const bob = new OctomindClient(url, 'bob-session', handlers);

alice.send('Review the auth module');
bob.send('Help me write tests for the API');
// Both sessions run independently
```

## Key Points

- The WebSocket server provides the same capabilities as the CLI
- Sessions are stateful -- context persists across messages
- Tool execution (file reading, shell commands) is streamed in real-time
- Use a reverse proxy with TLS for production
- Never expose the server directly to the internet without authentication
- Cost tracking is per-session via `cost` messages
