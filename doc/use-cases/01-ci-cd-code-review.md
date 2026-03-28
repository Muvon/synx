# Use Case: Automated Code Review in CI/CD

Run Octomind as part of your CI/CD pipeline to automatically review pull requests, check for security issues, or enforce coding standards.

## The Problem

Manual code review is slow. You want AI to catch common issues -- security vulnerabilities, performance problems, style violations -- before human reviewers even look at the PR.

## Solution

Use non-interactive mode with structured output to get machine-readable results.

### Basic: Review from Stdin

```bash
# Feed a diff to Octomind and get a review
git diff main..HEAD | octomind run developer \
  "Review this diff for security issues, performance problems, and bugs. Be specific about file and line numbers." \
  --format plain
```

### Structured: JSON Output for Pipeline Decisions

Define a schema (`review-schema.json`):

```json
{
  "type": "object",
  "properties": {
    "summary": { "type": "string" },
    "issues": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "file": { "type": "string" },
          "line": { "type": "integer" },
          "severity": { "type": "string", "enum": ["info", "warning", "error"] },
          "description": { "type": "string" }
        },
        "required": ["file", "severity", "description"]
      }
    },
    "approval": { "type": "string", "enum": ["approve", "request_changes", "comment"] }
  },
  "required": ["summary", "issues", "approval"],
  "additionalProperties": false
}
```

Run in CI:

```bash
#!/bin/bash
# ci-review.sh

diff=$(git diff main..HEAD)
result=$(echo "Review this diff for issues:\n\n$diff" | \
  octomind run developer \
  --schema review-schema.json \
  --model openai:gpt-4o \
  --format plain)

approval=$(echo "$result" | jq -r '.approval')
errors=$(echo "$result" | jq '[.issues[] | select(.severity == "error")] | length')

echo "Review: $approval ($errors errors)"

if [ "$approval" = "request_changes" ] || [ "$errors" -gt 0 ]; then
  echo "$result" | jq '.issues[]'
  exit 1
fi
```

### GitHub Actions Integration

```yaml
# .github/workflows/ai-review.yml
name: AI Code Review
on: [pull_request]

jobs:
  review:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Install Octomind
        run: curl -fsSL https://raw.githubusercontent.com/muvon/octomind/master/install.sh | bash

      - name: Run AI Review
        env:
          OPENROUTER_API_KEY: ${{ secrets.OPENROUTER_API_KEY }}
        run: |
          diff=$(git diff origin/main..HEAD)
          result=$(echo "$diff" | octomind run developer \
            "Review for security, performance, bugs" \
            --schema review-schema.json \
            --format plain)

          echo "$result" >> $GITHUB_STEP_SUMMARY
```

## Cost Control

Set spending limits to prevent runaway costs:

```toml
max_request_spending_threshold = 0.50  # Max $0.50 per review
```

Or use a cheaper model for initial triage:

```bash
octomind run -m openrouter:openai/gpt-4o-mini "Quick check for obvious bugs"
```

## Key Points

- `--format plain` or `--format jsonl` for non-interactive output
- `--schema` for machine-parseable JSON responses
- `--model` to balance cost vs quality per pipeline step
- Octomind has full tool access -- it can read files, not just the diff you pipe in
