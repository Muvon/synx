# MAP Architecture Diagrams

## 1. High-Level MAP Workflow

```
┌─────────────────────────────────────────────────────────────────┐
│                         USER TASK                                │
│              "Add JWT authentication to API"                     │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                    TASK DECOMPOSER                               │
│  (Breaks task into sequential subgoals)                         │
│                                                                  │
│  Output:                                                         │
│  • Subgoal 1: Add JWT dependency                                │
│  • Subgoal 2: Create auth middleware                            │
│  • Subgoal 3: Implement login endpoint                          │
│  • Subgoal 4: Protect existing routes                           │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
                    ┌────────────────┐
                    │  For Each      │
                    │  Subgoal       │
                    └────────┬───────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                    SUBGOAL EXECUTION LOOP                        │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  1. ACTOR: Propose 2-3 implementation approaches         │  │
│  │     Output: Proposal 1, Proposal 2, Proposal 3           │  │
│  └────────────────────────┬─────────────────────────────────┘  │
│                            │                                     │
│                            ▼                                     │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  2. MONITOR: Validate each proposal                      │  │
│  │     Check: Syntax, rules, security, compatibility        │  │
│  │     Output: VALID or INVALID + FEEDBACK                  │  │
│  └────────────────────────┬─────────────────────────────────┘  │
│                            │                                     │
│                            ├─── INVALID? ───┐                   │
│                            │                 │                   │
│                            │                 ▼                   │
│                            │         ┌───────────────┐           │
│                            │         │  Feedback to  │           │
│                            │         │  Actor        │           │
│                            │         │  (Retry)      │           │
│                            │         └───────┬───────┘           │
│                            │                 │                   │
│                            │                 └───────────────┐   │
│                            │                                 │   │
│                            ▼                                 │   │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  3. TREE SEARCH (for each valid proposal):              │  │
│  │                                                          │  │
│  │     a) PREDICTOR: Predict impact                        │  │
│  │        Output: Direct impact, affected components       │  │
│  │                                                          │  │
│  │     b) EVALUATOR: Score quality                         │  │
│  │        Output: Quality score (0-10), goal progress      │  │
│  │                                                          │  │
│  │     c) Select best proposal (highest score)             │  │
│  └────────────────────────┬─────────────────────────────────┘  │
│                            │                                     │
│                            ▼                                     │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  4. EXECUTE: Apply best proposal                        │  │
│  │     (Make actual code changes)                           │  │
│  └────────────────────────┬─────────────────────────────────┘  │
│                            │                                     │
│                            ▼                                     │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  5. ORCHESTRATOR: Check completion                       │  │
│  │     Verify: Success criteria met?                        │  │
│  │     Output: COMPLETE or INCOMPLETE + next steps          │  │
│  └────────────────────────┬─────────────────────────────────┘  │
│                            │                                     │
│                            ├─── INCOMPLETE? ───┐                │
│                            │                    │                │
│                            │                    └────────────┐   │
│                            │                                 │   │
│                            ▼                                 │   │
│                      COMPLETE!                              │   │
│                            │                                 │   │
└────────────────────────────┼─────────────────────────────────┘   │
                             │                                     │
                             ▼                                     │
                    Next Subgoal ◄───────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                   FINAL VERIFICATION                             │
│  Orchestrator confirms all subgoals complete                    │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
                        TASK DONE!
```

## 2. MAP Modules and Brain Regions

```
┌─────────────────────────────────────────────────────────────────┐
│                    HUMAN PREFRONTAL CORTEX                       │
│                                                                  │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐   │
│  │      ACC       │  │     dlPFC      │  │      OFC       │   │
│  │  (Conflict     │  │  (Decision     │  │  (State        │   │
│  │   Monitoring)  │  │   Making)      │  │   Prediction)  │   │
│  └────────┬───────┘  └────────┬───────┘  └────────┬───────┘   │
│           │                   │                   │             │
│           │                   │                   │             │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐   │
│  │      aPFC      │  │      aPFC      │  │      OFC       │   │
│  │  (Task         │  │  (Task         │  │  (Value        │   │
│  │   Decomp)      │  │   Coord)       │  │   Estimation)  │   │
│  └────────────────┘  └────────────────┘  └────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                             │
                             │ Inspiration
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                      MAP ARCHITECTURE                            │
│                                                                  │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐   │
│  │    MONITOR     │  │     ACTOR      │  │   PREDICTOR    │   │
│  │  Validates     │  │  Proposes      │  │  Predicts      │   │
│  │  proposals     │  │  actions       │  │  impact        │   │
│  └────────┬───────┘  └────────┬───────┘  └────────┬───────┘   │
│           │                   │                   │             │
│           │                   │                   │             │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐   │
│  │ TASK           │  │ ORCHESTRATOR   │  │  EVALUATOR     │   │
│  │ DECOMPOSER     │  │ Checks         │  │  Judges        │   │
│  │ Breaks tasks   │  │ completion     │  │  quality       │   │
│  └────────────────┘  └────────────────┘  └────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## 3. Octomind Layer System Integration

```
┌─────────────────────────────────────────────────────────────────┐
│                  OCTOMIND LAYER SYSTEM                           │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  LayeredOrchestrator                                     │  │
│  │  • Manages pipeline of layers                            │  │
│  │  • Sequential execution                                  │  │
│  │  • Token/cost tracking                                   │  │
│  └────────────────────────┬─────────────────────────────────┘  │
│                            │                                     │
│                            ▼                                     │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  Layer Trait                                             │  │
│  │  • name()                                                │  │
│  │  • config()                                              │  │
│  │  • process(input, session, config) -> LayerResult       │  │
│  └────────────────────────┬─────────────────────────────────┘  │
│                            │                                     │
│                            ▼                                     │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  Layer Implementations                                   │  │
│  │                                                          │  │
│  │  ┌────────────────┐         ┌────────────────┐         │  │
│  │  │ GenericLayer   │         │   MapLayer     │         │  │
│  │  │ (Config-driven)│         │ (MAP logic)    │         │  │
│  │  │                │         │                │         │  │
│  │  │ Used for:      │         │ Used for:      │         │  │
│  │  │ • task_refiner │         │ • map_main_    │         │  │
│  │  │ • task_        │         │   orchestrator │         │  │
│  │  │   researcher   │         │                │         │  │
│  │  │ • All MAP      │         │ Coordinates:   │         │  │
│  │  │   modules      │         │ • map_monitor  │         │  │
│  │  │   (monitor,    │         │ • map_actor    │         │  │
│  │  │    actor, etc) │         │ • map_predictor│         │  │
│  │  │                │         │ • map_evaluator│         │  │
│  │  │                │         │ • map_task_    │         │  │
│  │  │                │         │   decomposer   │         │  │
│  │  │                │         │ • map_         │         │  │
│  │  │                │         │   orchestrator │         │  │
│  │  └────────────────┘         └────────────────┘         │  │
│  └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

## 4. Data Flow in MAP

```
┌─────────────────────────────────────────────────────────────────┐
│                         INPUT                                    │
│  "Add JWT authentication to the API"                            │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│  TaskDecomposer Layer (GenericLayer)                            │
│  Input: Full task description                                   │
│  Output: "SUBGOAL 1: Add JWT dependency\n                       │
│           SUBGOAL 2: Create middleware\n..."                    │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│  MapLayer (Main Orchestrator)                                   │
│  • Parses subgoals                                              │
│  • For each subgoal:                                            │
│                                                                  │
│    ┌────────────────────────────────────────────────────────┐  │
│    │  Actor Layer (GenericLayer)                            │  │
│    │  Input: "SUBGOAL 1: Add JWT dependency"                │  │
│    │  Output: "PROPOSAL 1: Add to Cargo.toml\n              │  │
│    │           PROPOSAL 2: Use different crate\n..."         │  │
│    └──────────────────────┬─────────────────────────────────┘  │
│                            │                                     │
│                            ▼                                     │
│    ┌────────────────────────────────────────────────────────┐  │
│    │  Monitor Layer (GenericLayer)                          │  │
│    │  Input: All proposals                                  │  │
│    │  Output: "PROPOSAL 1: VALID\n                          │  │
│    │           PROPOSAL 2: INVALID - wrong version\n..."    │  │
│    └──────────────────────┬─────────────────────────────────┘  │
│                            │                                     │
│                            ▼                                     │
│    ┌────────────────────────────────────────────────────────┐  │
│    │  Predictor Layer (GenericLayer)                        │  │
│    │  Input: Valid proposal                                 │  │
│    │  Output: "DIRECT_IMPACT: Adds dependency\n             │  │
│    │           AFFECTED: Cargo.lock will update\n..."       │  │
│    └──────────────────────┬─────────────────────────────────┘  │
│                            │                                     │
│                            ▼                                     │
│    ┌────────────────────────────────────────────────────────┐  │
│    │  Evaluator Layer (GenericLayer)                        │  │
│    │  Input: Prediction                                     │  │
│    │  Output: "QUALITY_SCORE: 9\n                           │  │
│    │           GOAL_PROGRESS: 100%\n..."                    │  │
│    └──────────────────────┬─────────────────────────────────┘  │
│                            │                                     │
│                            ▼                                     │
│    ┌────────────────────────────────────────────────────────┐  │
│    │  Execute Best Proposal                                 │  │
│    │  (Apply code changes)                                  │  │
│    └──────────────────────┬─────────────────────────────────┘  │
│                            │                                     │
│                            ▼                                     │
│    ┌────────────────────────────────────────────────────────┐  │
│    │  Orchestrator Layer (GenericLayer)                     │  │
│    │  Input: Subgoal + changes made                         │  │
│    │  Output: "COMPLETE"                                    │  │
│    └────────────────────────────────────────────────────────┘  │
│                                                                  │
│  • Repeat for all subgoals                                      │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                         OUTPUT                                   │
│  "Task completed successfully. All subgoals achieved."          │
└─────────────────────────────────────────────────────────────────┘
```

## 5. Feedback Loop Detail

```
┌─────────────────────────────────────────────────────────────────┐
│                    ACTOR-MONITOR FEEDBACK LOOP                   │
│                                                                  │
│  Iteration 1:                                                    │
│  ┌────────────────┐                                             │
│  │  ACTOR         │                                             │
│  │  Proposes:     │                                             │
│  │  "Add jwt=9.0" │                                             │
│  └────────┬───────┘                                             │
│           │                                                      │
│           ▼                                                      │
│  ┌────────────────┐                                             │
│  │  MONITOR       │                                             │
│  │  Validates:    │                                             │
│  │  "INVALID:     │                                             │
│  │   Version 9.0  │                                             │
│  │   doesn't exist│                                             │
│  │   Use 9.2"     │                                             │
│  └────────┬───────┘                                             │
│           │                                                      │
│           │ Feedback                                            │
│           │                                                      │
│  Iteration 2:                                                    │
│           │                                                      │
│           ▼                                                      │
│  ┌────────────────┐                                             │
│  │  ACTOR         │                                             │
│  │  Proposes:     │                                             │
│  │  "Add jwt=9.2" │                                             │
│  └────────┬───────┘                                             │
│           │                                                      │
│           ▼                                                      │
│  ┌────────────────┐                                             │
│  │  MONITOR       │                                             │
│  │  Validates:    │                                             │
│  │  "VALID"       │                                             │
│  └────────┬───────┘                                             │
│           │                                                      │
│           ▼                                                      │
│     Continue to                                                  │
│     Tree Search                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## 6. Configuration Structure

```
config-templates/default.toml
│
├── [[layers.map_monitor]]
│   ├── description
│   ├── model
│   ├── temperature
│   ├── system_prompt
│   └── [mcp]
│       ├── server_refs
│       └── allowed_tools
│
├── [[layers.map_actor]]
│   └── ... (same structure)
│
├── [[layers.map_predictor]]
│   └── ... (same structure)
│
├── [[layers.map_evaluator]]
│   └── ... (same structure)
│
├── [[layers.map_task_decomposer]]
│   └── ... (same structure)
│
├── [[layers.map_orchestrator]]
│   └── ... (same structure)
│
├── [[layers.map_main_orchestrator]]
│   └── ... (same structure)
│
└── [[commands]]
    ├── name = "map"
    ├── description
    └── layer_name = "map_main_orchestrator"
```

## 7. Comparison: Standard vs MAP

```
┌─────────────────────────────────────────────────────────────────┐
│                    STANDARD LLM APPROACH                         │
│                                                                  │
│  User Task                                                       │
│      ↓                                                           │
│  Single LLM Call                                                 │
│      ↓                                                           │
│  Generated Code                                                  │
│      ↓                                                           │
│  Problems:                                                       │
│  • May hallucinate invalid code                                 │
│  • Poor multi-step planning                                     │
│  • No validation before execution                               │
│  • Unclear when task is complete                                │
│  • Black box process                                            │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                      MAP APPROACH                                │
│                                                                  │
│  User Task                                                       │
│      ↓                                                           │
│  TaskDecomposer (breaks into subgoals)                          │
│      ↓                                                           │
│  For each subgoal:                                              │
│      Actor (proposes solutions)                                 │
│      ↓                                                           │
│      Monitor (validates - feedback loop)                        │
│      ↓                                                           │
│      Predictor (predicts impact)                                │
│      ↓                                                           │
│      Evaluator (scores quality)                                 │
│      ↓                                                           │
│      Execute best proposal                                      │
│      ↓                                                           │
│      Orchestrator (verifies completion)                         │
│      ↓                                                           │
│  Final Verification                                             │
│      ↓                                                           │
│  Benefits:                                                       │
│  • Monitor catches invalid code                                 │
│  • Excellent multi-step planning                                │
│  • Validation before execution                                  │
│  • Clear completion criteria                                    │
│  • Transparent, debuggable process                              │
└─────────────────────────────────────────────────────────────────┘
```

---

These diagrams illustrate the MAP architecture and how it integrates with Octomind's existing layer system. The key insight is that Octomind's flexible layer system is already perfect for implementing MAP - we just need to add the coordination logic and configure the specialized modules.
