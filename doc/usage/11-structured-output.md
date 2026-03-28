# Structured Output

Octomind supports enforcing structured JSON output via the `--schema` flag. Useful for automation, CI/CD pipelines, and machine-readable responses.

## How It Works

When `--schema` is provided:
1. Schema is loaded and validated as JSON at startup
2. Schema is passed to the provider using its native structured output API
3. AI is constrained to respond with JSON matching the schema
4. Response is returned as raw JSON (markdown rendering disabled)
5. Strict mode is always enabled: non-conforming responses are rejected

## Usage

### Define a Schema

```json
{
  "type": "object",
  "properties": {
    "summary": { "type": "string" },
    "issues": {
      "type": "array",
      "items": { "type": "string" }
    },
    "severity": { "type": "string", "enum": ["low", "medium", "high"] }
  },
  "required": ["summary", "issues", "severity"],
  "additionalProperties": false
}
```

### Non-Interactive Mode

```bash
octomind run developer "Analyze the codebase and list all TODO items" \
  --schema todos.json \
  --format jsonl
```

### Interactive Session

```bash
octomind run assistant --schema schema.json
```

### Pipeline Integration

```bash
result=$(octomind run developer "Summarize recent changes" \
  --schema analysis.json \
  --model openai:gpt-4o)

severity=$(echo "$result" | jq -r '.severity')
if [ "$severity" = "high" ]; then
  echo "High severity issues found"
  exit 1
fi
```

## Provider Compatibility

Not all providers support structured output. Octomind fails fast with a clear error if unsupported:

```
Provider 'cloudflare' does not support structured output for model 'llama-3.1-8b-instruct'.
Remove --schema or use a compatible provider.
```

| Provider | Support | Notes |
|----------|---------|-------|
| OpenAI | Yes | `gpt-4o`, `gpt-4o-mini`, recent models |
| Anthropic | Yes | Tool-based JSON mode |
| OpenRouter | Varies | Depends on underlying model |
| Google | No | |
| Amazon | No | |
| Cloudflare | No | |
| DeepSeek | No | |

## Notes

- Schema is used only for the top-level session response
- Layers and compression always use `schema: None`
- When structured output is present, it takes precedence over `content` for display
- The `structured_output` field in provider response carries the parsed JSON value
