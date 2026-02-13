# MAP Implementation - Quick Start Guide

## 🎯 What is MAP?

**MAP (Modular Agentic Planner)** is a brain-inspired architecture from a Nature Communications paper that breaks down AI planning into specialized modules, similar to how the human prefrontal cortex works.

**Key Insight**: LLMs can do individual planning tasks (validate code, predict impact, evaluate quality) but struggle to coordinate them. MAP makes them work together through structured interaction.

## 🧠 MAP Modules (Brain-Inspired)

| Module | Brain Region | Role | What It Does |
|--------|--------------|------|--------------|
| **Monitor** | ACC (Anterior Cingulate Cortex) | Error Detection | Validates code changes, catches bugs |
| **Actor** | dlPFC (dorsolateral PFC) | Action Proposal | Proposes specific code changes |
| **Predictor** | OFC (Orbitofrontal Cortex) | State Prediction | Predicts impact of changes |
| **Evaluator** | OFC | Value Estimation | Judges quality and progress |
| **TaskDecomposer** | aPFC (anterior PFC) | Task Breakdown | Splits complex tasks into subgoals |
| **Orchestrator** | aPFC | Coordination | Determines when goals are achieved |

## 🏗️ How It Works in Octomind

```
User: "Add JWT authentication to the API"
    ↓
TaskDecomposer: Breaks into subgoals
    ↓
For each subgoal:
    ┌─────────────────────────────┐
    │ Actor: Proposes 2-3 ways    │
    │         ↓                    │
    │ Monitor: Validates each      │
    │         ↓ (feedback loop)    │
    │ Actor: Revises if invalid    │
    │         ↓                    │
    │ Predictor: Predicts impact   │
    │         ↓                    │
    │ Evaluator: Scores quality    │
    │         ↓                    │
    │ Execute best proposal        │
    │         ↓                    │
    │ Orchestrator: Check done?    │
    └─────────────────────────────┘
    ↓
Final Result
```

## 🚀 Implementation Strategy

### ✅ What We Already Have (90% Ready!)

Octomind's layer system is **already perfect** for MAP:

- ✅ **Layer trait** - Each MAP module is a layer
- ✅ **GenericLayer** - Config-driven layer implementation
- ✅ **Sequential orchestration** - Layers process in order
- ✅ **MCP tools per layer** - Each module has its own tools
- ✅ **Input/Output modes** - Flexible data flow

### ⚠️ What We Need to Add (10%)

1. **MAP layer configurations** (config-only, no code!)
2. **MAP orchestrator layer type** (new Rust code for coordination)
3. **Feedback loop mechanism** (Actor ↔ Monitor iteration)
4. **Tree search logic** (Predictor + Evaluator scoring)

## 📋 Implementation Phases

| Phase | What | Where | Time |
|-------|------|-------|------|
| **1** | Add MAP layer configs | `config-templates/default.toml` | 2-3 hours |
| **2** | Create MAP orchestrator | `src/session/layers/types/map.rs` | 1 day |
| **3** | Integrate with orchestrator | `src/session/layers/orchestrator.rs` | 2 hours |
| **4** | Add MAP command | `config-templates/default.toml` | 1 hour |
| **5** | Testing | Various | 4-6 hours |

**Total**: 2-3 days

## 🎮 Usage Example

```bash
# Start session
octomind session

# Use MAP for complex development task
/run map "Add user authentication with JWT tokens to the API"
```

**What happens**:
1. TaskDecomposer: "Need to add JWT dependency, create middleware, add login endpoint, protect routes"
2. For "Add JWT dependency":
   - Actor: "Add `jsonwebtoken = '9.2'` to Cargo.toml"
   - Monitor: "VALID"
   - Predictor: "Will add dependency, no breaking changes"
   - Evaluator: "Score 9/10, good approach"
   - Execute: Adds to Cargo.toml
   - Orchestrator: "COMPLETE"
3. For "Create middleware":
   - Actor: Proposes 3 ways to structure middleware
   - Monitor: Validates each
   - Predictor: Predicts impact
   - Evaluator: Scores (picks best)
   - Execute: Creates middleware
   - Orchestrator: "COMPLETE"
4. ... continues for all subgoals
5. Final: "Task complete!"

## 🔧 Configuration Flexibility

All MAP behavior is configurable via `config-templates/default.toml`:

```toml
# Make Monitor more strict
[[layers.map_monitor]]
temperature = 0.0
system_prompt = """... stricter validation rules ..."""

# Use cheaper model for Predictor
[[layers.map_predictor]]
model = "openrouter:anthropic/claude-haiku"

# Adjust iterations
[[layers.map_main_orchestrator]]
[layers.map_main_orchestrator.parameters]
max_iterations = 5
max_proposals = 3
```

## 📊 Benefits Over Standard Approach

| Aspect | Standard LLM | MAP Architecture |
|--------|--------------|------------------|
| **Hallucinations** | Common (invalid code) | Rare (Monitor catches) |
| **Planning** | Struggles with multi-step | Excellent (TaskDecomposer) |
| **Quality** | Variable | Consistent (Evaluator scores) |
| **Completion** | Unclear when done | Clear (Orchestrator verifies) |
| **Debugging** | Black box | Transparent (see each module) |

## 🎯 Success Metrics

From the paper's results:

- **Tower of Hanoi**: 74% solved (vs 11% for GPT-4 zero-shot)
- **Graph Traversal**: 95-100% solved (vs 50% for best baseline)
- **PlanBench**: Significant improvement over all baselines
- **StrategyQA**: On par with human performance

**Expected for Octomind**:
- ✅ Fewer invalid code proposals
- ✅ Better task decomposition
- ✅ Higher quality code generation
- ✅ Clear completion verification
- ✅ More explainable process

## 🚀 Getting Started

### Step 1: Read the Full Plan
```bash
cat MAP_IMPLEMENTATION_PLAN.md
```

### Step 2: Start with Phase 1 (Config Only)
- Add MAP layer definitions to `config-templates/default.toml`
- Test with `cargo check`
- Validate with `octomind config --validate`

### Step 3: Implement Phase 2 (MAP Orchestrator)
- Create `src/session/layers/types/map.rs`
- Implement coordination logic
- Add unit tests

### Step 4: Test End-to-End
```bash
octomind session
/run map "Add a hello_world function to src/main.rs"
```

## 📚 Key Files

- **Full Plan**: `MAP_IMPLEMENTATION_PLAN.md` (detailed implementation guide)
- **Paper**: https://www.nature.com/articles/s41467-025-63804-5
- **Octomind Layers**: `doc/07-command-layers.md`
- **Architecture**: `INSTRUCTIONS.md`

## 💡 Key Design Decisions

1. **Config-Based Layers**: All MAP modules defined in config (no code changes for layer definitions)
2. **Specialized Orchestrator**: MAP coordination logic in dedicated layer type
3. **Leverage Existing System**: 90% uses current Octomind infrastructure
4. **Flexible & Tunable**: Users can customize via config

## 🎉 Why This Will Work

1. **Proven Architecture**: Published in Nature Communications with strong results
2. **Perfect Fit**: Octomind's layer system is already 90% ready
3. **Low Risk**: Config-first approach, minimal code changes
4. **High Value**: Significantly improves planning and code quality
5. **Extensible**: Easy to add features (parallel search, learning, etc.)

---

**Ready to implement?** Start with Phase 1 in `MAP_IMPLEMENTATION_PLAN.md`! 🚀
