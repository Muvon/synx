# Plan Compression Context Loss Bug Fix

**Date:** 2026-02-12
**Severity:** CRITICAL
**Impact:** AI loses ALL context after 2-3 plan tasks, hallucinates completely

---

## **The Bug**

### **Symptom**
```
── task compression: 19 msgs → 47313 tokens saved (99.5% reduction) ──
```

After plan task completion, AI loses ALL memory of previous work:
- Only sees compressed summary of last few tool calls
- Hallucinates because it has no context
- Session becomes unusable after 2-3 tasks

### **Root Cause**

`start_index` was set at **FIRST tool execution** instead of **task start**, causing massive context loss.

**BROKEN Flow:**
```
1. plan(next) → Task 2 starts
2. User works → Messages 10-50 added (user requests, AI responses)
3. FIRST tool call → start_index = 49 (WRONG!)
4. Tools execute → Messages 51-52 added
5. plan(next) → end_index = 52
6. Compression: Remove messages 50-52, insert summary
7. RESULT: Messages 10-49 ORPHANED (not compressed, not preserved)
```

**CORRECT Flow:**
```
1. plan(next) → Task 2 starts → start_index = 9 (last message before task)
2. User works → Messages 10-50 added
3. Tools execute → Messages 51-52 added
4. plan(next) → end_index = 52
5. Compression: Remove messages 10-52, insert summary
6. RESULT: All task work compressed properly
```

---

## **The Fix**

### **Changed Files**

1. **`src/mcp/dev/plan/core.rs`**
   - `handle_next_command()`: Clear `start_index` after requesting compression
   - This signals that the NEXT task should set a new `start_index`

2. **`src/session/chat/response/tool_result_processor.rs`**
   - Set `start_index` AFTER plan tool execution (not before)
   - Set it when `start_index` is None and plan is active
   - This captures the message count where the NEXT task work will begin

3. **`src/session/chat/response.rs`**
   - **REMOVED** broken logic that set `start_index` on first tool execution

4. **`tests/compression_tests.rs`**
   - Added missing `reasoning_tokens: 0` field to fix test compilation

### **Key Changes**

**Before (BROKEN):**
```rust
// response.rs - WRONG: Sets start_index on first tool execution
if crate::mcp::dev::plan::get_current_task_start_index().is_none() {
    let start_index = params.chat_session.get_message_count().saturating_sub(1);
    crate::mcp::dev::plan::set_current_task_start_index(start_index);
}
```

**After (FIXED):**
```rust
// tool_result_processor.rs - CORRECT: Sets start_index after plan tool returns
if plan_tool_executed {
    if crate::mcp::dev::plan::core::get_current_task_start_index().is_none()
        && crate::mcp::dev::plan::core::has_active_plan()
    {
        let start_index = chat_session.get_message_count();
        crate::mcp::dev::plan::set_current_task_start_index(start_index);
    }
}

// core.rs - Clear start_index after plan(next) to trigger new start_index for next task
{
    let mut start_index = CURRENT_TASK_START_INDEX.lock().unwrap();
    *start_index = None;
}
```

---

## **How It Works Now**

### **Task Lifecycle**

1. **`plan(start)`** → Creates plan, returns
2. **Tool result processor** → Sets `start_index = current_message_count` (where Task 1 work begins)
3. **User works on Task 1** → Messages added
4. **`plan(next)`** → Requests compression, clears `start_index`
5. **Tool result processor** →
   - Sets compression range: `start_index` (from step 2) to `end_index` (current)
   - Compresses Task 1 work
   - Sets NEW `start_index = current_message_count` (where Task 2 work begins)
6. **Repeat for Task 2, 3, etc.**

### **Compression Range Calculation**

```rust
// When plan(next) is called:
start_index = <set when previous task started>  // e.g., 10
end_index = chat_session.get_message_count() - 1  // e.g., 52

// Compression removes messages (start_index+1)..=end_index
// In this example: messages 11-52 are compressed
// Message 10 (the plan(next) call) stays as boundary marker
```

---

## **Multi-Level Compression Safety**

This fix **DOES NOT** break multi-level compression (conversation, continuation):

1. **Conversation compression** (`src/session/chat/conversation_compression.rs`)
   - Uses its own logic: `find_compression_range()`
   - Preserves last 4 turns (2 exchanges)
   - Independent of plan compression

2. **Continuation** (`src/session/chat/continuation/`)
   - Triggers when context exceeds threshold
   - Uses `check_and_handle_continuation()`
   - Independent of plan compression

3. **Plan compression** (this fix)
   - Only triggers when `plan(next)` is called
   - Uses `start_index` tracking
   - Isolated to plan workflow

**All three systems are independent and work in parallel.**

---

## **Testing**

```bash
# Verify syntax
cargo check --message-format=short

# Verify quality
cargo clippy --all-features --all-targets -- -D warnings

# Run compression tests
cargo test compression_tests

# Manual test: Create plan with 3 tasks, verify context preserved
octomind session
> plan(start, "Test Plan", tasks=[...])
> <work on task 1>
> plan(next, "Task 1 done")
> <verify AI remembers task 1 context>
> <work on task 2>
> plan(next, "Task 2 done")
> <verify AI remembers task 1 AND task 2 context>
```

---

## **Impact**

✅ **FIXED:** AI now retains full context across all plan tasks
✅ **FIXED:** Compression removes correct message range (entire task work)
✅ **FIXED:** No more hallucinations after 2-3 tasks
✅ **SAFE:** Multi-level compression (conversation, continuation) unaffected

---

## **Related Issues**

- Original bug report: "huge bug in compression between tasks"
- Symptom: "99.5% reduction" showing nearly all tokens removed
- Evidence: 19 messages removed but 47313 tokens saved (orphaned context)

---

**Status:** ✅ FIXED and TESTED
