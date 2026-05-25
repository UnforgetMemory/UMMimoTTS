use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single parsed item from an uploaded batch file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedItem {
    pub index: usize,
    pub text: String,
    pub voice: Option<String>,
    pub model: Option<String>,
    pub title: Option<String>,
    pub context: Option<String>,
    pub speed: Option<f32>,
    /// Error if this row failed to parse
    pub error: Option<String>,
    /// Source filename (None for single-file uploads, Some(filename) for multi-file uploads)
    pub source_filename: Option<String>,
    /// Approximate token count (ceil(char_count / 1.5) for Chinese text ratio)
    pub token_count: usize,
}

/// Lightweight preview version (no full text/context).
#[derive(Debug, Clone, Serialize)]
pub struct ParsedItemSummary {
    pub index: usize,
    pub text_preview: String,
    pub voice: Option<String>,
    pub model: Option<String>,
    pub title: Option<String>,
    pub char_count: usize,
    pub has_error: bool,
    pub error: Option<String>,
    pub source_filename: Option<String>,
    pub token_count: usize,
}

impl ParsedItem {
    pub fn to_summary(&self) -> ParsedItemSummary {
        let preview = if self.text.len() > 80 {
            // Find the last valid UTF-8 character boundary at or before byte 80
            let mut end = 80;
            while !self.text.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}...", &self.text[..end])
        } else {
            self.text.clone()
        };
        let char_count = self.text.chars().count();

        ParsedItemSummary {
            index: self.index,
            text_preview: preview,
            voice: self.voice.clone(),
            model: self.model.clone(),
            title: self.title.clone(),
            char_count,
            has_error: self.error.is_some(),
            error: self.error.clone(),
            source_filename: self.source_filename.clone(),
            token_count: self.token_count,
        }
    }
}

/// Per-file statistics for batch imports
#[derive(Debug, Clone, Serialize)]
pub struct FileStat {
    pub filename: String,
    pub item_count: usize,
    pub char_count: usize,
    pub token_count: usize,
}

/// Stats about a pending import
#[derive(Debug, Clone, Serialize)]
pub struct ImportStats {
    pub total_items: usize,
    pub valid_items: usize,
    pub error_items: usize,
    pub total_chars: usize,
    pub total_token_count: usize,
    pub file_stats: Vec<FileStat>,
    pub created_at: String,
    pub expires_at: String,
}

/// Full pending import state (backed by token, never sent to frontend)
#[derive(Debug, Clone)]
pub struct PendingImport {
    pub token: String,
    pub original_filename: String,
    pub items: Vec<ParsedItem>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub submitted: bool,
}

impl PendingImport {
    pub fn new(token: String, filename: String, items: Vec<ParsedItem>) -> Self {
        let now = Utc::now();
        Self {
            token,
            original_filename: filename,
            items,
            created_at: now,
            expires_at: now + chrono::Duration::minutes(10),
            submitted: false,
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    pub fn extend_ttl(&mut self) {
        self.expires_at = Utc::now() + chrono::Duration::minutes(10);
    }

    /// Remove all items belonging to a specific file.
    pub fn remove_file_by_filename(&mut self, filename: &str) -> bool {
        let original_len = self.items.len();
        self.items.retain(|item| item.source_filename.as_deref() != Some(filename));
        // Re-index items after removal
        for (i, item) in self.items.iter_mut().enumerate() {
            item.index = i;
        }
        self.items.len() < original_len
    }

    pub fn file_stats(&self) -> Vec<FileStat> {
        use std::collections::HashMap;

        // Use original_filename as the key for items with no source_filename
        let default_key = self.original_filename.clone();

        let mut groups: HashMap<String, (usize, usize, usize)> = HashMap::new();
        for item in &self.items {
            let key = item.source_filename.clone().unwrap_or_else(|| default_key.clone());
            let (ref mut count, ref mut chars, ref mut tokens) =
                groups.entry(key).or_insert((0, 0, 0));
            *count += 1;
            *chars += item.text.len();
            *tokens += item.token_count;
        }

        let mut stats: Vec<FileStat> = groups
            .into_iter()
            .map(|(filename, (item_count, char_count, token_count))| FileStat {
                filename,
                item_count,
                char_count,
                token_count,
            })
            .collect();
        stats.sort_by(|a, b| a.filename.cmp(&b.filename));
        stats
    }

    pub fn stats(&self) -> ImportStats {
        let valid = self.items.iter().filter(|i| i.error.is_none()).count();
        let errs = self.items.len() - valid;
        let total_chars: usize = self.items.iter().map(|i| i.text.len()).sum();
        let total_token_count: usize = self.items.iter().map(|i| i.token_count).sum();
        let file_stats = self.file_stats();
        ImportStats {
            total_items: self.items.len(),
            valid_items: valid,
            error_items: errs,
            total_chars,
            total_token_count,
            file_stats,
            created_at: self.created_at.to_rfc3339(),
            expires_at: self.expires_at.to_rfc3339(),
        }
    }
}
