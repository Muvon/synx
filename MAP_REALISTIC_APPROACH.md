# MAP Implementation - Realistic Senior Approach

## 🎯 The Real Question

Can we implement MAP using **only configuration** without adding new layer types or commands?

## ❌ Short Answer: NO

**Why?** MAP requires **control flow logic** (loops, conditionals, branching) that cannot be expressed in pure declarative configuration.

**Current System:**
```
Layer1 → Layer2 → Layer3 → Output
(Sequential, no loops, no conditionals)
```

**MAP Needs:**
```
TaskDecomposer → For each subgoal:
                   Actor → Monitor → if invalid, retry Actor
                         → if valid, Predictor → Evaluator
                         → Execute best
                         → Orchestrator → if incomplete, retry
                 → Next subgoal
```

This is **imperative control flow**, not declarative configuration.

---

## ✅ What We CAN Do (Minimal Changes)

### Option 1: Add Control Flow to LayerConfig (Best Approach)

**Concept**: Extend `LayerConfig` with control flow directives that the orchestrator interprets.

**New Config Fields:**

```rust
// In src/session/layers/layer_trait.rs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LayerConfig {
    // ... existing fields ...

    // NEW: Control flow configuration
    #[serde(default)]
    pub control_flow: Option<ControlFlow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ControlFlow {
    // Execute once and continue
    Sequential,

    // Loop until condition met
    Loop {
        max_iterations: usize,
        condition_layer: String,  // Layer that checks "done?"
        condition_pattern: String, // Regex to match in output
    },

    // Conditional branching
    Conditional {
        condition_layer: String,
        condition_pattern: String,
        on_match: Vec<String>,    // Layer names to execute if matches
        on_no_match: Vec<String>, // Layer names to execute if doesn't match
    },

    // Parallel execution
    Parallel {
        layers: Vec<String>,      // Execute these layers in parallel
        aggregator: String,       // Layer that combines results
    },
}
```

**Configuration Example:**

```toml
# Role with MAP-style workflow
[[roles]]
name = "developer"
enable_layers = true
layer_refs = ["map_workflow"]

# MAP workflow layer with control flow
[[layers.map_workflow]]
description = "MAP-style development workflow"
input_mode = "all"
output_mode = "last"
system_prompt = "Coordinate MAP modules for development tasks"

# Control flow: Loop through subgoals
[layers.map_workflow.control_flow]
type = "loop"
max_iterations = 10
condition_layer = "map_orchestrator"
condition_pattern = "COMPLETE|FINAL_COMPLETE"

# Sub-layers for this workflow
[layers.map_workflow.sub_layers]
decomposer = "map_task_decomposer"
actor = "map_actor"
monitor = "map_monitor"
predictor = "map_predictor"
evaluator = "map_evaluator"
orchestrator = "map_orchestrator"

# Actor-Monitor feedback loop
[[layers.map_actor]]
description = "Proposes code changes"
# ... standard config ...

[layers.map_actor.control_flow]
type = "conditional"
condition_layer = "map_monitor"
condition_pattern = "VALID"
on_match = ["map_predictor"]      # Continue to prediction
on_no_match = ["map_actor"]       # Retry with feedback
```

**Code Changes Required:**

1. **Add ControlFlow enum** to `src/session/layers/layer_trait.rs` (~50 lines)
2. **Update orchestrator** to interpret control flow in `src/session/layers/orchestrator.rs` (~200 lines)
3. **Add loop/conditional logic** to process method (~150 lines)

**Total Code**: ~400 lines

**Benefits:**
- ✅ Pure configuration for workflows
- ✅ Reusable control flow patterns
- ✅ No new layer types
- ✅ No new commands
- ✅ Flexible and extensible

---

### Option 2: Simplified - Just Add Loop Support (Minimal)

**Concept**: Add only loop capability to layers, keep everything else sequential.

**New Config Field:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LayerConfig {
    // ... existing fields ...

    // NEW: Simple loop configuration
    #[serde(default)]
    pub loop_config: Option<LoopConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopConfig {
    pub max_iterations: usize,
    pub exit_pattern: String,  // Regex to match in output to exit loop
}
```

**Configuration Example:**

```toml
[[roles]]
name = "developer"
enable_layers = true
layer_refs = [
    "map_task_decomposer",
    "map_subgoal_loop",  # This layer loops
    "map_final_check"
]

# Subgoal execution loop
[[layers.map_subgoal_loop]]
description = "Execute subgoals until complete"
input_mode = "last"
output_mode = "none"
system_prompt = """Execute the current subgoal using Actor, Monitor, Predictor, Evaluator.
Return COMPLETE when done."""

# Loop configuration
[layers.map_subgoal_loop.loop_config]
max_iterations = 5
exit_pattern = "COMPLETE"

# This layer internally calls: actor → monitor → predictor → evaluator → orchestrator
# (Still needs some code to coordinate these, but simpler than full control flow)
```

**Code Changes Required:**

1. **Add LoopConfig** to `src/session/layers/layer_trait.rs` (~20 lines)
2. **Add loop logic** to orchestrator (~100 lines)

**Total Code**: ~120 lines

**Benefits:**
- ✅ Minimal code changes
- ✅ Solves the main MAP problem (iteration until done)
- ✅ Configuration-driven
- ❌ Still needs some hardcoded coordination logic

---

### Option 3: Hybrid - Config + Minimal Coordinator (Pragmatic)

**Concept**: Keep layers simple, add a lightweight "workflow coordinator" that reads workflow definitions from config.

**New Config Section:**

```toml
# Workflow definitions (new top-level section)
[[workflows]]
name = "map_development"
description = "MAP-style development workflow"

# Workflow steps with control flow
[[workflows.map_development.steps]]
name = "decompose"
layer = "map_task_decomposer"
type = "once"

[[workflows.map_development.steps]]
name = "execute_subgoals"
type = "foreach"  # Iterate over subgoals from previous step
parse_pattern = "SUBGOAL \\d+: (.*)"

  [[workflows.map_development.steps.execute_subgoals.substeps]]
  name = "propose"
  layer = "map_actor"

  [[workflows.map_development.steps.execute_subgoals.substeps]]
  name = "validate"
  layer = "map_monitor"
  retry_on_pattern = "INVALID"
  retry_target = "propose"
  max_retries = 3

  [[workflows.map_development.steps.execute_subgoals.substeps]]
  name = "predict"
  layer = "map_predictor"

  [[workflows.map_development.steps.execute_subgoals.substeps]]
  name = "evaluate"
  layer = "map_evaluator"

  [[workflows.map_development.steps.execute_subgoals.substeps]]
  name = "check_done"
  layer = "map_orchestrator"
  repeat_until_pattern = "COMPLETE"
  max_iterations = 5

[[workflows.map_development.steps]]
name = "final_check"
layer = "map_orchestrator"
type = "once"

# Use workflow in role
[[roles]]
name = "developer"
workflow = "map_development"  # Instead of layer_refs
```

**Code Changes Required:**

1. **Add Workflow config structs** in `src/config/workflows.rs` (NEW, ~150 lines)
2. **Add WorkflowOrchestrator** in `src/session/workflows/orchestrator.rs` (NEW, ~300 lines)
3. **Update role config** to support workflows (~50 lines)

**Total Code**: ~500 lines (but very clean, reusable)

**Benefits:**
- ✅ Fully declarative workflows
- ✅ Reusable workflow patterns
- ✅ Clear separation: layers = modules, workflows = coordination
- ✅ Easy to add new workflow types
- ✅ No changes to existing layer system

---

## 🎯 Recommendation: Option 3 (Hybrid)

**Why?**
1. **Clean separation**: Layers remain simple, workflows handle coordination
2. **Declarative**: Everything in config, no hardcoded logic
3. **Extensible**: Easy to add new workflow patterns
4. **Reusable**: Workflows can be shared across roles
5. **Senior-level**: Proper abstraction, not hacky

**Implementation Plan:**

### Phase 1: Add Workflow Config (2-3 hours)

**File**: `src/config/workflows.rs` (NEW)

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowConfig {
    pub name: String,
    pub description: String,
    pub steps: Vec<WorkflowStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowStep {
    pub name: String,
    pub layer: Option<String>,  // Layer to execute (if type = once/layer)

    #[serde(rename = "type")]
    pub step_type: WorkflowStepType,

    // For foreach loops
    pub parse_pattern: Option<String>,
    pub substeps: Option<Vec<WorkflowStep>>,

    // For retries
    pub retry_on_pattern: Option<String>,
    pub retry_target: Option<String>,
    pub max_retries: Option<usize>,

    // For loops
    pub repeat_until_pattern: Option<String>,
    pub max_iterations: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowStepType {
    Once,      // Execute layer once
    Foreach,   // Iterate over parsed items
    Loop,      // Loop until condition
    Parallel,  // Execute multiple layers in parallel
}
```

### Phase 2: Add Workflow Orchestrator (4-5 hours)

**File**: `src/session/workflows/orchestrator.rs` (NEW)

```rust
pub struct WorkflowOrchestrator {
    workflow: WorkflowConfig,
}

impl WorkflowOrchestrator {
    pub async fn execute(
        &self,
        input: &str,
        session: &mut Session,
        config: &Config,
        operation_cancelled: tokio::sync::watch::Receiver<bool>,
    ) -> Result<String> {
        let mut current_input = input.to_string();

        for step in &self.workflow.steps {
            current_input = self.execute_step(
                step,
                &current_input,
                session,
                config,
                operation_cancelled.clone(),
            ).await?;
        }

        Ok(current_input)
    }

    async fn execute_step(
        &self,
        step: &WorkflowStep,
        input: &str,
        session: &mut Session,
        config: &Config,
        operation_cancelled: tokio::sync::watch::Receiver<bool>,
    ) -> Result<String> {
        match step.step_type {
            WorkflowStepType::Once => {
                self.execute_layer_once(step, input, session, config, operation_cancelled).await
            }
            WorkflowStepType::Foreach => {
                self.execute_foreach(step, input, session, config, operation_cancelled).await
            }
            WorkflowStepType::Loop => {
                self.execute_loop(step, input, session, config, operation_cancelled).await
            }
            WorkflowStepType::Parallel => {
                self.execute_parallel(step, input, session, config, operation_cancelled).await
            }
        }
    }

    // ... implement each execution type
}
```

### Phase 3: Integrate with Roles (1-2 hours)

**File**: `src/config/roles.rs`

```rust
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RoleConfig {
    // ... existing fields ...

    // NEW: Either use layers OR workflow (mutually exclusive)
    pub workflow: Option<String>,  // Workflow name from global registry
}
```

### Phase 4: Update Session Runner (1-2 hours)

**File**: `src/session/chat/session/runner.rs`

```rust
// Check if role uses workflow or layers
if let Some(workflow_name) = &role_config.workflow {
    // Use workflow orchestrator
    let workflow = config.workflows.get(workflow_name)?;
    let orchestrator = WorkflowOrchestrator::new(workflow);
    orchestrator.execute(input, session, config, cancellation).await?
} else {
    // Use existing layer orchestrator
    layer_orchestrator.process(input, session, config, role, cancellation).await?
}
```

---

## 📊 Comparison

| Approach | Code Lines | Complexity | Flexibility | Config-Driven |
|----------|-----------|------------|-------------|---------------|
| **Option 1: Control Flow** | ~400 | Medium | High | ✅ Yes |
| **Option 2: Loop Only** | ~120 | Low | Low | ✅ Yes |
| **Option 3: Workflows** | ~500 | Medium | Very High | ✅✅ Yes |

---

## ✅ Final Answer

**Can we do MAP with pure config?** NO - we need control flow logic.

**Best approach?** Option 3 (Workflows) - clean, extensible, fully config-driven.

**Estimated time:** 8-12 hours of focused work.

**Benefits:**
- ✅ No new layer types
- ✅ No new commands (workflows are transparent)
- ✅ Fully declarative
- ✅ Reusable patterns
- ✅ Senior-level architecture

**Usage:**

```toml
[[roles]]
name = "developer"
workflow = "map_development"  # That's it!
```

The workflow handles all MAP coordination internally, reading from config.

---

## 🚀 Next Steps

1. **Decide**: Which option fits best?
2. **Prototype**: Start with workflow config structure
3. **Implement**: Build workflow orchestrator
4. **Test**: Simple workflow first, then MAP
5. **Document**: Update docs with workflow examples

**My recommendation**: Go with Option 3. It's the cleanest, most extensible, and truly config-driven approach.
