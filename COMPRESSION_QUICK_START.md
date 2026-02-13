# 🚀 SOTA Compression: Quick Start Guide

## TL;DR
Implement state-of-the-art context compression in **3-4 days** using EDU-inspired techniques with **zero ML training**.

---

## 📊 What You're Getting

| Metric | Current | After Implementation | Improvement |
|--------|---------|---------------------|-------------|
| Compression Ratio | 2-3x | 3-5x | **+67-100%** |
| Information Preservation | ~85% | 95%+ | **+12%** |
| Technical Accuracy | ~90% | 100% | **+11%** |
| Compression Trigger | Fixed turns | Adaptive pressure | **Proactive** |
| Processing Time | ~2s | <2s | **Same or better** |

---

## 🎯 Day 1: Quick Wins (4.5 hours)

### Morning: Adaptive Compression (4 hours)

**Add to `config-templates/default.toml`:**
```toml
[compression]
adaptive_threshold = true
pressure_trigger = 0.25  # Compress at 25% context usage

[[compression.pressure_levels]]
threshold = 0.25
target_ratio = 2.0  # Light compression

[[compression.pressure_levels]]
threshold = 0.50
target_ratio = 4.0  # Medium compression

[[compression.pressure_levels]]
threshold = 0.75
target_ratio = 8.0  # Aggressive compression
```

**Modify `src/session/chat/conversation_compression.rs`:**
```rust
pub fn should_check_compression(session: &ChatSession, config: &Config) -> (bool, f64) {
    if !config.compression.adaptive_threshold {
        let turns = count_conversation_turns(&session.session.messages);
        return (turns >= config.compression.min_conversation_turns, 2.0);
    }

    let max_tokens = config.max_session_tokens_threshold;
    if max_tokens == 0 {
        return (false, 2.0);
    }

    let current_tokens = session.current_total_tokens;
    let pressure = current_tokens as f64 / max_tokens as f64;

    let target_ratio = config.compression.pressure_levels
        .iter()
        .rev()
        .find(|level| pressure >= level.threshold)
        .map(|level| level.ratio)
        .unwrap_or(2.0);

    let should_compress = pressure >= config.compression.pressure_trigger;

    if should_compress {
        crate::log_debug!(
            "Context pressure: {:.1}% → target compression: {:.1}x",
            pressure * 100.0,
            target_ratio
        );
    }

    (should_compress, target_ratio)
}
```

**Update `check_and_compress_conversation()`:**
```rust
pub async fn check_and_compress_conversation(
    session: &mut ChatSession,
    config: &Config,
) -> Result<()> {
    let (should_compress, target_ratio) = should_check_compression(session, config);

    if !should_compress {
        return Ok(());
    }

    // Use target_ratio for compression aggressiveness
    compress_older_conversation(session, config, target_ratio).await
}
```

**Test:**
```bash
cargo check --message-format=short
cargo build
octomind session
# Try long conversation, watch compression trigger at 25%
```

---

### Afternoon: AI Compression Hints (30 minutes)

**Modify `src/session/chat/session/core.rs`:**
```rust
// In system prompt building function
pub fn build_system_prompt(&self, config: &Config) -> String {
    let mut prompt = self.role.system_prompt.clone();

    if self.session.info.compression_stats.total_compressions() > 0 {
        let stats = &self.session.info.compression_stats;
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

    prompt
}
```

**Test:**
```bash
cargo build
octomind session
# Trigger compression, ask AI about earlier conversation
# Verify AI acknowledges compression and doesn't hallucinate
```

---

## 🧩 Day 2-3: Semantic Chunking (1-2 days)

### Create `src/session/chat/semantic_chunking.rs`

**Core structures:**
```rust
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
```

**Core functions:**
```rust
pub fn chunk_messages(messages: &[Message]) -> Vec<SemanticChunk> {
    let mut chunks = Vec::new();

    for (idx, msg) in messages.iter().enumerate() {
        let segments = split_by_semantic_boundaries(&msg.content);

        for segment in segments {
            let chunk_type = classify_chunk(&segment);
            let importance = calculate_importance(&segment, &chunk_type, msg);

            chunks.push(SemanticChunk {
                content: segment,
                start_idx: idx,
                end_idx: idx,
                importance_score: importance,
                chunk_type,
            });
        }
    }

    chunks
}

fn classify_chunk(text: &str) -> ChunkType {
    let lower = text.to_lowercase();

    // Technical indicators
    if text.contains("src/") || text.contains("error:") ||
       text.contains("```") || text.contains("fn ") {
        return ChunkType::Technical;
    }

    // Decision indicators
    if lower.contains("decided") || lower.contains("will") ||
       lower.contains("should") || lower.contains("plan") {
        return ChunkType::Decision;
    }

    // Filler indicators
    if lower.starts_with("ok") || lower.starts_with("sure") ||
       lower.starts_with("thanks") {
        return ChunkType::Filler;
    }

    ChunkType::Context
}

fn calculate_importance(text: &str, chunk_type: &ChunkType, msg: &Message) -> f64 {
    let mut score = match chunk_type {
        ChunkType::Technical => 10.0,
        ChunkType::Decision => 8.0,
        ChunkType::Context => 5.0,
        ChunkType::Filler => 1.0,
    };

    // Boost for specific patterns
    if text.contains("error") || text.contains("Error") {
        score += 5.0;
    }
    if text.contains('/') && text.contains('.') {
        score += 3.0;  // Likely file path
    }
    if msg.tool_calls.is_some() {
        score += 4.0;
    }

    // Recency decay (exponential)
    let age_hours = calculate_age_hours(msg.timestamp);
    score *= (-age_hours / 24.0).exp();  // Half-life of 24 hours

    score
}

pub fn select_chunks_within_budget(
    chunks: &[SemanticChunk],
    budget: usize,
) -> Vec<SemanticChunk> {
    let mut sorted = chunks.to_vec();
    sorted.sort_by(|a, b| b.importance_score.partial_cmp(&a.importance_score).unwrap());

    let mut selected = Vec::new();
    let mut total_tokens = 0;

    for chunk in sorted {
        let chunk_tokens = estimate_tokens(&chunk.content);
        if total_tokens + chunk_tokens <= budget {
            selected.push(chunk);
            total_tokens += chunk_tokens;
        }
    }

    selected
}
```

**Test:**
```bash
cargo test --lib semantic_chunking
```

---

### Enhanced Structured Prompts (2 hours)

**Update `generate_conversation_summary()` in `conversation_compression.rs`:**
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

**Update `parse_structured_summary()`:**
```rust
fn parse_structured_summary(response: &str) -> (String, String, String, String) {
    let technical = extract_section(response, "**TECHNICAL**", "**DECISIONS**");
    let decisions = extract_section(response, "**DECISIONS**", "**CONTEXT**");
    let context = extract_section(response, "**CONTEXT**", "**NEXT STEPS**");
    let next_steps = extract_section(response, "**NEXT STEPS**", "");

    (technical, decisions, context, next_steps)
}
```

---

### Integration (4 hours)

**Create `compress_older_conversation_v2()` in `conversation_compression.rs`:**
```rust
async fn compress_older_conversation_v2(
    session: &mut ChatSession,
    config: &Config,
    target_ratio: f64,
) -> Result<()> {
    // 1. Chunk messages semantically
    let chunks = chunk_messages(&session.session.messages);

    // 2. Sort by importance
    let mut sorted_chunks = chunks.clone();
    sorted_chunks.sort_by(|a, b| b.importance_score.partial_cmp(&a.importance_score).unwrap());

    // 3. Calculate target tokens
    let current_tokens = estimate_full_context_tokens(&session.session.messages, None, None);
    let target_tokens = (current_tokens as f64 / target_ratio) as usize;

    // 4. Select top chunks within budget
    let selected = select_chunks_within_budget(&sorted_chunks, target_tokens);

    // 5. Separate by type
    let technical_chunks: Vec<_> = selected.iter()
        .filter(|c| matches!(c.chunk_type, ChunkType::Technical))
        .collect();
    let decision_chunks: Vec<_> = selected.iter()
        .filter(|c| matches!(c.chunk_type, ChunkType::Decision))
        .collect();
    let context_chunks: Vec<_> = selected.iter()
        .filter(|c| matches!(c.chunk_type, ChunkType::Context))
        .collect();

    // 6. Format output
    let technical_text = format_technical_chunks(&technical_chunks);
    let decisions_text = format_decision_chunks(&decision_chunks);
    let context_summary = summarize_chunks(&context_chunks, session, config).await?;

    let compressed = format!(
        "## Conversation Summary [COMPRESSED]\n\n\
        **TECHNICAL** (preserved verbatim):\n{}\n\n\
        **DECISIONS**:\n{}\n\n\
        **CONTEXT**: {}\n",
        technical_text, decisions_text, context_summary
    );

    // 7. Replace old messages
    let (start_idx, end_idx) = find_compression_range(&session.session.messages)?;
    session.remove_messages_in_range(start_idx, end_idx)?;
    session.insert_compressed_knowledge(start_idx, compressed)?;

    Ok(())
}
```

**Add config option:**
```toml
[compression]
use_semantic_chunking = true
```

**Test:**
```bash
cargo test --test compression_integration_tests
cargo build
octomind session
# Test with technical conversation, verify verbatim preservation
```

---

## 📋 Validation Checklist

### Day 1 Validation
- [ ] Compression triggers at 25% context usage
- [ ] Compression aggressiveness scales with pressure (2x → 4x → 8x)
- [ ] Debug logs show pressure % and target ratio
- [ ] AI acknowledges compression in responses
- [ ] No hallucination about "missing" information

### Day 2-3 Validation
- [ ] Semantic chunking correctly classifies message types
- [ ] Technical content scores higher than filler
- [ ] Importance scoring includes recency decay
- [ ] Structured prompts preserve technical details verbatim
- [ ] Integration tests pass
- [ ] Compression ratio: 3-5x
- [ ] Information preservation: 95%+

---

## 🎯 Success Criteria

### Technical
- ✅ Compression ratio: 3-5x (vs current 2-3x)
- ✅ Information preservation: 95%+ (vs ~85%)
- ✅ Technical accuracy: 100% (verbatim)
- ✅ Processing time: <2s per 100 messages

### Quality
- ✅ No user complaints about lost context
- ✅ AI reasoning improves with compressed context
- ✅ Technical details never paraphrased
- ✅ Important old messages preserved

### Business
- ✅ 60-80% token reduction
- ✅ Cost savings on long sessions
- ✅ No performance degradation
- ✅ Production-ready in 3-4 days

---

## 🚨 Common Issues & Fixes

### Issue: Compression not triggering
**Fix:** Check `max_session_tokens_threshold` is set (not 0)

### Issue: Technical details lost
**Fix:** Verify `use_semantic_chunking = true` and check chunk classification

### Issue: Poor compression quality
**Fix:** Adjust `target_ratio` in pressure_levels or enable quality evaluation

### Issue: Performance slow
**Fix:** Reduce `preserve_important_count` or use faster `decision_model`

---

## 📚 Next Steps

After Day 3, continue with:
- **Week 2:** Importance-weighted sliding window (4h)
- **Week 3:** Quality metrics & documentation (1.5d)
- **Week 4:** Testing & gradual rollout (1.5d)

See `COMPRESSION_PLAN.md` for full details.

---

## 🎉 You're Ready!

Start with Day 1 and you'll have a working SOTA compression system by Day 3-4.

**Questions?** Check `COMPRESSION_PLAN.md` for detailed explanations.

**Let's build! 🚀**
