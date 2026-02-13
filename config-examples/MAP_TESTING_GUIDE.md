# MAP Developer Workflow - Testing Guide

## Overview

This configuration implements the **Modular Agentic Planner (MAP)** architecture from the Nature Communications paper "A brain-inspired agentic architecture to improve planning with LLMs" (Webb, Mondal, Momennejad, 2025).

## 🧠 Critical Architecture Understanding

**MAP is a PLANNER, not an EXECUTOR!**

```
┌──────────────────────────────────────────────────────────────┐
│                      USER TASK                                │
│         "Add JWT authentication to API"                       │
└────────────────────────┬─────────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────────┐
│                  MAP WORKFLOW (PLANNER)                       │
│                                                               │
│  Step 1: TaskDecomposer                                      │
│    Output: SUBGOAL 1: Add dependency                         │
│            SUBGOAL 2: Create middleware                      │
│            SUBGOAL 3: Implement validation                   │
│                                                               │
│  Step 2: For Each Subgoal                                    │
│    Actor → Proposes 2-3 approaches                           │
│    Monitor → Validates proposals                             │
│    Predictor → Analyzes impact                               │
│    Evaluator → Scores quality                                │
│    Orchestrator → Confirms completion                        │
│                                                               │
│  Step 3: Final Orchestrator                                  │
│    Output: Comprehensive implementation plan                 │
│                                                               │
│  WORKFLOW OUTPUT: Enhanced, validated, structured plan       │
└────────────────────────┬─────────────────────────────────────┘
                         │
                         │ Enhanced Plan
                         ▼
┌──────────────────────────────────────────────────────────────┐
│              MAIN MODEL (EXECUTOR)                            │
│                                                               │
│  Receives: Detailed plan with validated approaches           │
│  Executes: Uses MCP tools to implement                       │
│  - text_editor to modify files                               │
│  - shell to run commands                                     │
│  - ast_grep to refactor code                                 │
│                                                               │
│  Returns: Actual implementation                              │
└────────────────────────┬─────────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────────┐
│                      RESULT                                   │
│         JWT authentication implemented                        │
└──────────────────────────────────────────────────────────────┘
```

### Why This Matters

**Planning ≠ Execution**:
- **MAP Workflow**: Thinks, validates, plans (like prefrontal cortex)
- **Main Model**: Executes the plan (like motor cortex)

**Benefits**:
- ✅ Better plans through validation and iteration
- ✅ Reduced execution errors
- ✅ Clear separation of concerns
- ✅ Brain-inspired cognitive architecture

## Architecture Mapping

### Brain-Inspired Modules

| MAP Module | Brain Region | Workflow Layer | Function |
|------------|--------------|----------------|----------|
| **TaskDecomposer** | aPFC (anterior prefrontal cortex) | `map_task_decomposer` | Breaks tasks into 3-7 sequential subgoals |
| **Actor** | dlPFC (dorsolateral prefrontal cortex) | `map_actor` | Proposes 2-3 implementation approaches |
| **Monitor** | ACC (anterior cingulate cortex) | `map_monitor` | Validates proposals, provides feedback |
| **Predictor** | OFC (orbitofrontal cortex) | `map_predictor` | Predicts impact of changes |
| **Evaluator** | OFC (orbitofrontal cortex) | `map_evaluator` | Judges quality and progress |
| **Orchestrator** | aPFC | `map_orchestrator` | Checks subgoal completion |

### Workflow Control Flow

```
User Task
    ↓
TaskDecomposer (once) → Parses into subgoals
    ↓
For Each Subgoal (foreach)
    ↓
    Subgoal Loop (loop, max 5 iterations)
        ↓
        Actor (once) → Proposes 2-3 solutions
        ↓
        Monitor (conditional) → Validates
            ├─ VALID → Predictor → Evaluator
            └─ INVALID → Back to Actor (with feedback)
        ↓
        Orchestrator (once) → Checks completion
            ├─ COMPLETE → Exit loop
            └─ INCOMPLETE → Continue loop
    ↓
Final Orchestrator (once) → Verifies all subgoals complete
```

## Setup

### 1. Install Octomind

```bash
cd /path/to/octomind
cargo build
```

### 2. Set API Key

```bash
export OPENROUTER_API_KEY="your_key_here"
# OR
export OPENAI_API_KEY="your_key_here"
# OR
export ANTHROPIC_API_KEY="your_key_here"
```

### 3. Use MAP Configuration

```bash
# Copy the MAP config
cp config-examples/map-developer-workflow.toml ~/.config/octomind/config.toml

# OR use it directly
./target/debug/octomind session --config config-examples/map-developer-workflow.toml
```

## Test Cases

### Test 1: Simple Dependency Addition (Easy)

**Task**: "Add the serde_json crate to this project"

**Expected Workflow**:
1. **TaskDecomposer** breaks into subgoals:
   - SUBGOAL 1: Add serde_json to Cargo.toml
   - SUBGOAL 2: Verify compilation

2. **For SUBGOAL 1**:
   - **Actor** proposes: Add `serde_json = "1.0"` to dependencies
   - **Monitor** validates: VALID
   - **Predictor** predicts: Cargo.lock updates, build time increases slightly
   - **Evaluator** scores: 9/10, GOAL_PROGRESS: 100%
   - **Orchestrator** confirms: COMPLETE

3. **For SUBGOAL 2**:
   - **Actor** proposes: Run `cargo check`
   - **Monitor** validates: VALID
   - **Orchestrator** confirms: COMPLETE

4. **Final Orchestrator**: TASK_COMPLETE

**Success Criteria**:
- ✓ Dependency added to Cargo.toml
- ✓ `cargo check` passes
- ✓ No invalid proposals

### Test 2: Authentication Implementation (Medium)

**Task**: "Add JWT authentication to the API endpoints"

**Expected Workflow**:
1. **TaskDecomposer** breaks into subgoals:
   - SUBGOAL 1: Add JWT dependency
   - SUBGOAL 2: Create auth middleware module
   - SUBGOAL 3: Implement token validation
   - SUBGOAL 4: Protect existing routes

2. **For each subgoal**:
   - Actor-Monitor feedback loop (may iterate 2-3 times)
   - Predictor analyzes impact
   - Evaluator judges quality
   - Orchestrator confirms completion

3. **Final Orchestrator**: Verifies all 4 subgoals complete

**Success Criteria**:
- ✓ All 4 subgoals completed
- ✓ Monitor catches invalid proposals (e.g., wrong version)
- ✓ Predictor identifies affected components
- ✓ Evaluator provides quality scores
- ✓ Final verification passes

### Test 3: Complex Refactoring (Hard)

**Task**: "Refactor the session management system to use async/await throughout"

**Expected Workflow**:
1. **TaskDecomposer** breaks into 5-7 subgoals:
   - SUBGOAL 1: Identify sync functions to convert
   - SUBGOAL 2: Add async runtime dependencies
   - SUBGOAL 3: Convert core session functions
   - SUBGOAL 4: Update callers
   - SUBGOAL 5: Handle error propagation
   - SUBGOAL 6: Update tests
   - SUBGOAL 7: Verify compilation

2. **For each subgoal**:
   - Multiple Actor-Monitor iterations (3-5)
   - Monitor catches breaking changes
   - Predictor identifies cascading effects
   - Evaluator ensures quality maintained

3. **Final Orchestrator**: Comprehensive verification

**Success Criteria**:
- ✓ 5-7 subgoals identified
- ✓ Multiple Monitor feedback loops
- ✓ Predictor identifies dependencies
- ✓ Evaluator maintains quality standards
- ✓ All subgoals complete
- ✓ Code compiles and tests pass

## Monitoring Workflow Execution

### Visual Indicators

The workflow provides colored output showing progress:

```
═══ Workflow ═══
Brain-inspired MAP workflow for complex software development tasks

▶ Step 1/3: decompose_task
→ decompose_task

▶ Step 2/3: execute_subgoals
⇉ execute_subgoals (3 items)
  → Item 1/3
  ⟳ subgoal_loop (max: 5)
    → propose_solutions
    ⎇ validate_proposals
      ✓ Condition matched
    → check_completion
    ✓ Loop complete (iteration 2)
  → Item 2/3
  ...

▶ Step 3/3: final_verification
→ final_verification

✓ Workflow completed in 45.23s
```

### Debug Logging

Enable detailed logging:

```bash
# In session
/loglevel debug

# Or via environment
export OCTOMIND_LOG_LEVEL="debug"
```

### Key Metrics to Watch

1. **Loop Iterations**: Should be 1-3 for simple tasks, 3-5 for complex
2. **Monitor Feedback**: Should catch invalid proposals
3. **Completion Time**: Varies by task complexity
4. **Quality Scores**: Should be 7+ for accepted proposals

## Comparison with Simple Workflow

To see the difference, test with the simple sequential workflow:

```toml
# In config, change:
workflow = "sequential"  # instead of "map_development"
```

**Expected Differences**:
- ❌ No task decomposition
- ❌ No validation feedback loop
- ❌ No impact prediction
- ❌ No quality evaluation
- ❌ Less robust error handling

## Troubleshooting

### Issue: Loop Never Exits

**Symptom**: Subgoal loop reaches max iterations (5)

**Causes**:
- Orchestrator not detecting completion
- Success criteria too strict
- Actor not implementing changes

**Fix**:
- Check Orchestrator output format
- Verify success criteria in subgoal
- Ensure Actor has proper tools

### Issue: Monitor Always Rejects

**Symptom**: Conditional always goes to `on_no_match`

**Causes**:
- Monitor output doesn't match `condition_pattern`
- Pattern regex incorrect

**Fix**:
- Check Monitor output format (must start with "VALID" or "INVALID")
- Test pattern with `/loglevel debug`

### Issue: No Subgoals Parsed

**Symptom**: Foreach step processes 0 items

**Causes**:
- TaskDecomposer output format wrong
- `parse_pattern` regex doesn't match

**Fix**:
- Verify TaskDecomposer output format
- Test pattern: `SUBGOAL \\d+: (.*?)(?=\\nSUBGOAL|\\nSuccess Criteria|$)`

## Performance Considerations

### Token Usage

MAP workflow uses more tokens than simple approaches:

- **Simple**: ~5K tokens per task
- **MAP**: ~20-50K tokens per task (depending on complexity)

**Why?**: Multiple module calls, feedback loops, validation

**Mitigation**:
- Use smaller models for some modules (e.g., Monitor)
- Reduce max_iterations for simple tasks
- Cache module outputs (future optimization)

### Cost Optimization

```toml
# Use cheaper models for validation
[layers.map_monitor]
model = "openrouter:anthropic/claude-haiku"  # Cheaper

[layers.map_evaluator]
model = "openrouter:anthropic/claude-haiku"  # Cheaper

# Keep expensive models for critical modules
[layers.map_actor]
model = "openrouter:anthropic/claude-sonnet-4"  # Keep quality

[layers.map_task_decomposer]
model = "openrouter:anthropic/claude-sonnet-4"  # Keep quality
```

## Advanced Customization

### Adjust Loop Iterations

```toml
[[workflows.map_development.steps.substeps]]
name = "subgoal_loop"
type = "loop"
max_iterations = 3  # Reduce for simple tasks
exit_pattern = "COMPLETE"
```

### Add Parallel Tree Search

```toml
# Add parallel exploration of multiple approaches
[[workflows.map_development.steps.substeps.substeps]]
name = "explore_alternatives"
type = "parallel"
parallel_layers = ["map_actor", "map_actor", "map_actor"]
aggregator = "map_evaluator"
```

### Custom Patterns

```toml
# Custom subgoal pattern
parse_pattern = "TODO \\d+: (.*)"

# Custom completion pattern
exit_pattern = "DONE|FINISHED|COMPLETE"

# Custom validation pattern
condition_pattern = "APPROVED|VALID|OK"
```

## Research Paper Alignment

### Key Findings from Paper

1. **MAP outperforms baselines**: 74% vs 11% on Tower of Hanoi
2. **Monitor is critical**: Prevents invalid actions (0% invalid vs 31%)
3. **Tree search helps**: But not sufficient alone
4. **Modularization matters**: Better than multi-agent debate
5. **Transfer learning**: Better generalization across tasks

### Our Implementation

✅ **TaskDecomposer**: Hierarchical planning (aPFC-inspired)
✅ **Actor-Monitor Loop**: Feedback mechanism (dlPFC-ACC)
✅ **Predictor-Evaluator**: State prediction and value estimation (OFC)
✅ **Orchestrator**: Goal verification (aPFC)
✅ **Pattern-based control**: Regex parsing for flexibility

### Differences from Paper

- **No explicit tree search**: Can be added with `parallel` type
- **Regex patterns**: Instead of LLM-based parsing
- **Configurable**: Via TOML instead of hardcoded
- **Developer-focused**: Adapted for software tasks

## Next Steps

1. **Test on real tasks**: Use your actual development work
2. **Tune parameters**: Adjust iterations, patterns, models
3. **Add tree search**: Implement parallel exploration
4. **Measure performance**: Compare with simple workflow
5. **Optimize costs**: Use cheaper models where possible

## References

- Webb, T., Mondal, S.S. & Momennejad, I. (2025). "A brain-inspired agentic architecture to improve planning with LLMs." *Nature Communications*, 16, 8633.
- Paper URL: https://www.nature.com/articles/s41467-025-63804-5
- GitHub: https://github.com/Shanka123/MAP

## Support

For issues or questions:
1. Check `/loglevel debug` output
2. Review workflow execution logs
3. Verify output formats match patterns
4. Test with simpler tasks first
5. Compare with `sequential` workflow
