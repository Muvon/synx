# Compression Cache Cost Calculation Fix

## Problem Identified

The compression cost calculation was using **lifetime accumulated `cached_tokens`** instead of **current context cache state**, causing incorrect cost estimates.

### Bug Details

**Before (WRONG):**
```rust
let current_cached = session.session.info.cached_tokens as f64;  // ← 977,777 (LIFETIME)
let current_uncached = (total_tokens - current_cached).max(0.0); // ← 52,266 - 977,777 = 0
```

**Issue:**
- `session.info.cached_tokens` accumulates across ALL API calls (lifetime counter)
- `total_tokens` is current context size (52K)
- Subtracting lifetime (977K) from current (52K) = nonsense!

**Example from logs:**
```
Current: 52266 tokens (cached: 977777, uncached: 0)  ← IMPOSSIBLE!
```

## Root Cause

`session.info.cached_tokens` is a **cumulative counter**:
- Call 1: 50K cached → `cached_tokens` = 50K
- Call 2: 50K cached → `cached_tokens` = 100K (cumulative!)
- Call 3: 50K cached → `cached_tokens` = 150K (cumulative!)

It does NOT represent "how many tokens in current context are cached".

## Solution

### Key Insight
When compression API call happens:
- **Session context is ALREADY cached** from previous API calls
- **Only the NEW decision prompt is uncached**
- This applies ONLY when `decision_model == session_model`

### Fix Implementation

**After (CORRECT):**
```rust
// Estimate decision prompt size (the NEW content)
let decision_prompt_tokens = estimate_tokens("...decision prompt...") as f64;

// Check if decision model can reuse session cache
let same_model = decision_model == session_model;

// Calculate compression cost
let compression_cost = if same_model {
    // Same model: session context is cached, only prompt is new
    decision_pricing.calculate_cost(
        decision_prompt_tokens as u64,  // Only new prompt is uncached
        0,                               // No cache write
        (total_tokens - decision_prompt_tokens) as u64,  // Rest is cached
        compressed_tokens as u64,        // Output tokens
    )
} else {
    // Different model: NO cache reuse, everything is uncached
    decision_pricing.calculate_cost(
        total_tokens as u64,             // ALL tokens uncached
        0,                                // No cache write
        0,                                // NO cache
        compressed_tokens as u64,        // Output tokens
    )
};
```

## What This Fixes

### ✅ Same Model (decision_model == session_model)
**Before:** Paid for 0 uncached + 977K cached (wrong!)
**After:** Pays for ~500 uncached (prompt) + 52K cached (correct!)

**Impact:** Compression cost now **accurately reflects cache reuse**

### ✅ Different Model (decision_model ≠ session_model)
**Before:** Paid for 0 uncached + 977K cached (wrong!)
**After:** Pays for 52K uncached + 0 cached (correct!)

**Impact:** Compression cost now **accurately reflects NO cache reuse**

## Expected Behavior After Fix

### Scenario A: Same Model
```
Decision model: anthropic:claude-sonnet-4-5
Session model: anthropic:claude-sonnet-4-5
Models match: YES (cache reuse: YES)
Current: 52266 tokens (decision prompt: ~500 tokens)
Compression cost: $X (using anthropic:claude-sonnet-4-5, 500 uncached, 51766 cached)
```

### Scenario B: Different Model
```
Decision model: anthropic:claude-haiku
Session model: anthropic:claude-sonnet-4-5
Models match: NO (cache reuse: NO)
Current: 52266 tokens (decision prompt: ~500 tokens)
Compression cost: $Y (using anthropic:claude-haiku, 52266 uncached, 0 cached)
```

## Testing

Run a session and trigger compression to see the new log output:
```bash
./target/debug/octomind session
# ... work until compression triggers ...
```

Look for:
- `Models match: YES/NO (cache reuse: YES/NO)`
- `Compression cost: $X (using MODEL, N uncached, M cached)`
- Verify numbers make sense (uncached + cached ≈ total_tokens)

## Files Changed

- `src/session/chat/conversation_compression.rs`
  - Fixed `calculate_compression_net_benefit()` function
  - Added `same_model` check
  - Corrected cache state calculation
  - Updated debug logging

## Verification

✅ `cargo check` passes
✅ `cargo clippy` passes with no warnings
✅ Logic verified for both same/different model scenarios
✅ Debug logging enhanced for transparency
