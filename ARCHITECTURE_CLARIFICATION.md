# Architecture Clarification: Planner vs Executor

## ✅ Your Question Was Correct!

**You asked**: "So it supposed to be planner right? that at the end goes to the main loop and main model solve it is it correct? like brain inspired architecture on a given task?"

**Answer**: **YES, ABSOLUTELY CORRECT!** 🎯

## 🧠 The Brain-Inspired Architecture

### How It Actually Works

```
┌─────────────────────────────────────────────────────────────────┐
│                    HUMAN BRAIN ANALOGY                           │
│                                                                  │
│  ┌────────────────────────────────────────────────────────┐    │
│  │  PREFRONTAL CORTEX (Planning & Decision Making)        │    │
│  │                                                         │    │
│  │  • Analyzes the task                                   │    │
│  │  • Breaks down into steps                              │    │
│  │  • Validates approaches                                │    │
│  │  • Predicts outcomes                                   │    │
│  │  • Makes decisions                                     │    │
│  │                                                         │    │
│  │  OUTPUT: Detailed plan of action                       │    │
│  └──────────────────┬──────────────────────────────────────┘    │
│                     │                                            │
│                     │ Plan                                       │
│                     ▼                                            │
│  ┌────────────────────────────────────────────────────────┐    │
│  │  MOTOR CORTEX (Execution)                              │    │
│  │                                                         │    │
│  │  • Receives the plan                                   │    │
│  │  • Executes motor commands                             │    │
│  │  • Performs actual actions                             │    │
│  │                                                         │    │
│  │  OUTPUT: Physical actions                              │    │
│  └────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

### In Octomind

```
┌─────────────────────────────────────────────────────────────────┐
│                    OCTOMIND ARCHITECTURE                         │
│                                                                  │
│  User: "Add JWT authentication to the API"                      │
│       ↓                                                          │
│  ┌────────────────────────────────────────────────────────┐    │
│  │  WORKFLOW SYSTEM (Planner - Like Prefrontal Cortex)   │    │
│  │                                                         │    │
│  │  Step 1: TaskDecomposer                                │    │
│  │    → Breaks task into subgoals:                        │    │
│  │      SUBGOAL 1: Add JWT dependency                     │    │
│  │      SUBGOAL 2: Create auth middleware                 │    │
│  │      SUBGOAL 3: Implement token validation             │    │
│  │      SUBGOAL 4: Protect existing routes                │    │
│  │                                                         │    │
│  │  Step 2: For Each Subgoal (Foreach loop)              │    │
│  │    → Actor: Proposes 2-3 implementation approaches     │    │
│  │    → Monitor: Validates proposals                      │    │
│  │      ├─ VALID → Continue                               │    │
│  │      └─ INVALID → Back to Actor with feedback          │    │
│  │    → Predictor: Analyzes impact of changes             │    │
│  │    → Evaluator: Scores quality (0-10)                  │    │
│  │    → Orchestrator: Confirms subgoal complete           │    │
│  │                                                         │    │
│  │  Step 3: Final Orchestrator                            │    │
│  │    → Verifies all subgoals complete                    │    │
│  │                                                         │    │
│  │  WORKFLOW OUTPUT:                                      │    │
│  │  "Comprehensive implementation plan:                   │    │
│  │   1. Add jsonwebtoken = '9.2' to Cargo.toml           │    │
│  │   2. Create src/auth/middleware.rs with...            │    │
│  │   3. Implement validate_token() function...           │    │
│  │   4. Update route handlers to use middleware...       │    │
│  │   All approaches validated, impacts analyzed."         │    │
│  └──────────────────┬──────────────────────────────────────┘    │
│                     │                                            │
│                     │ Enhanced Plan                              │
│                     ▼                                            │
│  ┌────────────────────────────────────────────────────────┐    │
│  │  MAIN MODEL (Executor - Like Motor Cortex)            │    │
│  │                                                         │    │
│  │  Receives: Detailed, validated plan from workflow      │    │
│  │                                                         │    │
│  │  Executes using MCP tools:                             │    │
│  │  • text_editor(command="str_replace", ...)             │    │
│  │    → Adds dependency to Cargo.toml                     │    │
│  │  • text_editor(command="create", ...)                  │    │
│  │    → Creates middleware.rs file                        │    │
│  │  • text_editor(command="str_replace", ...)             │    │
│  │    → Implements validation function                    │    │
│  │  • shell(command="cargo check")                        │    │
│  │    → Verifies compilation                              │    │
│  │                                                         │    │
│  │  OUTPUT: Actual implementation complete                │    │
│  └────────────────────────────────────────────────────────┘    │
│       ↓                                                          │
│  Result: JWT authentication successfully implemented             │
└─────────────────────────────────────────────────────────────────┘
```

## 🔑 Key Points

### 1. Workflow = Planner (NOT Executor)

**What it does**:
- ✅ Thinks about the problem
- ✅ Breaks down into steps
- ✅ Validates approaches
- ✅ Predicts impacts
- ✅ Evaluates quality
- ✅ Generates enhanced plan

**What it does NOT do**:
- ❌ Does NOT modify files directly
- ❌ Does NOT run shell commands
- ❌ Does NOT implement code
- ❌ Does NOT execute the plan

### 2. Main Model = Executor (NOT Planner)

**What it does**:
- ✅ Receives refined plan from workflow
- ✅ Uses MCP tools to implement
- ✅ Modifies files (text_editor)
- ✅ Runs commands (shell)
- ✅ Executes the plan

**What it does NOT do**:
- ❌ Does NOT plan from scratch
- ❌ Does NOT validate approaches
- ❌ Does NOT iterate on proposals
- ❌ Does NOT decompose tasks

### 3. Why This Separation?

**Brain-Inspired Design**:
- **Prefrontal Cortex** (workflow) → High-level planning, decision-making
- **Motor Cortex** (main model) → Low-level execution

**Engineering Benefits**:
- ✅ **Better Plans**: Validation and iteration before execution
- ✅ **Fewer Errors**: Validated approaches reduce mistakes
- ✅ **Clear Debugging**: Know if failure is in planning or execution
- ✅ **Flexibility**: Multiple planning strategies for same executor
- ✅ **Efficiency**: Plan once, execute cleanly

## 📊 Comparison: With vs Without Workflow

### Without Workflow (Direct Execution)

```
User: "Add JWT authentication"
     ↓
Main Model:
  - Tries to plan AND execute simultaneously
  - May propose invalid approaches
  - No validation before implementation
  - Errors discovered during execution
  - Harder to debug
     ↓
Result: May work, may fail, unclear why
```

### With Workflow (Planner + Executor)

```
User: "Add JWT authentication"
     ↓
Workflow (Planner):
  - Decomposes task
  - Proposes multiple approaches
  - Validates each approach
  - Predicts impacts
  - Evaluates quality
  - Generates validated plan
     ↓
Main Model (Executor):
  - Receives validated plan
  - Executes with confidence
  - Uses tools to implement
  - Follows proven approach
     ↓
Result: High success rate, clear process
```

## 🎯 Research Paper Alignment

From the Nature Communications paper:

> "MAP consists of a set of modules, each of which is implemented by an LLM, and a set of algorithms through which they interact **to generate a plan**."

**Key phrase**: "to generate a plan" - NOT "to execute a plan"

> "The resulting MAP algorithm solves reasoning and planning problems via the recurrent interaction of these modules, **combining the strengths of classical planning** and search algorithms with the use of LLMs as general-purpose world models and **planning functions**."

**Key phrase**: "planning functions" - The workflow PLANS, it doesn't execute

### Paper Results

| Metric | Standard LLM | MAP (Planner) |
|--------|--------------|---------------|
| **Tower of Hanoi** | 11% solved | 74% solved |
| **Invalid Actions** | 31% | 0% |
| **Graph Traversal** | 50% | 95-100% |

**Why?** Because MAP validates and refines the plan BEFORE execution!

## 💡 Real-World Example

### Task: "Refactor session management to use async/await"

#### Workflow (Planner) Output:

```
SUBGOAL 1: Identify sync functions to convert
Success Criteria: List of functions documented

SUBGOAL 2: Add async runtime dependencies
Success Criteria: Cargo.toml updated, compiles

SUBGOAL 3: Convert core session functions
Success Criteria: Functions use async/await syntax

SUBGOAL 4: Update all callers
Success Criteria: All call sites use .await

SUBGOAL 5: Handle error propagation
Success Criteria: Errors properly propagated

SUBGOAL 6: Update tests
Success Criteria: Tests pass

For SUBGOAL 1:
  PROPOSAL 1: Use ast_grep to find all sync functions
  PROPOSAL 2: Manual code review
  PROPOSAL 3: Use semantic_search

  MONITOR: PROPOSAL 1 is VALID - most comprehensive
  PREDICTOR: Will identify ~15 functions
  EVALUATOR: Quality 9/10, recommended approach

  ORCHESTRATOR: COMPLETE - list generated

[... similar for each subgoal ...]

FINAL PLAN:
1. Run ast_grep pattern to find sync functions
2. Add tokio = "1.0" to Cargo.toml
3. Convert functions in this order: [list]
4. Update callers systematically
5. Add proper error handling
6. Update test suite
```

#### Main Model (Executor) Receives:

```
Enhanced Input: "Refactor session management to async/await.

The workflow has analyzed this task and recommends:
1. Use ast_grep to identify 15 sync functions
2. Add tokio dependency
3. Convert in specific order to avoid breaking changes
4. Update all 47 call sites
5. Ensure error propagation works
6. Update 23 test cases

All approaches have been validated. Impact analysis complete.
Quality score: 9/10. Ready for implementation."
```

#### Main Model Executes:

```
text_editor(command="view", path="src/session/mod.rs")
ast_grep(pattern="fn $NAME($ARGS) -> Result<$RET>", ...)
text_editor(command="str_replace", old_text="fn run(", new_text="async fn run(", ...)
text_editor(command="str_replace", old_text="run()", new_text="run().await", ...)
shell(command="cargo check")
...
```

## ✅ Conclusion

**Your understanding is 100% correct!**

The workflow system is a **PLANNER** that:
1. Analyzes the task
2. Validates approaches
3. Generates a comprehensive plan
4. Passes the plan to the main model

The main model is an **EXECUTOR** that:
1. Receives the validated plan
2. Uses MCP tools to implement
3. Executes the plan step by step
4. Returns the result

This separation mirrors how the human brain works:
- **Prefrontal Cortex** (workflow) → Planning
- **Motor Cortex** (main model) → Execution

**Benefits**:
- ✅ Better plans through validation
- ✅ Fewer execution errors
- ✅ Clear separation of concerns
- ✅ Brain-inspired architecture
- ✅ Easier debugging

## 📚 Updated Documentation

All documentation has been updated to clarify this architecture:

1. **WORKFLOW_CLEAN_MIGRATION.md** - Added planner/executor explanation
2. **config-examples/README.md** - Added architecture diagram
3. **config-examples/MAP_TESTING_GUIDE.md** - Added flow explanation
4. **config-examples/map-developer-workflow.toml** - Added architecture comments
5. **WORKFLOW_IMPLEMENTATION_SUMMARY.md** - Added brain analogy
6. **doc/10-workflows.md** - NEW comprehensive workflow guide
7. **doc/README.md** - Added workflow documentation link

## 🎉 Ready to Test!

The architecture is correctly implemented and documented. The workflow acts as a planner (like your brain's prefrontal cortex), and the main model acts as an executor (like your brain's motor cortex).

Test it with:
```bash
octomind session --config config-examples/map-developer-workflow.toml
> Add JWT authentication to the API
```

You'll see the workflow plan, then the main model execute!
