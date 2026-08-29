//! MiMo-V2.5-TTS smart chunker (ADR-005).
//!
//! Official limits (https://mimo.mi.com/docs/zh-CN/quick-start/summary/model):
//! context window **8K tokens**, max output 8K. The platform exposes no remote
//! tokenize endpoint, so we use a *single* calibrated over-estimator plus a
//! safety margin; on a context-overflow 400 the engine re-chunks at ×0.8
//! (ADR-013 self-heal, applied by the engine, not here).

/// Official context window.
pub const CONTEXT_WINDOW_TOKENS: i64 = 8000;
/// Default budget per chunk: 6000 tokens (>= 12% headroom for user-message
/// style instructions, inline tags and estimator error).
pub const DEFAULT_TARGET_TOKENS: i64 = 6000;
/// Hard cap: a single sentence beyond this gets force-split at clause level.
pub const DEFAULT_HARD_CAP_TOKENS: i64 = 7500;
/// Self-heal scale factor on context-overflow (ADR-013).
pub const OVERFLOW_RECHUNK_SCALE: f64 = 0.8;

/// Calibrated single estimator weights (ADR-005: over-estimate, not under).
/// CJK ≈ 2.0 tokens/char; other non-space chars ≈ 1.2. Whitespace is free.
const CJK_WEIGHT: f64 = 2.0;
const OTHER_WEIGHT: f64 = 1.2;

/// Sentence / clause delimiters. Kept attached to the sentence (no loss).
const SENTENCE_DELIMS: [char; 8] = ['。', '！', '？', '…', '.', '!', '?', '\n'];
const CLAUSE_DELIMS: [char; 7] = ['，', ',', '、', '；', ';', '：', ':'];

/// One chunk ready for a synthesis request.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkSegment {
    pub text: String,
    pub char_count: i64,
    pub token_estimate: i64,
    /// Style instructions for this chunk's `user` message. Carried on EVERY
    /// chunk: each request is independent (no cross-request state).
    pub style_hint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkConfig {
    pub target_tokens: i64,
    pub hard_cap_tokens: i64,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            target_tokens: DEFAULT_TARGET_TOKENS,
            hard_cap_tokens: DEFAULT_HARD_CAP_TOKENS,
        }
    }
}

/// Normalize text: unify newlines, collapse whitespace runs to a single space.
/// Any `char::is_whitespace` counts (incl. U+3000 / NBSP) — keeps the same
/// definition as `estimate_tokens`, which treats all of them as free.
/// Pure collapse — boundary trimming happens at chunk assembly (`push_chunk`),
/// so English word boundaries ("world. This") survive chunk joins.
pub fn normalize_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_ws = false;
    for c in text.chars() {
        if c.is_whitespace() {
            if !prev_ws && !out.is_empty() {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    out
}

/// Single calibrated token estimator: CJK ×2.0, other non-space ×1.2.
pub fn estimate_tokens(text: &str) -> i64 {
    let mut cjk = 0i64;
    let mut other = 0i64;
    for c in text.chars() {
        if c.is_whitespace() {
            continue;
        }
        let cp = c as u32;
        // CJK unified ideographs + extensions, CJK symbols, kana, hangul.
        let is_cjk = (0x2E80..=0x9FFF).contains(&cp)
            || (0xF900..=0xFAFF).contains(&cp)
            || (0xAC00..=0xD7AF).contains(&cp);
        if is_cjk {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    let est = cjk as f64 * CJK_WEIGHT + other as f64 * OTHER_WEIGHT;
    std::cmp::max(1, est.round() as i64)
}

/// Split text into sentences, delimiter attached, whitespace normalized.
/// The whitespace run *after* a delimiter is absorbed into the sentence, so
/// re-joining sentences reproduces the original spacing exactly.
fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        current.push(c);
        if SENTENCE_DELIMS.contains(&c) {
            while let Some(&nc) = chars.peek() {
                if nc.is_whitespace() {
                    current.push(nc);
                    chars.next();
                } else {
                    break;
                }
            }
            let norm = normalize_whitespace(&current);
            if !norm.is_empty() {
                sentences.push(norm);
            }
            current.clear();
        }
    }
    let tail = normalize_whitespace(&current);
    if !tail.is_empty() {
        sentences.push(tail);
    }
    sentences
}

/// Token count from incremental CJK/other char tallies (same math as
/// `estimate_tokens`, without the O(L) re-scan).
fn tokens_from_counts(cjk: i64, other: i64) -> i64 {
    let est = cjk as f64 * CJK_WEIGHT + other as f64 * OTHER_WEIGHT;
    std::cmp::max(1, est.round() as i64)
}

/// Force-split one over-cap sentence at clause boundaries, then raw chars.
/// Whitespace following a flushed clause delimiter is absorbed into the
/// flushed part, so English word boundaries ("word, word") survive the split.
fn force_split(sentence: &str, cap_tokens: i64) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    // Incremental tallies: `estimate_tokens(&current)` per char would be O(L²).
    let mut cjk = 0i64;
    let mut other = 0i64;

    let flush = |parts: &mut Vec<String>, current: &mut String, cjk: &mut i64, other: &mut i64| {
        let norm = normalize_whitespace(current);
        if !norm.is_empty() {
            parts.push(norm);
        }
        current.clear();
        *cjk = 0;
        *other = 0;
    };

    let mut chars = sentence.chars().peekable();
    while let Some(c) = chars.next() {
        current.push(c);
        if !c.is_whitespace() {
            let cp = c as u32;
            let is_cjk = (0x2E80..=0x9FFF).contains(&cp)
                || (0xF900..=0xFAFF).contains(&cp)
                || (0xAC00..=0xD7AF).contains(&cp);
            if is_cjk {
                cjk += 1;
            } else {
                other += 1;
            }
        }
        let current_tokens = tokens_from_counts(cjk, other);
        let is_clause_boundary = CLAUSE_DELIMS.contains(&c);
        if is_clause_boundary && current_tokens >= cap_tokens / 2 {
            // Absorb the trailing whitespace run into this part: the space
            // after a comma is a real word boundary and must not be dropped.
            while let Some(&nc) = chars.peek() {
                if nc.is_whitespace() {
                    current.push(nc);
                    chars.next();
                } else {
                    break;
                }
            }
            flush(&mut parts, &mut current, &mut cjk, &mut other);
        } else if current_tokens >= cap_tokens {
            // Mid-clause hard cut.
            flush(&mut parts, &mut current, &mut cjk, &mut other);
        }
    }
    if !current.trim().is_empty() {
        flush(&mut parts, &mut current, &mut cjk, &mut other);
    }
    parts
}

/// Split text into synthesis-ready chunks.
///
/// Pipeline: normalize → sentences → greedy pack ≤ target → force-split
/// over-cap sentences. Every chunk carries `style_hint` (ADR-005).
pub fn split(text: &str, style_hint: Option<&str>, config: &ChunkConfig) -> Vec<ChunkSegment> {
    if text.trim().is_empty() {
        return Vec::new();
    }

    let sentences = split_sentences(text);
    let mut chunks: Vec<ChunkSegment> = Vec::new();
    let mut acc = String::new();

    let push_chunk = |acc: &mut String, chunks: &mut Vec<ChunkSegment>| {
        // Trim leading only: a trailing space after an ASCII sentence end
        // ("world. Next...") is a real word boundary and must survive joins.
        let text = normalize_whitespace(acc).trim_start().to_string();
        if text.trim().is_empty() {
            return;
        }
        chunks.push(ChunkSegment {
            char_count: text.chars().count() as i64,
            token_estimate: estimate_tokens(&text),
            text,
            style_hint: style_hint.map(|s| s.to_string()),
        });
    };

    for sentence in sentences {
        let st = estimate_tokens(&sentence);
        if st > config.hard_cap_tokens {
            // Flush accumulator first, then force-split the giant sentence.
            push_chunk(&mut acc, &mut chunks);
            acc.clear();
            for part in force_split(&sentence, config.hard_cap_tokens) {
                acc.push_str(&part);
                // Split at clause granularity — keep packing until target.
                if estimate_tokens(&acc) >= config.target_tokens {
                    push_chunk(&mut acc, &mut chunks);
                    acc.clear();
                }
            }
            continue;
        }
        if !acc.is_empty() && estimate_tokens(&acc) + st > config.target_tokens {
            push_chunk(&mut acc, &mut chunks);
            acc.clear();
        }
        acc.push_str(&sentence);
    }
    push_chunk(&mut acc, &mut chunks);
    chunks
}

/// Self-heal entry: re-chunk with a scaled-down budget (ADR-013).
pub fn rechunk_scaled(text: &str, style_hint: Option<&str>, config: &ChunkConfig) -> Vec<ChunkSegment> {
    rechunk_at_depth(text, style_hint, config, 1)
}

/// Cumulative self-heal: budget shrinks by 0.8^depth. Re-using the same
/// (unscaled) budget on every overflow would produce identical chunks and
/// livelock the re-chunk loop. The engine caps depth at 3 (MAX_RECHUNK_DEPTH)
/// and fails the task beyond it; the clamp here is belt-and-suspenders.
pub fn rechunk_at_depth(
    text: &str,
    style_hint: Option<&str>,
    config: &ChunkConfig,
    depth: u32,
) -> Vec<ChunkSegment> {
    let scale = OVERFLOW_RECHUNK_SCALE.powi(depth.clamp(1, 3) as i32);
    let scaled = ChunkConfig {
        target_tokens: ((config.target_tokens as f64 * scale).round() as i64).max(500),
        hard_cap_tokens: ((config.hard_cap_tokens as f64 * scale).round() as i64).max(600),
    };
    split(text, style_hint, &scaled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_chinese_weighted_2x() {
        assert_eq!(estimate_tokens("你好世界"), 8); // 4 CJK × 2.0
    }

    #[test]
    fn estimate_ascii_weighted_1_2x() {
        assert_eq!(estimate_tokens("hello world"), 12); // 10 non-space × 1.2 = 12.0
    }

    #[test]
    fn estimate_whitespace_free() {
        assert_eq!(estimate_tokens("   "), 1);
    }

    #[test]
    fn normalize_unifies_newlines_and_spaces() {
        assert_eq!(normalize_whitespace("a\r\n  b\nc"), "a b c");
        // leading runs collapse to nothing (out empty), trailing runs to one space;
        // chunk assembly trims boundaries — pure collapse semantics
        assert_eq!(normalize_whitespace("  领先  "), "领先 ");
    }

    #[test]
    fn normalize_collapses_unicode_spaces() {
        assert_eq!(normalize_whitespace("a\u{3000}b\u{00A0}c"), "a b c");
    }

    #[test]
    fn force_split_preserves_english_word_boundaries() {
        // A single giant clause-delimited "sentence" with spaces must round-trip:
        // "word, word" must never become "word,word" after the force split.
        let mut input = String::new();
        for i in 0..3000 {
            if i > 0 {
                input.push_str(", ");
            }
            input.push_str("word");
        }
        let chunks = split(&input, None, &ChunkConfig::default());
        assert!(chunks.len() >= 2, "sample must trigger force_split");
        let joined: String = chunks.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(joined, normalize_whitespace(&input));
        assert!(!joined.contains("word,word"), "space after comma was dropped");
    }

    #[test]
    fn roundtrip_join_equals_normalized_input() {
        // Property: concatenating chunk texts must equal normalize_whitespace(input).
        let huge = "没有标点的一段超长文本".to_string() + &"很".repeat(5000);
        let samples: Vec<&str> = vec![
            "第一句。第二句！第三句？",
            "Hello world. This is a test! Another one?",
            "段落一，有逗号；还有分号：冒号。\n段落二内容很多很多很多。",
            huge.as_str(),
            "中文english混排文本Mixed content测试。\n\n再一句！",
        ];
        for text in samples {
            let chunks = split(text, Some("温柔"), &ChunkConfig::default());
            let joined: String = chunks.iter().map(|c| c.text.as_str()).collect();
            assert_eq!(
                joined,
                normalize_whitespace(text),
                "round-trip mismatch for sample: {text}"
            );
            assert!(!chunks.is_empty());
        }
    }

    #[test]
    fn every_chunk_carries_style_hint() {
        let chunks = split(
            "一句。两句。三句。四句。五句。六句。",
            Some("活泼"),
            &ChunkConfig {
                target_tokens: 8,
                hard_cap_tokens: 100,
            },
        );
        assert!(chunks.len() >= 2, "small target should produce multiple chunks");
        for c in &chunks {
            assert_eq!(c.style_hint.as_deref(), Some("活泼"));
            assert!(c.token_estimate <= 8 + 8, "estimate over target: {}", c.token_estimate);
        }
    }

    #[test]
    fn chunks_respect_target_and_hard_cap() {
        let long = "这是一句比较长的句子，".repeat(200) + "。";
        let cfg = ChunkConfig {
            target_tokens: 600,
            hard_cap_tokens: 750,
        };
        let chunks = split(&long, None, &cfg);
        assert!(chunks.len() > 1);
        for c in &chunks {
            assert!(
                c.token_estimate <= cfg.hard_cap_tokens + 16,
                "chunk {} exceeds hard cap",
                c.token_estimate
            );
        }
    }

    #[test]
    fn empty_and_blank_produce_nothing() {
        assert!(split("", None, &ChunkConfig::default()).is_empty());
        assert!(split("   \n  ", None, &ChunkConfig::default()).is_empty());
    }

    #[test]
    fn giant_sentence_force_splits() {
        let huge = "哈".repeat(20_000);
        let chunks = split(&huge, None, &ChunkConfig::default());
        assert!(chunks.len() >= 2);
        let joined: String = chunks.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(joined, huge);
    }

    #[test]
    fn rechunk_scaled_shrinks_budget() {
        let cfg = ChunkConfig {
            target_tokens: 600,
            hard_cap_tokens: 750,
        };
        // 20 × 28 tokens = 560: one chunk unscaled; at ×0.8 (target 480) it
        // must split into two — proving the budget actually shrinks.
        let text = "这是一句接近预算上限的文本。".repeat(20);
        let plain = split(&text, None, &cfg);
        let scaled = rechunk_scaled(&text, None, &cfg);
        assert!(
            scaled.len() > plain.len(),
            "scaled budget must produce more chunks ({} vs {})",
            scaled.len(),
            plain.len()
        );
        for c in &scaled {
            assert!(c.token_estimate < cfg.target_tokens);
        }
    }
}
