# SOTA Context Compression Implementation Plan
## EDU-Inspired 80/20 Approach

**Status:** Ready for implementation
**Timeline:** 4 weeks (3-4 days core features, rest polish & rollout)
**Expected Impact:** 3-5x compression, 95%+ information preservation, zero hallucination on technical details

---

## 📊 Executive Summary

### What We're Building
State-of-the-art context compression system inspired by the latest EDU (Elementary Discourse Units) research, but implemented with pragmatic heuristics instead of ML training.

### Why This Approach (80/20 Rule)
- **80% of benefits** from EDU research
- **20% of implementation effort** (no ML training required)
- **Production-ready in 3-4 days** (core features)
- **Zero overengineering** - pure heuristics

### Key Innovations
1. **Adaptive pressure-based triggering** (compress at 25% context usage, not 85%)
2. **Semantic chunking** (variable-length meaningful units, not fixed tokens)
3. **Importance scoring** (preserve what matters, not just recent)
4. **Structured preservation** (verbatim technical details, zero hallucination)
5. **AI transparency** (system prompt hints about compression state)

### Expected Results
- **Compression ratio:** 3-5x (vs current 2-3x)
- **Information preservation:** 95%+ (vs ~85%)
- **Technical accuracy:** 100% (verbatim preservation)
- **Processing time:** <2s per 100 messages
- **Cost:** Minimal (uses existing decision_model)

---

## 🎯 Implementation Phases

### **WEEK 1: Core Features (Day 1-4)**

#### Phase 1: Adaptive Compression Threshold ⚡ (Day 1 - 4 hours)
**Priority:** P0 - Critical foundation

**What:** Replace fixed turn-based triggering with dynamic pressure-based triggering.

**Research Basis:** Factory.ai research shows compression at 25% context usage (not 85%) yields better results.

**Implementation:**
```toml
# config-templates/default.toml
[compression]
adaptive_threshold = true
pressure_trigger = 0.25  # Compress at 25% context usage

[[compression.pressure_levels]]
threshold = 0.25
target_ratio = 2.0  # Light: 50% reduction

[[compression.pressure_levels]]
threshold = 0.50
target_ratio = 4.0  # Medium: 75% reduction

[[compression.pressure_levels]]
threshold = 0.75
target_ratio = 8.0  # Aggressive: 87.5% reduction
```

**Files to modify:**
- `config-templates/default.toml` - Add configuration
- `src/config/mod.rs` - Add config structs
- `src/session/chat/conversation_compression.rs` - Modify `should_check_compression()`

**Testing:**
- Sessions at 10%, 30%, 60%, 80% context usage
- Verify progressive compression behavior
- Check debug logging shows pressure % and target ratio

**Success criteria:**
- ✅ Compression triggers at 25% context usage
- ✅ Compression aggressiveness scales with pressure
- ✅ No compression before threshold
- ✅ Prevents context overflow

---

#### Phase 2: Compression Context Hints to AI 💡 (Day 1 - 30 minutes)
**Priority:** P0 - Quick win

**What:** Add transparency metadata to system prompt informing AI about compression state.

**Research Basis:** AI reasoning improves when aware of compressed context.

**Implementation:**
```rust
// In src/session/chat/session/core.rs
if self.session.info.compression_stats.total_compressions() > 0 {
    prompt.push_str(&format!(
        "\n\n## CONTEXT COMPRESSION ACTIVE\n\
        - {} compressions performed\n\
        - {} tokens saved ({:.1}% reduction)\n\
        - Compressed sections marked with [COMPRESSED: id]\n\
        - Technical details preserved verbatim in TECHNICAL sections\n\
        - Focus on recent uncompressed messages for current context",
        stats.total_compressions(),
        stats.total_tokens_saved,
        stats.avg_compression_ratio() * 100.0
    ));
}
```

**Files to modify:**
- `src/session/chat/session/core.rs` - System prompt building

**Testing:**
- Trigger compression in session
- Verify AI acknowledges compression in responses
- Check AI doesn't hallucinate about "missing" information

**Success criteria:**
- ✅ AI understands compression state
- ✅ Better reasoning with compressed context
- ✅ Reduced confusion about missing information

---

#### Phase 3: Semantic Chunking Module 🧩 (Day 2-3 - 1-2 days)
**Priority:** P1 - Core innovation

**What:** Create EDU-inspired semantic chunking with importance scoring using pure heuristics.

**Research Basis:** EDU paper shows semantic units > fixed tokens. Extractive methods with reranking achieve +7.89 F1 at 4.5x compression.

**Implementation:**
```rust
// New file: src/session/chat/semantic_chunking.rs

pub struct SemanticChunk {
    pub content: String,
    pub start_idx: usize,
    pub end_idx: usize,
    pub importance_score: f64,
    pub chunk_type: ChunkType,
}

pub enum ChunkType {
    Technical,   // File paths, commands, errors
    Decision,    // Key decisions, conclusions
    Context,     // Background information
    Filler,      // Conversational noise
}

// Core functions:
pub fn chunk_messages(messages: &[Message]) -> Vec<SemanticChunk>
fn classify_chunk(text: &str) -> ChunkType
fn calculate_importance(text: &str, chunk_type: &ChunkType, msg: &Message) -> f64
pub fn select_chunks_within_budget(chunks: &[SemanticChunk], budget: usize) -> Vec<SemanticChunk>
```

**Classification heuristics:**
- **Technical:** Contains `src/`, `error:`, code blocks, function definitions
- **Decision:** Contains `decided`, `will`, `should`, `plan`
- **Filler:** Starts with `ok`, `sure`, `thanks`
- **Context:** Everything else

**Importance scoring:**
- Base scores: Technical=10, Decision=8, Context=5, Filler=1
- Boosts: Errors +5, File paths +3, Tool calls +4
- Temporal decay: `score *= exp(-age_hours / 24.0)` (24-hour half-life)

**Files to create:**
- `src/session/chat/semantic_chunking.rs` - New module

**Testing:**
- Unit tests for each function
- Various message types (technical, conversational, mixed)
- Verify importance scoring accuracy

**Success criteria:**
- ✅ Intelligent chunk selection
- ✅ Technical content scores higher than filler
- ✅ 3-5x compression ratio
- ✅ Zero ML dependencies

---

#### Phase 4: Structured Extraction Templates 📋 (Day 3 - 2 hours)
**Priority:** P1 - Quality improvement

**What:** Enhance compression prompts with structured output format.

**Research Basis:** EDU paper shows structured output preserves 95%+ information vs free-form summarization.

**Implementation:**
```rust
let summary_prompt = format!(
    "Compress the conversation using this EXACT structure:\n\n\
    **TECHNICAL** (preserve verbatim - copy exact text):\n\
    - File paths: [list]\n\
    - Commands: [list]\n\
    - Errors: [list]\n\
    - Code changes: [list]\n\n\
    **DECISIONS** (preserve verbatim):\n\
    - [list key decisions with exact wording]\n\n\
    **CONTEXT** (summarize in 2-3 sentences):\n\
    [narrative summary]\n\n\
    **NEXT STEPS** (if any):\n\
    - [list]\n\n\
    CRITICAL: For TECHNICAL and DECISIONS sections, copy exact text from conversation. \
    Do NOT paraphrase technical details.\n\n\
    Conversation:\n{}\n\n\
    Output:",
    conversation_text
);
```

**Files to modify:**
- `src/session/chat/conversation_compression.rs` - Update `generate_conversation_summary()` and `parse_structured_summary()`

**Testing:**
- Conversations with technical details (file paths, errors, commands)
- Verify verbatim preservation vs paraphrasing
- Compare quality before/after

**Success criteria:**
- ✅ Verbatim preservation of technical info
- ✅ Structured retrieval capability
- ✅ 90% of EDU structure benefits
- ✅ Reduced hallucination on technical details

---

#### Phase 5: Integrate Semantic Chunking 🔗 (Day 3-4 - 4 hours)
**Priority:** P1 - Integration

**What:** Replace current message-range compression with semantic chunk-based compression.

**Implementation:**
```rust
// In src/session/chat/conversation_compression.rs

async fn compress_older_conversation_v2(
    session: &mut ChatSession,
    config: &Config,
) -> Result<()> {
    // 1. Chunk messages semantically
    let chunks = chunk_messages(&session.session.messages);

    // 2. Sort by importance
    let mut sorted_chunks = chunks.clone();
    sorted_chunks.sort_by(|a, b| b.importance_score.partial_cmp(&a.importance_score).unwrap());

    // 3. Select top chunks within budget
    let target_tokens = calculate_target_tokens(config, session);
    let selected = select_chunks_within_budget(&sorted_chunks, target_tokens);

    // 4. Separate by type
    let technical_chunks = selected.iter().filter(|c| matches!(c.chunk_type, ChunkType::Technical));
    let decision_chunks = selected.iter().filter(|c| matches!(c.chunk_type, ChunkType::Decision));
    let context_chunks = selected.iter().filter(|c| matches!(c.chunk_type, ChunkType::Context));

    // 5. Preserve technical verbatim, summarize context
    let technical_text = format_technical_chunks(&technical_chunks);
    let context_summary = summarize_chunks(&context_chunks, session, config).await?;
    let decisions_text = format_decision_chunks(&decision_chunks);

    // 6. Format compressed output
    let compressed = format!(
        "## Conversation Summary [COMPRESSED]\n\n\
        **TECHNICAL** (preserved verbatim):\n{}\n\n\
        **CONTEXT**: {}\n\n\
        **DECISIONS**: {}\n",
        technical_text, context_summary, decisions_text
    );

    // 7. Replace old messages
    session.remove_messages_in_range(start_idx, end_idx)?;
    session.insert_compressed_knowledge(start_idx, compressed)?;

    Ok(())
}
```

**Configuration:**
```toml
[compression]
use_semantic_chunking = true  # Enable new system
```

**Files to modify:**
- `src/session/chat/conversation_compression.rs` - Add `compress_older_conversation_v2()`
- `config-templates/default.toml` - Add config option

**Testing:**
- Integration tests comparing old vs new compression
- Measure compression ratio, information preservation, processing time
- Real session data validation

**Success criteria:**
- ✅ Production-ready semantic compression
- ✅ 3-5x compression with 95%+ information retention
- ✅ Seamless integration with existing system
- ✅ Fallback to old method if disabled

---

### **WEEK 2: Advanced Features**

#### Phase 6: Importance-Weighted Sliding Window 🎯 (Week 2 - 4 hours)
**Priority:** P2 - Enhancement

**What:** Keep recent + important messages (not just recent).

**Implementation:**
```rust
pub fn select_messages_for_compression(
    messages: &[Message],
    config: &Config,
) -> (Vec<usize>, Vec<usize>) {  // (preserve_indices, compress_indices)
    // 1. Always preserve last N messages
    let preserve_recent = config.compression.preserve_recent_count;

    // 2. Score all older messages
    let mut scored: Vec<(usize, f64)> = messages[..messages.len()-preserve_recent]
        .iter()
        .enumerate()
        .map(|(idx, msg)| (idx, calculate_message_importance(msg)))
        .collect();

    // 3. Select top K important messages
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let preserve_important: Vec<usize> = scored
        .iter()
        .take(config.compression.preserve_important_count)
        .map(|(idx, _)| *idx)
        .collect();

    // 4. Merge preserved sets
    let mut preserve_indices = preserve_important;
    preserve_indices.extend((messages.len()-preserve_recent)..messages.len());
    preserve_indices.sort();

    // 5. Compress remaining
    let compress_indices: Vec<usize> = (0..messages.len())
        .filter(|idx| !preserve_indices.contains(idx))
        .collect();

    (preserve_indices, compress_indices)
}
```

**Configuration:**
```toml
[compression]
preserve_recent_count = 4      # Recent messages to keep
preserve_important_count = 3   # Important old messages to keep
importance_decay_hours = 24.0  # Temporal decay rate
```

**Success criteria:**
- ✅ Critical old information preserved
- ✅ Better than pure recency-based selection
- ✅ Prevents loss of important context

---

### **WEEK 3: Quality & Documentation**

#### Phase 7: Compression Quality Metrics 📊 (Week 3 - 1 day)
**Priority:** P3 - Feedback loop

**What:** Implement feedback loop to measure and improve compression quality.

**Implementation:**
```rust
// New file: src/session/chat/compression_quality.rs

pub struct QualityScores {
    pub preservation: f64,  // 0-10: Critical facts retained?
    pub clarity: f64,       // 0-10: Understandable summary?
    pub conciseness: f64,   // 0-10: No redundancy?
    pub overall: f64,       // Weighted average
}

pub async fn evaluate_compression_quality(
    original: &str,
    compressed: &str,
    session: &mut ChatSession,
) -> Result<QualityScores> {
    let eval_prompt = format!(
        "Rate this compression on 3 criteria (0-10):\n\
        1. Information preservation (critical facts retained?)\n\
        2. Clarity (understandable summary?)\n\
        3. Conciseness (no redundancy?)\n\n\
        Original ({} tokens):\n{}\n\n\
        Compressed ({} tokens):\n{}\n\n\
        Output JSON: {{\"preservation\": X, \"clarity\": Y, \"conciseness\": Z}}",
        estimate_tokens(original), original,
        estimate_tokens(compressed), compressed
    );

    // Use decision_model for cost efficiency
    let response = call_decision_model(eval_prompt, session).await?;
    let scores: QualityScores = parse_quality_scores(&response)?;

    // Calculate overall: preservation=50%, clarity=30%, conciseness=20%
    scores.overall = scores.preservation * 0.5 + scores.clarity * 0.3 + scores.conciseness * 0.2;

    // Adaptive adjustment
    if scores.preservation < 7.0 {
        // Reduce compression ratio
    }

    Ok(scores)
}
```

**Configuration:**
```toml
[compression]
enable_quality_evaluation = false  # Default: disabled in production
```

**Success criteria:**
- ✅ Measurable compression quality
- ✅ Automatic quality-based adjustments
- ✅ Continuous improvement feedback loop

---

#### Phase 8: Documentation & Configuration Guide 📚 (Week 3 - 2 hours)
**Priority:** P2 - Essential

**What:** Comprehensive documentation for users and developers.

**Deliverables:**
1. **INSTRUCTIONS.md** - New section "Context Compression System"
2. **config-templates/default.toml** - Detailed comments for all options
3. **doc/compression-system.md** - Architecture overview, tuning guide
4. **Inline rustdoc** - All public functions documented
5. **/help command** - Mention compression system

**Success criteria:**
- ✅ Clear documentation for users and developers
- ✅ Easy configuration tuning
- ✅ Troubleshooting guide

---

### **WEEK 4: Testing & Rollout**

#### Phase 9: Testing & Validation ✅ (Week 3-4 - 1 day)
**Priority:** P1 - Quality assurance

**What:** Comprehensive testing to ensure production readiness.

**Test Coverage:**
1. **Unit tests** - All functions in semantic_chunking.rs
2. **Integration tests** - End-to-end compression pipeline
3. **Benchmark tests** - Performance characteristics
4. **Edge cases** - Empty messages, very long messages, rapid cycles
5. **Real data** - 10 diverse sessions from production

**Performance targets:**
- Compression ratio: 3-5x
- Processing time: <2s for 100 messages
- Information preservation: 95%+ (manual evaluation)
- Cost per compression: <$0.01

**Success criteria:**
- ✅ Production-ready compression system
- ✅ Validated performance characteristics
- ✅ Comprehensive test coverage
- ✅ Documented edge cases and limitations

---

#### Phase 10: Rollout & Monitoring 🚀 (Week 4 - 4 hours)
**Priority:** P1 - Safe deployment

**What:** Gradual rollout with comprehensive monitoring.

**Rollout Plan:**
- **Week 1:** 10% of sessions with close monitoring
- **Week 2:** 25% if no issues
- **Week 3:** 50% if metrics look good
- **Week 4:** 100% full rollout

**Configuration:**
```toml
[compression]
rollout_percentage = 10  # 0-100: Gradual rollout
emergency_disable = false  # Quick kill switch
```

**Monitoring Metrics:**
- Compression success rate (target: >99%)
- Average compression ratio (target: 3-5x)
- User-reported issues (target: 0)
- Performance impact (target: <5% latency increase)
- Cost savings (track token reduction)

**Success criteria:**
- ✅ Safe production rollout
- ✅ Comprehensive monitoring
- ✅ Quick rollback capability
- ✅ Validated real-world performance

---

## 📈 Success Metrics

### Technical Metrics
- **Compression ratio:** 3-5x (vs current 2-3x)
- **Information preservation:** 95%+ (vs ~85%)
- **Technical accuracy:** 100% (verbatim preservation)
- **Processing time:** <2s per 100 messages
- **Cost per compression:** <$0.01

### Quality Metrics
- **Preservation score:** >8.0/10
- **Clarity score:** >7.5/10
- **Conciseness score:** >7.5/10
- **Overall quality:** >8.0/10

### Business Metrics
- **Token savings:** 60-80% reduction
- **Cost savings:** $X per 1M tokens
- **User satisfaction:** No complaints about lost context
- **System stability:** >99% compression success rate

---

## 🎯 Quick Start Guide

### Day 1 (4.5 hours)
```bash
# Morning: Phase 1 - Adaptive Compression Threshold (4 hours)
# 1. Update config-templates/default.toml
# 2. Modify src/session/chat/conversation_compression.rs
# 3. Test with sessions at different pressure levels

# Afternoon: Phase 2 - Compression Context Hints (30 minutes)
# 1. Modify src/session/chat/session/core.rs
# 2. Test compression awareness in AI responses

cargo check --message-format=short
cargo clippy --all-features --all-targets -- -D warnings
cargo build
```

### Day 2-3 (1-2 days)
```bash
# Phase 3: Semantic Chunking Module
# 1. Create src/session/chat/semantic_chunking.rs
# 2. Implement all core functions
# 3. Write comprehensive unit tests

# Phase 4: Structured Extraction Templates (2 hours)
# 1. Update compression prompts
# 2. Test verbatim preservation

cargo test --lib semantic_chunking
```

### Day 3-4 (4 hours)
```bash
# Phase 5: Integration
# 1. Create compress_older_conversation_v2()
# 2. Add configuration options
# 3. Integration tests

cargo test --test compression_integration_tests
```

---

## 🔧 Configuration Reference

```toml
[compression]
# Core settings
adaptive_threshold = true
pressure_trigger = 0.25
min_conversation_turns = 0  # Disabled when adaptive_threshold = true
use_semantic_chunking = true

# Compression aggressiveness
[[compression.pressure_levels]]
threshold = 0.25
target_ratio = 2.0

[[compression.pressure_levels]]
threshold = 0.50
target_ratio = 4.0

[[compression.pressure_levels]]
threshold = 0.75
target_ratio = 8.0

# Message selection
preserve_recent_count = 4
preserve_important_count = 3
importance_decay_hours = 24.0

# Quality & monitoring
enable_quality_evaluation = false
rollout_percentage = 100
emergency_disable = false

# Cost optimization
decision_model = "openrouter:anthropic/claude-haiku"
```

---

## 🚨 Troubleshooting

### Compression not triggering
- Check `max_session_tokens_threshold` is set (not 0)
- Verify `adaptive_threshold = true`
- Check current context pressure: `current_tokens / max_tokens`

### Poor compression quality
- Enable quality evaluation: `enable_quality_evaluation = true`
- Check preservation scores in `/info` command
- Adjust `target_ratio` in pressure_levels

### Technical details lost
- Verify `use_semantic_chunking = true`
- Check chunk classification in debug logs
- Ensure structured extraction template is used

### Performance issues
- Reduce `preserve_important_count`
- Increase `pressure_trigger` (compress less frequently)
- Use faster `decision_model`

---

## 📚 References

### Research Papers
1. **"From Context to EDUs"** (Dec 2025, arXiv:2512.14244)
   - 49.60% structural accuracy, 4.77 TED
   - +51.11% improvement on complex reasoning
   - 10x faster, 24x cheaper than GPT-4.1

2. **Factory.ai Context Compression** (2024)
   - Compression at 25% threshold (not 85%)
   - Structured summarization > aggressive truncation

3. **LLMLingua-2** (Microsoft, ACL 2024)
   - 14-20x compression via token-level filtering
   - Task-agnostic prompt compression

4. **Extractive Reranker-Based Compression** (2024)
   - +7.89 F1 at 4.5x compression
   - Compression improves accuracy by filtering noise

### Implementation Principles
- **80/20 Rule:** Maximum impact, minimum effort
- **No ML Training:** Pure heuristics
- **Zero Overengineering:** Pragmatic approach
- **Production-First:** Safe rollout, monitoring, rollback

---

## ✅ Checklist

### Week 1: Core Features
- [ ] Phase 1: Adaptive Compression Threshold (4h)
- [ ] Phase 2: Compression Context Hints (30m)
- [ ] Phase 3: Semantic Chunking Module (1-2d)
- [ ] Phase 4: Structured Extraction Templates (2h)
- [ ] Phase 5: Integration (4h)

### Week 2: Advanced Features
- [ ] Phase 6: Importance-Weighted Sliding Window (4h)

### Week 3: Quality & Documentation
- [ ] Phase 7: Compression Quality Metrics (1d)
- [ ] Phase 8: Documentation & Configuration Guide (2h)

### Week 4: Testing & Rollout
- [ ] Phase 9: Testing & Validation (1d)
- [ ] Phase 10: Rollout & Monitoring (4h)

---

**Ready to proceed? Start with Phase 1!** 🚀
