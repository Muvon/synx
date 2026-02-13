# Session Recovery Fix

## Problem

When resuming a session, token counts, costs, and layer statistics were being reset to zero, even though the data was saved correctly. The `/info` command would show:

```
Total tokens: 0
Breakdown: 0 processed, 0 output, 0 cached
Total cost: $0.00000
Messages: 3

No layer-specific statistics available.
```

## Root Cause

The session loading logic had a timestamp-based issue:

1. **STATS entries** are written after each assistant response during the session
2. **SUMMARY entries** are written when you explicitly save (`/save`) or when the session ends
3. During loading, STATS entries were **overwriting** SUMMARY data regardless of timestamps
4. Old STATS entries (from during the session) would overwrite the final SUMMARY (from save/exit)
5. **STATS entries don't include `layer_stats`**, so this data was being lost

## Solution

Modified `load_session()` in `src/session/mod.rs` to:

1. **Track the last SUMMARY timestamp** when loading
2. **Only apply STATS entries that are NEWER than the last SUMMARY**
3. This ensures SUMMARY (written on save/exit) is the **source of truth**
4. STATS entries only provide incremental updates during an active session

### Code Changes

```rust
// Before: STATS always overwrote session_info
"STATS" => {
    if let Some(info) = &mut session_info {
        // Always applied, even if older than SUMMARY
        info.total_cost = ...;
    }
}

// After: STATS only applied if newer than last SUMMARY
"STATS" => {
    let stats_timestamp = json_value.get("timestamp")...;

    // Only apply if newer than last SUMMARY
    if stats_timestamp > last_summary_timestamp {
        if let Some(info) = &mut session_info {
            info.total_cost = ...;
        }
    }
}
```

## What's Preserved Now

After resuming a session, the following are correctly restored:

✅ **Token counts**: `input_tokens`, `output_tokens`, `cached_tokens`
✅ **Costs**: `total_cost`
✅ **Layer statistics**: `layer_stats` (per-layer token/cost breakdown)
✅ **Tool calls**: `tool_calls` count
✅ **Timing**: `total_api_time_ms`, `total_tool_time_ms`, `total_layer_time_ms`
✅ **Model**: Session model (including changes via `/model` command)
✅ **Messages**: All conversation messages
✅ **Runtime state**: Cache flags, continuation state, compression hints

## Testing

Added comprehensive tests in `src/session/mod.rs`:

1. **`test_session_loading_preserves_stats_from_summary`**
   - Verifies SUMMARY is source of truth
   - Ensures old STATS don't overwrite fresh SUMMARY data
   - Validates layer_stats preservation

2. **`test_session_loading_restores_model_from_command`**
   - Tests model restoration when changed via `/model` command
   - Verifies both COMMAND and SUMMARY are used correctly

3. **`test_session_loading_model_without_command`**
   - Tests model restoration from SUMMARY alone
   - Ensures model is preserved when no `/model` command was used

All tests pass:
```
test session::tests::test_session_loading_preserves_stats_from_summary ... ok
test session::tests::test_session_loading_restores_model_from_command ... ok
test session::tests::test_session_loading_model_without_command ... ok
```

## Verification

To verify the fix works:

1. Start a session: `octomind session`
2. Have a conversation (use some tokens)
3. Check stats: `/info` (should show tokens/costs)
4. Save and exit: `/save` then exit
5. Resume session: `octomind session -r`
6. Check stats again: `/info` (should show same tokens/costs)

## Technical Details

### Session File Format

Session files (`.jsonl`) contain:

- **SUMMARY entries**: Complete session state (written on save/exit)
  - Contains: `session_info` with all fields including `layer_stats`
  - Timestamp: When the save occurred

- **STATS entries**: Incremental updates (written after each response)
  - Contains: Token counts, costs, timing (but NOT `layer_stats`)
  - Timestamp: When the response completed

- **COMMAND entries**: User commands like `/model`, `/role`
  - Used to restore runtime state changes

### Loading Priority

1. **SUMMARY** = Source of truth (complete state at save time)
2. **STATS** = Incremental updates (only if newer than last SUMMARY)
3. **COMMAND** = Runtime state overrides (e.g., model changes)

This ensures that:
- Saved state is always preserved
- Incremental updates during active session work correctly
- Runtime changes (like `/model`) are properly restored
