# MAP (Modular Agentic Planner) Implementation Plan for Octomind

## 📋 Executive Summary

Implement brain-inspired MAP architecture using Octomind's existing flexible layer system. The implementation leverages current infrastructure (layers, MCP tools, orchestration) and adds MAP-specific coordination logic through configuration and a new orchestrator layer type.

**Key Insight**: Octomind's layer system is already 90% ready for MAP. We just need:
1. MAP-specific layer configurations (config-only)
2. MAP orchestrator logic (new layer type)
3. Feedback loop mechanism (enhancement to existing orchestrator)

---

## 🎯 Architecture Overview

### Current Octomind Layer System (What We Have)

```rust
// src/session/layers/orchestrator.rs
pub struct LayeredOrchestrator {
    pub layers: Vec<Box<dyn Layer + Send + Sync>>,
}

// Layers process sequentially:
// Input → Layer1 → Layer2 → Layer3 → Output
// Each layer:
// - Has own model, system prompt, MCP tools
// - Processes input independently
// - Returns output to next layer
```

**Key Features Already Available**:
- ✅ Layer trait with `process()` method
- ✅ GenericLayer implementation (flexible, config-driven)
- ✅ MCP tool access per layer
- ✅ Input/Output modes (last, all, summary, none, append, replace)
- ✅ Sequential orchestration
- ✅ Token/cost tracking
- ✅ Cancellation support

### MAP Architecture (What We Need)

```
User Task → TaskDecomposer → [For each subgoal]:
                                 ┌─────────────────┐
                                 │  Feedback Loop  │
                                 │                 │
                                 │  Actor ──→ Monitor
                                 │    ↑         │
                                 │    └─────────┘
                                 │   (retry if invalid)
                                 │                 │
                                 │  Valid proposals│
                                 │       ↓         │
                                 │  Tree Search:   │
                                 │  - Predictor    │
                                 │  - Evaluator    │
                                 │  - Select best  │
                                 │                 │
                                 │  Execute best   │
                                 │       ↓         │
                                 │  Orchestrator   │
                                 │  (check done?)  │
                                 └─────────────────┘
                              → Final Result
```

**New Requirements**:
- ⚠️ Feedback loops (Actor ↔ Monitor iteration)
- ⚠️ Tree search logic (Predictor + Evaluator scoring)
- ⚠️ Subgoal iteration (loop until Orchestrator says "done")
- ⚠️ MAP orchestration layer type

---

## 🏗️ Implementation Strategy

### Phase 1: Configuration-Based MAP Layers (Config Only - No Code)

**Goal**: Define all MAP modules as standard layers in config

**File**: `config-templates/default.toml`

**Add MAP Layer Definitions**:

```toml
#############################################
# MAP (Modular Agentic Planner) Layers
#############################################

# Monitor Layer - Validates proposed actions
[[layers.map_monitor]]
description = "Validates code changes against project rules and constraints"
model = "openrouter:anthropic/claude-sonnet-4"
temperature = 0.0
max_tokens = 4096
input_mode = "last"
output_mode = "none"
output_role = "assistant"
system_prompt = """You are a code validation monitor inspired by the Anterior Cingulate Cortex (ACC).

Your role is to detect errors, conflicts, and constraint violations in proposed code changes.

INPUT FORMAT:
You will receive a proposed code change with:
- File path
- Specific modifications
- Context about the change

VALIDATION CHECKS:
1. Syntax correctness
2. Project rule compliance (check INSTRUCTIONS.md patterns)
3. Type safety and compilation viability
4. Security vulnerabilities
5. Breaking changes to existing APIs
6. MCP protocol compliance (for MCP tools)

OUTPUT FORMAT:
Return ONLY one of these responses:

VALID
(if the proposal passes all checks)

INVALID: <specific reason>
FEEDBACK: <actionable guidance for fixing>
(if there are issues)

Be specific and actionable in your feedback. Focus on what's wrong and how to fix it."""

[layers.map_monitor.mcp]
server_refs = ["developer", "filesystem"]
allowed_tools = ["ast_grep", "text_editor", "view_signatures"]

# Actor Layer - Proposes code changes
[[layers.map_actor]]
description = "Proposes specific code changes to achieve subgoals"
model = "openrouter:anthropic/claude-sonnet-4"
temperature = 0.2
max_tokens = 8192
input_mode = "last"
output_mode = "none"
output_role = "assistant"
system_prompt = """You are a code action proposer inspired by the dorsolateral Prefrontal Cortex (dlPFC).

Your role is to generate specific, actionable code changes to achieve a given subgoal.

INPUT FORMAT:
You will receive:
- A subgoal to achieve
- Current codebase context
- (Optional) Feedback from previous attempts

PROPOSAL REQUIREMENTS:
1. Generate 2-3 alternative approaches when possible
2. Be specific: exact file paths, line numbers, code snippets
3. Follow existing code patterns and conventions
4. Consider edge cases and error handling
5. Provide clear rationale for each proposal

OUTPUT FORMAT:
PROPOSAL 1: <brief description>
File: <path/to/file.rs>
Action: <insert|replace|modify>
Location: <line numbers or function name>
Code:
```rust
<exact code to add/change>
```
Rationale: <why this approach>

PROPOSAL 2: <brief description>
...

If you receive FEEDBACK, incorporate it into your next proposals."""

[layers.map_actor.mcp]
server_refs = ["developer", "filesystem"]
allowed_tools = ["semantic_search", "ast_grep", "text_editor", "view_signatures", "list_files"]

# Predictor Layer - Predicts impact of changes
[[layers.map_predictor]]
description = "Predicts the impact and consequences of proposed code changes"
model = "openrouter:anthropic/claude-sonnet-4"
temperature = 0.1
max_tokens = 4096
input_mode = "last"
output_mode = "none"
output_role = "assistant"
system_prompt = """You are a code impact predictor inspired by the Orbitofrontal Cortex (OFC).

Your role is to predict what will happen after a code change is applied.

INPUT FORMAT:
You will receive a validated code proposal with:
- File path and changes
- Current codebase state
- Subgoal context

PREDICTION REQUIREMENTS:
Analyze and predict:
1. Direct impact (what changes immediately)
2. Affected components (what else is impacted)
3. Compilation/runtime effects
4. Potential side effects or bugs
5. Integration with existing code

OUTPUT FORMAT:
DIRECT_IMPACT:
- <specific immediate changes>

AFFECTED_COMPONENTS:
- <file/module/function affected>
- <nature of impact>

POTENTIAL_ISSUES:
- <possible problems or edge cases>

COMPILATION_IMPACT: <will it compile? any warnings?>

CONFIDENCE: <high|medium|low>

Be thorough but concise. Focus on actionable predictions."""

[layers.map_predictor.mcp]
server_refs = ["developer", "filesystem"]
allowed_tools = ["semantic_search", "graphrag", "view_signatures", "ast_grep"]

# Evaluator Layer - Judges quality of predicted states
[[layers.map_evaluator]]
description = "Evaluates the quality and goal-progress of predicted code states"
model = "openrouter:anthropic/claude-sonnet-4"
temperature = 0.0
max_tokens = 4096
input_mode = "last"
output_mode = "none"
output_role = "assistant"
system_prompt = """You are a code quality evaluator inspired by the Orbitofrontal Cortex (OFC).

Your role is to assess the value and quality of a predicted code state.

INPUT FORMAT:
You will receive:
- A prediction of code changes and their impact
- The subgoal being pursued
- Current progress context

EVALUATION CRITERIA:
1. Goal Progress: How much closer to the subgoal?
2. Code Quality: Maintainability, readability, performance
3. Risk Assessment: Potential for bugs or issues
4. Alignment: Does it follow project patterns?
5. Completeness: Are there missing pieces?

OUTPUT FORMAT:
QUALITY_SCORE: <0-10>
(0=poor, 10=excellent)

GOAL_PROGRESS: <0-100>%
(percentage toward subgoal completion)

STEPS_REMAINING: <estimated number>
(how many more changes needed)

STRENGTHS:
- <what's good about this approach>

WEAKNESSES:
- <what could be improved>

RECOMMENDATION: <accept|revise|reject>

REASONING: <brief explanation>

Be objective and constructive. Focus on helping achieve the goal."""

[layers.map_evaluator.mcp]
server_refs = ["filesystem"]
allowed_tools = ["view_signatures", "semantic_search"]

# TaskDecomposer Layer - Breaks down complex tasks
[[layers.map_task_decomposer]]
description = "Decomposes complex development tasks into sequential subgoals"
model = "openrouter:anthropic/claude-sonnet-4"
temperature = 0.3
max_tokens = 8192
input_mode = "all"
output_mode = "none"
output_role = "assistant"
system_prompt = """You are a task decomposition specialist inspired by the anterior Prefrontal Cortex (aPFC).

Your role is to break down complex development tasks into manageable, sequential subgoals.

INPUT FORMAT:
You will receive:
- A high-level development task
- Current codebase context
- Project structure and patterns

DECOMPOSITION REQUIREMENTS:
1. Create 3-7 sequential subgoals
2. Each subgoal should be concrete and testable
3. Identify dependencies between subgoals
4. Order subgoals logically (dependencies first)
5. Ensure each subgoal is achievable in isolation

OUTPUT FORMAT:
SUBGOAL 1: <clear, specific description>
Dependencies: <none or list of prerequisite subgoals>
Success Criteria:
- <how to verify this subgoal is complete>
- <specific tests or checks>
Files Involved: <list of files to modify>

SUBGOAL 2: <description>
Dependencies: <e.g., "Subgoal 1">
Success Criteria:
- <verification steps>
Files Involved: <list>

...

EXECUTION ORDER: [1, 2, 3, ...]

ESTIMATED COMPLEXITY: <low|medium|high>

Think hierarchically. Break complex goals into simpler steps."""

[layers.map_task_decomposer.mcp]
server_refs = ["filesystem", "developer"]
allowed_tools = ["semantic_search", "graphrag", "view_signatures", "list_files"]

# Orchestrator Layer - Determines task completion
[[layers.map_orchestrator]]
description = "Determines when subgoals and final goals are achieved"
model = "openrouter:anthropic/claude-sonnet-4"
temperature = 0.0
max_tokens = 2048
input_mode = "last"
output_mode = "none"
output_role = "assistant"
system_prompt = """You are a task completion orchestrator inspired by the anterior Prefrontal Cortex (aPFC).

Your role is to determine when subgoals and final goals have been achieved.

INPUT FORMAT:
You will receive:
- A subgoal or final goal description
- Current code state after changes
- Success criteria for the goal

VERIFICATION REQUIREMENTS:
1. Check if all success criteria are met
2. Verify code compiles (if applicable)
3. Confirm no regressions introduced
4. Validate against original requirements

OUTPUT FORMAT:
Return ONLY one of these responses:

COMPLETE
(if the subgoal/goal is fully achieved)

INCOMPLETE: <what's missing>
NEXT_STEPS: <specific actions needed>
(if not yet complete)

FINAL_COMPLETE
(if the entire task is done - all subgoals achieved)

Be strict but fair. Don't mark complete unless truly done."""

[layers.map_orchestrator.mcp]
server_refs = ["developer", "filesystem"]
allowed_tools = ["shell", "text_editor", "view_signatures", "ast_grep"]
```

**Benefits of Config-Only Approach**:
- ✅ No code changes needed for layer definitions
- ✅ Easy to tune prompts and parameters
- ✅ Users can customize MAP behavior via config
- ✅ Leverages existing GenericLayer implementation

---

### Phase 2: MAP Orchestrator Layer Type (New Code)

**Goal**: Create a specialized layer type that implements MAP coordination logic

**File**: `src/session/layers/types/map.rs` (NEW)

```rust
// Copyright 2025 Muvon Un Limited
// Licensed under the Apache License, Version 2.0

use super::super::layer_trait::{Layer, LayerConfig, LayerResult};
use crate::config::Config;
use crate::session::Session;
use anyhow::Result;
use async_trait::async_trait;

/// MAP (Modular Agentic Planner) orchestrator layer
/// Coordinates MAP modules (Monitor, Actor, Predictor, Evaluator, TaskDecomposer, Orchestrator)
/// to perform brain-inspired planning and code generation
pub struct MapLayer {
    config: LayerConfig,
}

impl MapLayer {
    pub fn new(config: LayerConfig) -> Self {
        Self { config }
    }

    /// Execute the full MAP workflow
    async fn execute_map_workflow(
        &self,
        task: &str,
        session: &Session,
        config: &Config,
        operation_cancelled: tokio::sync::watch::Receiver<bool>,
    ) -> Result<String> {
        let mut result = String::new();

        // Step 1: Task Decomposition
        result.push_str("=== MAP: Task Decomposition ===\n");
        let subgoals = self.execute_layer_by_name(
            "map_task_decomposer",
            task,
            session,
            config,
            operation_cancelled.clone(),
        ).await?;

        result.push_str(&format!("Subgoals identified:\n{}\n\n", subgoals));

        // Parse subgoals from output
        let subgoal_list = self.parse_subgoals(&subgoals);

        // Step 2: Execute each subgoal
        for (i, subgoal) in subgoal_list.iter().enumerate() {
            result.push_str(&format!("=== MAP: Executing Subgoal {} ===\n", i + 1));
            result.push_str(&format!("{}\n\n", subgoal));

            // Subgoal execution loop
            let subgoal_result = self.execute_subgoal(
                subgoal,
                session,
                config,
                operation_cancelled.clone(),
            ).await?;

            result.push_str(&subgoal_result);
            result.push_str("\n\n");

            // Check if operation was cancelled
            if *operation_cancelled.borrow() {
                return Err(anyhow::anyhow!("Operation cancelled"));
            }
        }

        // Step 3: Final verification
        result.push_str("=== MAP: Final Verification ===\n");
        let final_check = self.execute_layer_by_name(
            "map_orchestrator",
            &format!("Original task: {}\n\nCompleted work:\n{}", task, result),
            session,
            config,
            operation_cancelled.clone(),
        ).await?;

        result.push_str(&final_check);

        Ok(result)
    }

    /// Execute a single subgoal using MAP modules
    async fn execute_subgoal(
        &self,
        subgoal: &str,
        session: &Session,
        config: &Config,
        operation_cancelled: tokio::sync::watch::Receiver<bool>,
    ) -> Result<String> {
        let mut result = String::new();
        let max_iterations = 5; // Prevent infinite loops

        for iteration in 0..max_iterations {
            result.push_str(&format!("--- Iteration {} ---\n", iteration + 1));

            // Step 1: Actor proposes actions
            let proposals = self.execute_layer_by_name(
                "map_actor",
                subgoal,
                session,
                config,
                operation_cancelled.clone(),
            ).await?;

            result.push_str(&format!("Proposals:\n{}\n\n", proposals));

            // Step 2: Monitor validates proposals (feedback loop)
            let (valid_proposals, feedback) = self.validate_proposals(
                &proposals,
                session,
                config,
                operation_cancelled.clone(),
            ).await?;

            if valid_proposals.is_empty() {
                result.push_str(&format!("All proposals invalid. Feedback:\n{}\n", feedback));
                // Feed back to Actor for retry
                continue;
            }

            result.push_str(&format!("Valid proposals: {}\n\n", valid_proposals.len()));

            // Step 3: Tree Search (Predictor + Evaluator)
            let best_proposal = self.tree_search(
                &valid_proposals,
                subgoal,
                session,
                config,
                operation_cancelled.clone(),
            ).await?;

            result.push_str(&format!("Selected best proposal:\n{}\n\n", best_proposal));

            // Step 4: Execute the best proposal
            // (In real implementation, this would apply code changes)
            result.push_str("Executing proposal...\n");

            // Step 5: Check completion with Orchestrator
            let completion_status = self.execute_layer_by_name(
                "map_orchestrator",
                &format!("Subgoal: {}\n\nChanges made:\n{}", subgoal, best_proposal),
                session,
                config,
                operation_cancelled.clone(),
            ).await?;

            result.push_str(&format!("Status: {}\n\n", completion_status));

            if completion_status.contains("COMPLETE") {
                result.push_str("✓ Subgoal achieved!\n");
                break;
            }

            if iteration == max_iterations - 1 {
                result.push_str("⚠ Max iterations reached. Subgoal may be incomplete.\n");
            }
        }

        Ok(result)
    }

    /// Validate proposals using Monitor layer (feedback loop)
    async fn validate_proposals(
        &self,
        proposals: &str,
        session: &Session,
        config: &Config,
        operation_cancelled: tokio::sync::watch::Receiver<bool>,
    ) -> Result<(Vec<String>, String)> {
        let validation_result = self.execute_layer_by_name(
            "map_monitor",
            proposals,
            session,
            config,
            operation_cancelled,
        ).await?;

        // Parse validation results
        let valid_proposals = Vec::new(); // TODO: Parse valid proposals
        let feedback = validation_result.clone();

        Ok((valid_proposals, feedback))
    }

    /// Tree search using Predictor and Evaluator
    async fn tree_search(
        &self,
        proposals: &[String],
        subgoal: &str,
        session: &Session,
        config: &Config,
        operation_cancelled: tokio::sync::watch::Receiver<bool>,
    ) -> Result<String> {
        let mut best_proposal = String::new();
        let mut best_score = 0.0;

        for proposal in proposals {
            // Predict impact
            let prediction = self.execute_layer_by_name(
                "map_predictor",
                &format!("Subgoal: {}\n\nProposal:\n{}", subgoal, proposal),
                session,
                config,
                operation_cancelled.clone(),
            ).await?;

            // Evaluate quality
            let evaluation = self.execute_layer_by_name(
                "map_evaluator",
                &format!("Subgoal: {}\n\nPrediction:\n{}", subgoal, prediction),
                session,
                config,
                operation_cancelled.clone(),
            ).await?;

            // Parse score from evaluation
            let score = self.parse_quality_score(&evaluation);

            if score > best_score {
                best_score = score;
                best_proposal = proposal.clone();
            }
        }

        Ok(best_proposal)
    }

    /// Execute a specific MAP layer by name
    async fn execute_layer_by_name(
        &self,
        layer_name: &str,
        input: &str,
        session: &Session,
        config: &Config,
        operation_cancelled: tokio::sync::watch::Receiver<bool>,
    ) -> Result<String> {
        // Get layer config from global registry
        let layer_config = config
            .layers
            .layers
            .get(layer_name)
            .ok_or_else(|| anyhow::anyhow!("Layer '{}' not found in config", layer_name))?
            .clone();

        // Create GenericLayer instance
        let layer = super::GenericLayer::new(layer_config);

        // Execute layer
        let result = layer.process(input, session, config, operation_cancelled).await?;

        // Return last output
        Ok(result.outputs.last().unwrap_or(&String::new()).clone())
    }

    /// Parse subgoals from TaskDecomposer output
    fn parse_subgoals(&self, output: &str) -> Vec<String> {
        // TODO: Implement robust parsing
        // For now, simple line-based parsing
        output
            .lines()
            .filter(|line| line.starts_with("SUBGOAL"))
            .map(|line| line.to_string())
            .collect()
    }

    /// Parse quality score from Evaluator output
    fn parse_quality_score(&self, output: &str) -> f64 {
        // TODO: Implement robust parsing
        // Look for "QUALITY_SCORE: X"
        output
            .lines()
            .find(|line| line.starts_with("QUALITY_SCORE:"))
            .and_then(|line| line.split(':').nth(1))
            .and_then(|score| score.trim().parse::<f64>().ok())
            .unwrap_or(0.0)
    }
}

#[async_trait]
impl Layer for MapLayer {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn config(&self) -> &LayerConfig {
        &self.config
    }

    async fn process(
        &self,
        input: &str,
        session: &Session,
        config: &Config,
        operation_cancelled: tokio::sync::watch::Receiver<bool>,
    ) -> Result<LayerResult> {
        let start = std::time::Instant::now();

        // Execute MAP workflow
        let output = self.execute_map_workflow(
            input,
            session,
            config,
            operation_cancelled,
        ).await?;

        // Create result
        Ok(LayerResult {
            outputs: vec![output],
            exchange: crate::session::ProviderExchange::default(),
            token_usage: None,
            tool_calls: None,
            api_time_ms: 0,
            tool_time_ms: 0,
            total_time_ms: start.elapsed().as_millis() as u64,
        })
    }
}
```

**Update**: `src/session/layers/types/mod.rs`

```rust
mod generic;
mod map; // NEW

pub use generic::GenericLayer;
pub use map::MapLayer; // NEW
```

---

### Phase 3: Integration with Orchestrator (Enhancement)

**Goal**: Allow orchestrator to instantiate MAP layer type

**File**: `src/session/layers/orchestrator.rs`

**Modification** (around line 49):

```rust
// Create layers from enabled layer configs
for layer_config in enabled_layers {
    // Check if this is a MAP orchestrator layer
    if layer_config.name.starts_with("map_") && layer_config.name.contains("orchestrator") {
        // Use specialized MAP layer type
        layers.push(Box::new(MapLayer::new(layer_config)));
    } else {
        // Use generic layer for all other layers
        layers.push(Box::new(GenericLayer::new(layer_config)));
    }
}
```

---

### Phase 4: Configuration for MAP Command

**Goal**: Create a command that triggers MAP workflow

**File**: `config-templates/default.toml`

**Add MAP Command**:

```toml
# MAP Development Command
[[commands]]
name = "map"
description = "Execute development task using MAP (Modular Agentic Planner) architecture"
layer_name = "map_main_orchestrator"

# Main MAP orchestrator layer (coordinates all MAP modules)
[[layers.map_main_orchestrator]]
description = "Main MAP orchestrator that coordinates all MAP modules for complex development tasks"
model = "openrouter:anthropic/claude-sonnet-4"
temperature = 0.1
max_tokens = 16384
input_mode = "all"
output_mode = "last"
output_role = "assistant"
system_prompt = """You are the main MAP (Modular Agentic Planner) orchestrator.

Your role is to coordinate specialized MAP modules to solve complex development tasks.

You have access to these MAP modules:
- map_task_decomposer: Breaks tasks into subgoals
- map_actor: Proposes code changes
- map_monitor: Validates proposals
- map_predictor: Predicts impact
- map_evaluator: Judges quality
- map_orchestrator: Checks completion

Execute the MAP workflow to achieve the user's development goal."""

[layers.map_main_orchestrator.mcp]
server_refs = ["developer", "filesystem", "web"]
allowed_tools = ["developer:*", "filesystem:*", "web_search"]
```

---

## 📊 Implementation Phases Summary

| Phase | Description | Files Changed | Complexity | Time Estimate |
|-------|-------------|---------------|------------|---------------|
| **Phase 1** | Config-based MAP layers | `config-templates/default.toml` | Low | 2-3 hours |
| **Phase 2** | MAP orchestrator layer type | `src/session/layers/types/map.rs` (new) | Medium | 1 day |
| **Phase 3** | Integration with orchestrator | `src/session/layers/orchestrator.rs` | Low | 2 hours |
| **Phase 4** | MAP command configuration | `config-templates/default.toml` | Low | 1 hour |
| **Testing** | End-to-end testing | Various | Medium | 4-6 hours |

**Total Estimated Time**: 2-3 days

---

## 🎮 Usage Examples

### Basic Usage

```bash
# Start session
octomind session

# Use MAP for complex task
/run map "Add user authentication with JWT tokens to the API"
```

### What Happens Internally

1. **TaskDecomposer** breaks it into:
   - Subgoal 1: Add JWT dependency to Cargo.toml
   - Subgoal 2: Create auth middleware module
   - Subgoal 3: Implement login endpoint
   - Subgoal 4: Protect existing routes with middleware

2. **For each subgoal**:
   - **Actor** proposes 2-3 implementation approaches
   - **Monitor** validates each proposal (feedback loop if invalid)
   - **Predictor** forecasts impact of each valid proposal
   - **Evaluator** scores quality and goal progress
   - Best proposal is selected and executed
   - **Orchestrator** verifies subgoal completion

3. **Final verification**: Orchestrator confirms entire task is complete

---

## 🔧 Configuration Flexibility

### Tuning MAP Behavior

Users can customize MAP by editing `config-templates/default.toml`:

**Example: Make Monitor more strict**
```toml
[[layers.map_monitor]]
temperature = 0.0  # More deterministic
system_prompt = """... add more strict validation rules ..."""
```

**Example: Use cheaper model for Predictor**
```toml
[[layers.map_predictor]]
model = "openrouter:anthropic/claude-haiku"  # Faster, cheaper
```

**Example: Adjust tree search depth**
```toml
[[layers.map_main_orchestrator]]
[layers.map_main_orchestrator.parameters]
max_proposals = 3
max_iterations = 5
tree_search_depth = 2
```

---

## 🚀 Advanced Features (Future Enhancements)

### 1. Parallel Tree Search
Instead of sequential evaluation, evaluate multiple proposals in parallel:

```rust
// In tree_search method
let futures: Vec<_> = proposals.iter().map(|p| {
    self.evaluate_proposal(p, subgoal, session, config)
}).collect();

let results = futures::future::join_all(futures).await;
```

### 2. Learning from Past Executions
Store successful MAP executions and use them as few-shot examples:

```toml
[[layers.map_actor]]
[layers.map_actor.parameters]
use_past_examples = true
example_cache_size = 10
```

### 3. Interactive Mode
Allow user to approve/reject proposals before execution:

```toml
[[layers.map_main_orchestrator]]
[layers.map_main_orchestrator.parameters]
interactive_mode = true
require_approval = true
```

### 4. Cost Optimization
Use cheaper models for certain modules:

```toml
# Expensive for critical decisions
[[layers.map_monitor]]
model = "openrouter:anthropic/claude-sonnet-4"

# Cheaper for predictions
[[layers.map_predictor]]
model = "openrouter:anthropic/claude-haiku"
```

---

## ✅ Testing Strategy

### Unit Tests

**File**: `src/session/layers/types/map_test.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_subgoals() {
        let output = "SUBGOAL 1: Add dependency\nSUBGOAL 2: Create module";
        let layer = MapLayer::new(/* ... */);
        let subgoals = layer.parse_subgoals(output);
        assert_eq!(subgoals.len(), 2);
    }

    #[tokio::test]
    async fn test_parse_quality_score() {
        let output = "QUALITY_SCORE: 8.5\nOther text";
        let layer = MapLayer::new(/* ... */);
        let score = layer.parse_quality_score(output);
        assert_eq!(score, 8.5);
    }
}
```

### Integration Tests

**Test Scenario**: Simple task decomposition and execution

```bash
# Test with a simple, well-defined task
octomind session --name map_test
/run map "Add a new function 'hello_world' to src/main.rs that prints 'Hello, World!'"
```

**Expected Behavior**:
1. TaskDecomposer creates 1-2 subgoals
2. Actor proposes implementation
3. Monitor validates (should pass)
4. Predictor/Evaluator score it
5. Code is generated
6. Orchestrator confirms completion

---

## 📋 Checklist for Implementation

### Phase 1: Configuration
- [ ] Add MAP layer definitions to `config-templates/default.toml`
- [ ] Test config loading with `cargo check`
- [ ] Validate layer configs with `octomind config --validate`

### Phase 2: MAP Layer Type
- [ ] Create `src/session/layers/types/map.rs`
- [ ] Implement `MapLayer` struct and methods
- [ ] Implement `Layer` trait for `MapLayer`
- [ ] Add unit tests for parsing functions
- [ ] Update `src/session/layers/types/mod.rs`

### Phase 3: Integration
- [ ] Modify `src/session/layers/orchestrator.rs`
- [ ] Add MAP layer type detection
- [ ] Test layer instantiation

### Phase 4: Command Configuration
- [ ] Add MAP command to config
- [ ] Add main orchestrator layer config
- [ ] Test command execution

### Phase 5: Testing
- [ ] Unit tests for MAP layer
- [ ] Integration test with simple task
- [ ] Integration test with complex task
- [ ] Performance testing (token usage, cost)
- [ ] Error handling testing

### Phase 6: Documentation
- [ ] Update `doc/07-command-layers.md` with MAP section
- [ ] Add MAP examples to README
- [ ] Create MAP tutorial in docs
- [ ] Update CHANGELOG.md

---

## 🎯 Success Criteria

1. ✅ MAP command executes without errors
2. ✅ TaskDecomposer successfully breaks down tasks
3. ✅ Actor-Monitor feedback loop works (retries on invalid proposals)
4. ✅ Tree search selects best proposals based on scores
5. ✅ Orchestrator correctly detects completion
6. ✅ Full workflow completes for simple tasks (< 5 minutes)
7. ✅ Token usage is reasonable (< 50k tokens for simple tasks)
8. ✅ Cost is acceptable (< $0.50 for simple tasks)

---

## 🔍 Key Design Decisions

### 1. Why Config-Based Layers?
- **Flexibility**: Users can customize prompts without code changes
- **Simplicity**: Leverages existing GenericLayer implementation
- **Maintainability**: Easier to tune and debug

### 2. Why Specialized MAP Layer Type?
- **Coordination Logic**: MAP requires complex orchestration (feedback loops, tree search)
- **Separation of Concerns**: Keep MAP logic separate from generic layers
- **Extensibility**: Easy to add MAP-specific features

### 3. Why Sequential Subgoal Execution?
- **Simplicity**: Easier to implement and debug
- **Determinism**: Predictable execution order
- **Future**: Can add parallel execution later if needed

### 4. Why Feedback Loops?
- **Core MAP Feature**: Monitor-Actor iteration is essential for quality
- **Error Prevention**: Catches invalid proposals before execution
- **Alignment with Paper**: Matches ACC-dlPFC interaction in brain

---

## 📚 References

- **MAP Paper**: [Nature Communications - A brain-inspired agentic architecture](https://www.nature.com/articles/s41467-025-63804-5)
- **Octomind Layers**: `doc/07-command-layers.md`
- **Octomind Architecture**: `INSTRUCTIONS.md`

---

## 🚀 Next Steps

1. **Review this plan** with the team
2. **Start with Phase 1** (config-only, low risk)
3. **Test Phase 1** thoroughly before moving to Phase 2
4. **Iterate** based on feedback and results
5. **Document** learnings and best practices

---

**Ready to implement?** Let's start with Phase 1! 🎉
