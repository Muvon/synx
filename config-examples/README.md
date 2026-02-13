# Octomind Configuration Examples

This directory contains example configurations demonstrating different workflow patterns and use cases.

## 🧠 Understanding Workflows: Planner vs Executor

**CRITICAL CONCEPT**: Workflows are **PLANNERS**, not executors!

```
┌─────────────────────────────────────────────────────────┐
│                    USER INPUT                            │
│              "Add JWT authentication"                    │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│              WORKFLOW (PLANNER)                          │
│  • Decomposes task into subgoals                        │
│  • Validates approaches                                 │
│  • Predicts impacts                                     │
│  • Evaluates quality                                    │
│  • Generates enhanced plan/input                        │
└────────────────────┬────────────────────────────────────┘
                     │
                     │ Enhanced Input/Plan
                     ▼
┌─────────────────────────────────────────────────────────┐
│              MAIN MODEL (EXECUTOR)                       │
│  • Receives refined plan from workflow                  │
│  • Executes with full context                          │
│  • Uses MCP tools to implement                         │
│  • Returns final result                                │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│                    OUTPUT                                │
│              Implementation complete                     │
└─────────────────────────────────────────────────────────┘
```

### Why This Architecture?

**Brain-Inspired Design**:
- **Prefrontal Cortex** (workflow) → Planning, validation, decision-making
- **Motor Cortex** (main model) → Execution of planned actions

**Benefits**:
- ✅ Separation of planning and execution
- ✅ Validation before implementation
- ✅ Multiple planning strategies for same executor
- ✅ Reduced execution errors through better planning
- ✅ Cognitive architecture alignment

## Available Configurations

### 1. MAP Developer Workflow (`map-developer-workflow.toml`)

**Brain-inspired planning architecture for complex software development.**

Based on the research paper: "A brain-inspired agentic architecture to improve planning with LLMs" (Nature Communications, 2025)

**Features**:
- ✅ Task decomposition into subgoals
- ✅ Actor-Monitor feedback loops
- ✅ Impact prediction and quality evaluation
- ✅ Automatic validation and error correction
- ✅ Completion verification at each step

**Best for**:
- Complex refactoring tasks
- Multi-step implementations
- Tasks requiring validation
- High-quality code generation

**Quick Start**:
```bash
# Copy to your config
cp map-developer-workflow.toml ~/.config/octomind/config.toml

# Or use directly
octomind session --config map-developer-workflow.toml

# Test with a simple task
> Add serde_json dependency to this project
```

**See**: `MAP_TESTING_GUIDE.md` for detailed testing instructions

### 2. Simple Sequential Workflow (included in MAP config)

**Basic sequential processing without MAP architecture.**

**Features**:
- Task refinement
- Codebase research
- Direct execution

**Best for**:
- Simple tasks
- Quick queries
- Exploratory work

**Usage**:
```toml
# In your config, set:
workflow = "sequential"  # instead of "map_development"
```

## Workflow System Capabilities

Our workflow system provides flexible control flow:

| Type | Description | Use Case |
|------|-------------|----------|
| **once** | Execute layer once | Single operations |
| **loop** | Repeat until condition | Feedback loops, iteration |
| **foreach** | Iterate over items | Process subgoals, batch operations |
| **conditional** | Branch on pattern | Validation, decision making |
| **parallel** | Execute in parallel | Tree search, exploration |

## Pattern-Based Control

Workflows use regex patterns for control flow:

```toml
# Parse subgoals from output
parse_pattern = "SUBGOAL \\d+: (.*)"

# Exit loop when complete
exit_pattern = "COMPLETE"

# Branch on validation
condition_pattern = "VALID"
```

## Creating Custom Workflows

### Basic Structure

```toml
[workflows.my_workflow]
description = "My custom workflow"

[[workflows.my_workflow.steps]]
name = "step1"
type = "once"
layer = "my_layer"

[[workflows.my_workflow.steps]]
name = "step2"
type = "loop"
max_iterations = 5
exit_pattern = "DONE"

  [[workflows.my_workflow.steps.substeps]]
  name = "substep"
  type = "once"
  layer = "another_layer"
```

### Layer Configuration

```toml
[layers.my_layer]
description = "My custom layer"
model = "openrouter:anthropic/claude-sonnet-4"
temperature = 0.1
max_tokens = 8192
input_mode = "last"  # or "all"
output_mode = "none"  # or "all"
output_role = "assistant"
system_prompt = """Your layer instructions here.

OUTPUT FORMAT:
<specify exact format>
"""

[layers.my_layer.mcp]
server_refs = ["filesystem", "developer"]
allowed_tools = ["text_editor", "shell"]
```

## Comparison: MAP vs Simple

| Aspect | MAP Workflow | Simple Workflow |
|--------|--------------|-----------------|
| **Task Decomposition** | ✅ Automatic | ❌ Manual |
| **Validation** | ✅ Built-in | ❌ None |
| **Feedback Loops** | ✅ Actor-Monitor | ❌ Single pass |
| **Quality Evaluation** | ✅ Scored | ❌ None |
| **Impact Prediction** | ✅ Analyzed | ❌ None |
| **Completion Verification** | ✅ Automatic | ❌ Manual |
| **Token Usage** | 🔴 High (20-50K) | 🟢 Low (5K) |
| **Cost** | 🔴 Higher | 🟢 Lower |
| **Quality** | 🟢 Higher | 🟡 Variable |
| **Robustness** | 🟢 High | 🟡 Medium |

## Performance Tips

### Cost Optimization

```toml
# Use cheaper models for validation
[layers.map_monitor]
model = "openrouter:anthropic/claude-haiku"

# Keep quality for critical modules
[layers.map_actor]
model = "openrouter:anthropic/claude-sonnet-4"
```

### Speed Optimization

```toml
# Reduce iterations for simple tasks
max_iterations = 3  # instead of 5

# Use smaller context
max_tokens = 4096  # instead of 8192
```

## Research Background

The MAP workflow is based on cognitive neuroscience research:

- **aPFC** (anterior prefrontal cortex) → Task decomposition, coordination
- **dlPFC** (dorsolateral prefrontal cortex) → Action proposal
- **ACC** (anterior cingulate cortex) → Conflict monitoring
- **OFC** (orbitofrontal cortex) → State prediction, value estimation

**Key Results from Paper**:
- 74% success rate vs 11% baseline (Tower of Hanoi)
- 0% invalid actions vs 31% baseline
- Better transfer across tasks
- Outperforms Tree-of-Thought and Multi-Agent Debate

## Examples

### Example 1: Add Dependency

```
User: Add serde_json to this project

Workflow:
1. TaskDecomposer → 2 subgoals
2. For each subgoal:
   - Actor proposes solutions
   - Monitor validates
   - Predictor analyzes impact
   - Evaluator scores quality
   - Orchestrator confirms completion
3. Final verification

Result: Dependency added, verified, complete
```

### Example 2: Implement Feature

```
User: Add JWT authentication to API

Workflow:
1. TaskDecomposer → 4 subgoals
   - Add dependency
   - Create middleware
   - Implement validation
   - Protect routes
2. For each subgoal (with feedback loops):
   - Multiple Actor proposals
   - Monitor catches issues
   - Predictor identifies impacts
   - Evaluator ensures quality
3. Final verification

Result: Complete authentication system, validated
```

## Troubleshooting

### Common Issues

**Loop never exits**:
- Check Orchestrator output format
- Verify exit_pattern matches output
- Enable debug logging: `/loglevel debug`

**No subgoals parsed**:
- Check TaskDecomposer output format
- Verify parse_pattern regex
- Test pattern with sample output

**Monitor always rejects**:
- Check condition_pattern matches "VALID"
- Verify Monitor output format
- Review validation criteria

### Debug Commands

```bash
# Enable debug logging
/loglevel debug

# Check workflow status
/info

# View context
/context all
```

## Contributing

To add new example configurations:

1. Create new `.toml` file in this directory
2. Add documentation in this README
3. Include testing guide if complex
4. Test thoroughly before committing

## Resources

- **Main Documentation**: `../doc/`
- **MAP Testing Guide**: `MAP_TESTING_GUIDE.md`
- **Research Paper**: https://www.nature.com/articles/s41467-025-63804-5
- **Workflow Diagrams**: `../MAP_DIAGRAMS.md`

## License

Apache License 2.0 - See `../LICENSE` for details
