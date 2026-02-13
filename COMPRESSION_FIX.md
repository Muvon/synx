# Compression Auto-Continuation Fix

## Problem
When task compression triggered, the system would return to user input instead of automatically continuing the conversation loop. This broke the flow during active plan execution.

## Root Cause
The `check_and_compress_conversation()` function returned `Ok(())` without indicating whether compression occurred. The main loop had no way to know if it should check for continuation after compression freed up space.

## Solution

### 1. Return Value Change
Changed `check_and_compress_conversation()` to return `Result<bool>`:
- `Ok(true)` = compression performed
- `Ok(false)` = no compression needed

**File**: `src/session/chat/conversation_compression.rs`
- Updated function signature (line 437-440)
- Changed all return points to return boolean values

### 2. Auto-Continuation After Compression
Added logic in main loop to check for continuation after successful compression:

**File**: `src/session/chat/session/main_loop.rs` (lines 554-592)
```rust
// Capture compression result
let compression_occurred = match check_and_compress_conversation(...).await {
    Ok(compressed) => compressed,
    Err(e) => { log_debug!(...); false }
};

// CRITICAL FIX: After compression, check if continuation should trigger
if compression_occurred && has_active_plan() {
    if check_and_handle_continuation(...).await? {
        continue; // Skip to next iteration to process summary
    }
}
```

**Flow**:
1. Compression frees up space
2. System detects active plan
3. Checks if continuation threshold reached
4. Injects summary request automatically
5. Continues loop without user input

### 3. Enhanced Compression with File Context Support

Added smart file context injection (same as continuation system):

**Improvements**:
- AI can now request specific file contexts in compression summary
- Uses `<context>` tags with `filename:startline:endline` format
- Automatically expands file references to full content
- Reuses continuation's file parsing/rendering logic

**Updated prompt** (`ask_ai_decision_and_summary`):
- Instructs AI to include `<context>` tags if files needed
- Provides clear format requirements
- Maximum 5 file ranges for efficiency

**Updated processing** (`apply_compression`):
- Parses file contexts from AI summary
- Generates file content using XML renderer
- Injects into compressed entry automatically

**Benefits**:
- Compression preserves critical file context
- AI can continue work seamlessly after compression
- Reduces need for re-reading files
- Consistent with continuation system

## Testing

```bash
cargo check --message-format=short  # ✓ Passes
cargo clippy --all-features --all-targets -- -D warnings  # ✓ Passes
```

## Expected Behavior

**Before**:
```
[Compression triggers]
── task compression: 49 msgs → 19661 tokens saved (98.8% reduction) ──
[$0.40|15.8%] 〉  # ← WRONG: Returns to user input
```

**After**:
```
[Compression triggers]
── task compression: 49 msgs → 19661 tokens saved (98.8% reduction) ──
[Continuation check]
Token limit reached - requesting work summary...
[AI provides summary with file contexts]
📋 Session Summary (Token limit reached)
[Summary displayed]
🔄 Continuing Session
📁 Loaded context from 2 file(s)
   • src/main.rs (lines 100-200)
   • src/config.rs (lines 50-100)
🚀 Ready to continue...
[Automatically continues with next task]
```

## Key Files Modified

1. `src/session/chat/conversation_compression.rs`
   - Changed return type to `Result<bool>`
   - Enhanced prompt with file context support
   - Added file context parsing and injection
   - Updated formatting functions

2. `src/session/chat/session/main_loop.rs`
   - Added compression result capture
   - Added auto-continuation check after compression
   - Integrated with active plan detection

## Architecture

```
User Input
    ↓
[Check Compression] → Compression needed?
    ↓ YES                    ↓ NO
[Compress Messages]      [Continue]
    ↓
[Parse File Contexts]
    ↓
[Inject File Content]
    ↓
[Check Continuation] → Limit reached?
    ↓ YES                    ↓ NO
[Inject Summary Request] [Continue]
    ↓
[Loop: Process Summary]
    ↓
[Auto-Continue Work]
```

## Benefits

1. **Seamless Flow**: No interruption during active work
2. **Smart Context**: File contexts preserved through compression
3. **Cost Efficient**: Compression + continuation work together
4. **Consistent UX**: Same file context system as continuation
5. **Zero User Action**: Fully automatic when plan active
