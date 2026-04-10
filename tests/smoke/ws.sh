#!/usr/bin/env bash
# WebSocket server smoke test — persistent connection, same scenarios as ACP test
# Usage: ./test_ws.sh [--tag assistant:general] [--bin /path/to/octomind] [--port 9199]

TAG="assistant:general"
BIN="$(dirname "$0")/../../target/debug/octomind"
PORT=9199
SCHEDULE_DELAY=5

while [[ $# -gt 0 ]]; do
  case $1 in
    --tag)  TAG="$2";  shift 2 ;;
    --bin)  BIN="$2";  shift 2 ;;
    --port) PORT="$2"; shift 2 ;;
    *) shift ;;
  esac
done

if [[ ! -x "$BIN" ]]; then
  echo "Binary not found: $BIN"
  echo "Run 'cargo build' first, or pass --bin /path/to/octomind"
  exit 1
fi

if ! command -v websocat &>/dev/null; then
  echo "websocat not found. Install with: brew install websocat"
  exit 1
fi

PASS=0
FAIL=0
WS_URL="ws://127.0.0.1:$PORT"

GREEN='\033[0;32m'; RED='\033[0;31m'; YELLOW='\033[1;33m'; NC='\033[0m'
ok()   { echo -e "${GREEN}PASS${NC} $1"; ((PASS++)); }
fail() { echo -e "${RED}FAIL${NC} $1"; ((FAIL++)); }
info() { echo -e "${YELLOW}>>>${NC} $1"; }

echo ""
echo "=== Octomind WebSocket smoke test (tag: $TAG, port: $PORT) ==="
echo ""

# Start server
"$BIN" server "$TAG" --port "$PORT" > /dev/null 2>&1 &
SERVER_PID=$!

# Persistent websocat connection via named pipe
FIFO=$(mktemp -u)
TMPOUT=$(mktemp)
mkfifo "$FIFO"

cleanup() {
  exec 3>&- 2>/dev/null
  kill $WS_PID 2>/dev/null || true
  kill $SERVER_PID 2>/dev/null || true
  rm -f "$FIFO" "$TMPOUT"
}
trap cleanup EXIT

# Wait for server to listen
info "Starting server on $WS_URL ..."
elapsed=0
while [ $elapsed -lt 75 ]; do
  if lsof -i ":$PORT" -sTCP:LISTEN >/dev/null 2>&1; then break; fi
  sleep 0.2; ((elapsed++))
done
if [ $elapsed -ge 75 ]; then
  fail "server did not start within 15s"
  exit 1
fi
ok "server started"

# Open persistent WebSocket connection
websocat -n "$WS_URL" < "$FIFO" > "$TMPOUT" 2>/dev/null &
WS_PID=$!
exec 3>"$FIFO"
sleep 0.5

send() { echo "$1" >&3; }

wait_for() {
  local pattern="$1" timeout_s="$2" elapsed=0
  while [ $elapsed -lt $((timeout_s * 5)) ]; do
    grep -q "$pattern" "$TMPOUT" 2>/dev/null && return 0
    sleep 0.2; ((elapsed++))
  done
  return 1
}

# Count occurrences of pattern in output after a given line
count_after() {
  local start_line="$1" pattern="$2"
  local n
  n=$(tail -n +"$start_line" "$TMPOUT" 2>/dev/null | grep -c "$pattern" 2>/dev/null) || true
  echo "${n:-0}" | tr -d '[:space:]'
}

# ── 1. Welcome message ───────────────────────────────────────────────────────
info "Test 1: welcome message"
if wait_for '"status"' 5; then
  ok "welcome message received"
else
  fail "no welcome message"
  exit 1
fi

# ── 2. Create session ────────────────────────────────────────────────────────
info "Test 2: create session"
send '{"type":"session"}'
if wait_for 'Session created' 15; then
  SESSION_ID=$(grep 'Session created' "$TMPOUT" | jq -r '.session_id // empty' 2>/dev/null | head -1)
  if [ -n "$SESSION_ID" ]; then
    ok "session created — sessionId=$SESSION_ID"
  else
    fail "session created but no session_id in payload"
    exit 1
  fi
else
  fail "session creation timeout"
  exit 1
fi

# ── 3. Send message and get AI response ──────────────────────────────────────
info "Test 3: send message"
BEFORE=$(wc -l < "$TMPOUT" | tr -d ' ')
send "{\"type\":\"message\",\"session_id\":\"$SESSION_ID\",\"content\":\"Reply with exactly one word: hello\"}"

# Wait for cost message (marks end of AI turn)
elapsed=0
while [ $elapsed -lt 300 ]; do
  if [ "$(count_after "$BEFORE" '"type":"cost"')" -gt 0 ]; then break; fi
  sleep 0.2; ((elapsed++))
done

if [ $elapsed -lt 300 ]; then
  ASSISTANT_COUNT=$(count_after "$BEFORE" '"type":"assistant"')
  if [ "$ASSISTANT_COUNT" -gt 0 ]; then
    CONTENT=$(tail -n +"$BEFORE" "$TMPOUT" | grep '"type":"assistant"' | jq -r '.content // empty' 2>/dev/null | tr -d '\n')
    ok "AI response received: \"$CONTENT\""
  else
    fail "cost received but no assistant message"
  fi
else
  fail "message response timeout (60s)"
fi

# ── 4. Command (/info) ───────────────────────────────────────────────────────
info "Test 4: command /info"
BEFORE=$(wc -l < "$TMPOUT" | tr -d ' ')
send "{\"type\":\"command\",\"session_id\":\"$SESSION_ID\",\"command\":\"info\"}"

elapsed=0
while [ $elapsed -lt 50 ]; do
  if [ "$(count_after "$BEFORE" '"type":"status"')" -gt 0 ]; then break; fi
  sleep 0.2; ((elapsed++))
done

if [ $elapsed -lt 50 ]; then
  ok "command /info executed"
else
  fail "command /info timeout"
fi

# ── 5. Schedule auto-fire ────────────────────────────────────────────────────
info "Test 5: schedule auto-fire (delay: ${SCHEDULE_DELAY}s)"
BEFORE=$(wc -l < "$TMPOUT" | tr -d ' ')
send "{\"type\":\"message\",\"session_id\":\"$SESSION_ID\",\"content\":\"Use the schedule tool to add a schedule with when='in ${SCHEDULE_DELAY}s' and message='SCHEDULE_FIRED_OK'. Only call the tool, no explanation needed.\"}"

# Wait for the schedule prompt to complete (cost = end of turn)
elapsed=0
while [ $elapsed -lt 450 ]; do  # 90s for slow models
  if [ "$(count_after "$BEFORE" '"type":"cost"')" -gt 0 ]; then break; fi
  sleep 0.2; ((elapsed++))
done

if [ $elapsed -ge 450 ]; then
  fail "schedule prompt timeout (90s)"
else
  SCHED_TOOL=$(count_after "$BEFORE" '"schedule"')
  if [ "$SCHED_TOOL" -gt 0 ]; then
    ok "schedule created (tool called)"
  else
    fail "schedule tool not called"
  fi
fi

# Now wait for the auto-response — NO more messages sent
AFTER=$(wc -l < "$TMPOUT" | tr -d ' ')
info "Waiting for automatic response after schedule fires (up to 45s)..."

NEW_ASSISTANT=0
elapsed=0
while [ $elapsed -lt 225 ]; do
  NEW_ASSISTANT=$(count_after "$AFTER" '"type":"assistant"')
  if [ "$NEW_ASSISTANT" -gt 0 ]; then break; fi
  sleep 0.2; ((elapsed++))
done

if [ "$NEW_ASSISTANT" -gt 0 ]; then
  CONTENT=$(tail -n +"$AFTER" "$TMPOUT" | grep '"type":"assistant"' | jq -r '.content // empty' 2>/dev/null | tr -d '\n')
  ok "auto-response received: \"$(echo "$CONTENT" | head -c 100)\""
else
  fail "no auto-response within 45s — schedule monitor is broken"
  echo "--- last 10 lines ---"
  tail -10 "$TMPOUT" | while IFS= read -r line; do echo "$line" | jq '.' 2>/dev/null || echo "$line"; done
  echo "--- end ---"
fi

# ── Shutdown ──────────────────────────────────────────────────────────────────
echo ""
echo "=== Results: ${PASS} passed, ${FAIL} failed ==="
echo ""
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
