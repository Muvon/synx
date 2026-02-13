# Fix Non-Interactive Mode Output

## Problem
The `octomind run` command (non-interactive mode) displays ANSI escape codes and formatted output instead of plain text.

## Root Causes
1. **Markdown rendering enabled** - `print_assistant_response()` uses markdown with ANSI codes
2. **Colored tool output** - Tool display functions use `colored` crate
3. **Missing flag propagation** - `is_interactive` flag not passed to display functions

## Solution Steps

### 1. Modify `print_assistant_response()` to accept `is_interactive`
**File**: `src/session/chat/assistant_output.rs`

```rust
pub fn print_assistant_response(
    content: &str,
    config: &Config,
    _role: &str,
    thinking: &Option<ThinkingBlock>,
    is_interactive: bool,  // NEW PARAMETER
) {
    let content_to_display = get_content_to_display(content, thinking);

    if content_to_display.is_empty() {
        return;
    }

    // Only use markdown rendering in interactive mode
    if is_interactive && config.enable_markdown_rendering && is_markdown_content(&content_to_display) {
        // Use markdown rendering
        let theme = config.markdown_theme.parse().unwrap_or_default();
        let renderer = MarkdownRenderer::with_theme(theme);
        match renderer.render_and_print(&content_to_display) {
            Ok(_) => {},
            Err(e) => {
                crate::log_debug!("{}: {}", "Warning: Markdown rendering failed".yellow(), e);
                println!("{}", content_to_display);  // Plain text fallback
            }
        }
    } else {
        // Non-interactive mode or markdown disabled: use plain text
        println!("{}", content_to_display);
    }
}
```

### 2. Update all calls to `print_assistant_response()`
**Files**:
- `src/session/chat/response.rs` (2 locations)

Add `is_interactive` or `params.is_interactive` parameter to all calls.

### 3. Add `is_interactive` to tool display functions
**File**: `src/session/chat/tool_display.rs`

Modify functions to accept `is_interactive` parameter and use plain text when false:
- `display_individual_tool_header_with_context()`
- `display_tool_output_smart()`

### 4. Update tool execution display
**File**: `src/session/chat/response/tool_execution.rs`

Pass `is_interactive` through:
- `display_tool_success()`
- `display_tool_error()`
- All tool display calls

### 5. Disable colored output globally for non-interactive
**Alternative approach**: Set environment variable `NO_COLOR=1` when running in non-interactive mode.

This would automatically disable all colored output from the `colored` crate.

## Implementation Priority

**Quick Fix (Recommended)**:
Set `NO_COLOR=1` environment variable in non-interactive mode at the start of `run_interactive_session_with_input()`.

**Complete Fix**:
Implement all steps above for full control over output formatting.

## Testing
```bash
# Test non-interactive mode
echo "What is 2+2?" | octomind run

# Should output plain text without ANSI codes
# Expected: plain text response
# Current: ANSI escape codes visible
```
