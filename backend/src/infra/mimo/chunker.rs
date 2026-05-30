use crate::shared::error::AppError;
use tracing::trace;

/// Rough token estimation: Chinese chars ≈ 2 tokens, ASCII ≈ 0.3 tokens.
fn estimate_token_count(text: &str) -> i64 {
    let (chinese, ascii) = text.chars().fold((0i64, 0i64), |(c, a), ch| {
        if ('\u{4E00}'..='\u{9FFF}').contains(&ch)
            || ('\u{3400}'..='\u{4DBF}').contains(&ch)
        {
            (c + 1, a)
        } else {
            (c, a + 1)
        }
    });
    std::cmp::max(1, chinese * 2 + ascii * 3 / 10)
}

/// A text segment produced by the chunker.
pub struct ChunkSegment {
    pub text: String,
    pub char_count: i64,
    pub token_count: i64,
    /// Optional style/context prefix for this chunk.
    pub context_hint: Option<String>,
}

/// Sentence-level breakdown from the tokenize API.
pub struct SentenceInfo {
    pub text: String,
    pub token_count: i64,
    pub char_count: i64,
}

/// Splits long TTS text into chunks suitable for the MIMO API.
///
/// Uses local heuristic sentence splitting — the MIMO API does not expose
/// a tokenize endpoint, so we rely on punctuation-based boundary detection.
pub struct MimoChunker {
    /// Target tokens per chunk (typical range: 2000–3000).
    pub target_tokens: i64,
    /// Hard cap — a single sentence that exceeds this will be split at char level.
    pub hard_cap_tokens: i64,
}

impl MimoChunker {
    pub fn new(_base_url: &str, target_tokens: i64, hard_cap_tokens: i64) -> Self {
        Self {
            target_tokens,
            hard_cap_tokens,
        }
    }

    /// Tokenize text into sentences using local heuristic splitting.
    pub async fn tokenize(&self, text: &str) -> Result<Vec<SentenceInfo>, AppError> {
        if text.is_empty() {
            return Ok(Vec::new());
        }
        trace!("using local sentence tokenizer");
        Ok(self.tokenize_local(text))
    }

    /// Local fallback: split text by sentence-ending punctuation and estimate token counts.
    fn tokenize_local(&self, text: &str) -> Vec<SentenceInfo> {
        let mut result = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut start = 0;

        for (i, c) in chars.iter().enumerate() {
            if matches!(c, '。' | '！' | '？' | '!' | '?' | '\n') && i + 1 < chars.len() {
                let sentence: String = chars[start..=i].iter().collect();
                if !sentence.trim().is_empty() {
                    let char_count = sentence.chars().count() as i64;
                    let token_count = estimate_token_count(&sentence);
                    result.push(SentenceInfo { text: sentence, token_count, char_count });
                }
                start = i + 1;
            }
        }

        // Last segment
        if start < chars.len() {
            let sentence: String = chars[start..].iter().collect();
            if !sentence.trim().is_empty() {
                let char_count = sentence.chars().count() as i64;
                let token_count = estimate_token_count(&sentence);
                result.push(SentenceInfo { text: sentence, token_count, char_count });
            }
        }

        // If no boundaries found, treat the whole text as one sentence
        if result.is_empty() {
            let char_count = text.chars().count() as i64;
            let token_count = estimate_token_count(text);
            result.push(SentenceInfo { text: text.to_string(), token_count, char_count });
        }

        result
    }

    /// Split text into chunks respecting sentence boundaries.
    ///
    /// Uses the remote tokenize API when available, falling back to
    /// a local heuristic if the API call fails.
    pub async fn split(
        &self,
        text: &str,
        context_hint: Option<&str>,
    ) -> Result<Vec<ChunkSegment>, AppError> {
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }

        let sentences = match self.tokenize(text).await {
            Ok(s) => s,
            Err(_) => {
                return Ok(self.split_heuristic(text, context_hint));
            }
        };

        Ok(self.build_chunks_from_sentences(&sentences, context_hint))
    }

    /// Build chunks from a list of sentence infos.
    fn build_chunks_from_sentences(
        &self,
        sentences: &[SentenceInfo],
        context_hint: Option<&str>,
    ) -> Vec<ChunkSegment> {
        let mut chunks: Vec<ChunkSegment> = Vec::new();
        let mut acc_text = String::new();
        let mut acc_tokens: i64 = 0;
        let mut acc_chars: i64 = 0;

        for sentence in sentences {
            // Single sentence exceeding hard cap → force split at character level
            if sentence.token_count > self.hard_cap_tokens && acc_text.is_empty() {
                let forced = self.force_split_sentence(sentence, context_hint);
                chunks.extend(forced);
                continue;
            }

            let new_total = acc_tokens + sentence.token_count;

            // If adding this sentence would exceed target, flush the accumulated chunk
            if !acc_text.is_empty() && new_total > self.target_tokens {
                let context = if chunks.is_empty() {
                    context_hint.map(|s| s.to_string())
                } else {
                    None
                };
                chunks.push(ChunkSegment {
                    text: acc_text.trim().to_string(),
                    char_count: acc_chars,
                    token_count: acc_tokens,
                    context_hint: context,
                });
                acc_text.clear();
                acc_tokens = 0;
                acc_chars = 0;
            }

            if !acc_text.is_empty() {
                acc_text.push('\n');
            }
            acc_text.push_str(&sentence.text);
            acc_tokens += sentence.token_count;
            acc_chars += sentence.char_count;
        }

        // Flush remaining
        if !acc_text.is_empty() {
            let context = if chunks.is_empty() {
                context_hint.map(|s| s.to_string())
            } else {
                None
            };
            chunks.push(ChunkSegment {
                text: acc_text.trim().to_string(),
                char_count: acc_chars,
                token_count: acc_tokens,
                context_hint: context,
            });
        }

        chunks
    }

    /// Force-split a single sentence that exceeds hard_cap_tokens.
    fn force_split_sentence(
        &self,
        sentence: &SentenceInfo,
        context_hint: Option<&str>,
    ) -> Vec<ChunkSegment> {
        let text = &sentence.text;
        let total_chars = text.chars().count() as i64;
        let tokens_per_char = sentence.token_count as f64 / total_chars.max(1) as f64;
        let cap_chars = (self.hard_cap_tokens as f64 / tokens_per_char) as usize;

        let mut chunks = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut pos = 0;
        let mut chunk_idx = 0;

        while pos < chars.len() {
            let end = (pos + cap_chars).min(chars.len());
            let segment: String = chars[pos..end].iter().collect();
            let seg_chars = (end - pos) as i64;
            let seg_tokens = (seg_chars as f64 * tokens_per_char) as i64;

            let context = if chunk_idx == 0 {
                context_hint.map(|s| s.to_string())
            } else {
                None
            };

            chunks.push(ChunkSegment {
                text: segment,
                char_count: seg_chars,
                token_count: seg_tokens,
                context_hint: context,
            });
            pos = end;
            chunk_idx += 1;
        }

        chunks
    }

    /// Fallback heuristic split when the remote tokenize API is unavailable.
    pub fn split_heuristic(
        &self,
        text: &str,
        context_hint: Option<&str>,
    ) -> Vec<ChunkSegment> {
        if text.trim().is_empty() {
            return Vec::new();
        }

        let raw_sentences = split_sentences(text);

        if raw_sentences.is_empty() {
            let token_count = Self::estimate_tokens(text);
            let char_count = text.chars().count() as i64;
            return vec![ChunkSegment {
                text: text.to_string(),
                char_count,
                token_count,
                context_hint: context_hint.map(|s| s.to_string()),
            }];
        }

        let sentences: Vec<SentenceInfo> = raw_sentences
            .iter()
            .map(|s| {
                let token_count = Self::estimate_tokens(s);
                let char_count = s.chars().count() as i64;
                SentenceInfo {
                    text: s.to_string(),
                    token_count,
                    char_count,
                }
            })
            .collect();

        self.build_chunks_from_sentences(&sentences, context_hint)
    }

    /// Heuristic token estimate without calling the remote API.
    ///
    /// Chinese characters (Unicode > U+2E80) count as 1.3 tokens each;
    /// ASCII characters count as 0.4 tokens each.
    pub fn estimate_tokens(text: &str) -> i64 {
        let mut chinese_chars = 0i64;
        let mut ascii_chars = 0i64;

        for c in text.chars() {
            if c > '\u{2E80}' {
                chinese_chars += 1;
            } else if c.is_ascii() && !c.is_ascii_whitespace() {
                ascii_chars += 1;
            }
        }

        let tokens = (chinese_chars as f64 * 1.3) + (ascii_chars as f64 * 0.4);
        tokens.round() as i64
    }
}

/// Split text at sentence boundaries: 。！？.!?\n
fn split_sentences(text: &str) -> Vec<String> {
    let delimiters = ['。', '！', '？', '.', '!', '?', '\n'];
    let mut result = Vec::new();
    let mut current = String::new();

    for c in text.chars() {
        current.push(c);
        if delimiters.contains(&c) {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                result.push(trimmed);
            }
            current.clear();
        }
    }

    let remaining = current.trim().to_string();
    if !remaining.is_empty() {
        result.push(remaining);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[actix_rt::test]
    async fn test_local_tokenize_splits_sentences() {
        let chunker = MimoChunker::new("http://unused", 1000, 2000);
        let result = chunker
            .tokenize("你好世界。今天天气真好。我们去公园吧。")
            .await
            .unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].text, "你好世界。");
        assert_eq!(result[1].text, "今天天气真好。");
        assert_eq!(result[2].text, "我们去公园吧。");
    }

    #[actix_rt::test]
    async fn test_local_tokenize_empty() {
        let chunker = MimoChunker::new("http://unused", 1000, 2000);
        let result = chunker.tokenize("").await.unwrap();
        assert!(result.is_empty());
    }

    #[actix_rt::test]
    async fn test_local_tokenize_no_punctuation() {
        let chunker = MimoChunker::new("http://unused", 1000, 2000);
        let result = chunker.tokenize("没有标点的文本").await.unwrap();
        assert_eq!(result.len(), 1);
    }

    #[actix_rt::test]
    async fn test_chunker_split_normal_text() {
        let chunker = MimoChunker::new("http://unused", 20, 500);
        let chunks = chunker.split(TEXT_5_SENTENCES, None).await.unwrap();

        assert!(
            chunks.len() >= 2,
            "5 sentences with small target should produce >=2 chunks, got {}",
            chunks.len()
        );
        for chunk in &chunks {
            assert!(!chunk.text.is_empty(), "No empty chunks");
        }
    }

    #[actix_rt::test]
    async fn test_chunker_split_with_context_hint() {
        let chunker = MimoChunker::new("http://unused", 500, 1000);
        let chunks = chunker.split("句子1。句子2。句子3。", Some("激昂")).await.unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].context_hint.as_deref(), Some("激昂"));
    }

    #[actix_rt::test]
    async fn test_chunker_split_single_huge_sentence() {
        let huge_text = "哈".repeat(2000);

        let chunker = MimoChunker::new("http://unused", 2000, 100);
        let chunks = chunker.split(&huge_text, None).await.unwrap();

        assert!(
            chunks.len() >= 2,
            "Huge sentence should be force-split into >=2 chunks, got {}",
            chunks.len()
        );
        let total: String = chunks.iter().map(|c| c.text.clone()).collect();
        assert_eq!(total.len(), huge_text.len());
    }

    #[actix_rt::test]
    async fn test_chunker_split_empty() {
        let chunker = MimoChunker::new("http://unused", 100, 500);
        let chunks = chunker.split("", None).await.unwrap();
        assert!(chunks.is_empty(), "Empty text should produce 0 chunks");

        let chunks = chunker.split("  ", None).await.unwrap();
        assert!(chunks.is_empty(), "Whitespace-only text should produce 0 chunks");
    }

    #[test]
    fn test_chunker_estimate_tokens_chinese() {
        assert_eq!(MimoChunker::estimate_tokens("你好世界"), 5);
    }

    #[test]
    fn test_chunker_estimate_tokens_english() {
        assert_eq!(MimoChunker::estimate_tokens("hello world"), 4);
    }

    #[test]
    fn test_chunker_estimate_tokens_mixed() {
        // 2 Chinese * 1.3 = 2.6, 3 ASCII * 0.4 = 1.2, total = 3.8 ≈ 4
        assert_eq!(MimoChunker::estimate_tokens("中文abc"), 4);
    }

    #[test]
    fn test_split_sentences_function() {
        let result = split_sentences("第一句。第二句！第三句？");
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "第一句。");
        assert_eq!(result[1], "第二句！");
        assert_eq!(result[2], "第三句？");
    }

    const TEXT_5_SENTENCES: &str = "这是第一个句子。第二个句子内容。第三个句子在这里。第四句。第五句最后。";
}
