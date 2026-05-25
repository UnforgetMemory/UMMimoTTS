use crate::db::SqlitePool;
use crate::models::batch_import::{FileStat, ParsedItem, ParsedItemSummary, PendingImport};
use parking_lot::RwLock;
use std::collections::HashMap;

/// Manages batch file imports with token-based access and TTL expiry.
pub struct BatchImportManager {
    inner: RwLock<HashMap<String, PendingImport>>,
    pool: SqlitePool,
}

impl BatchImportManager {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
            pool,
        }
    }

    /// Store a new pending import and return its token.
    pub fn create_import(&self, filename: String, items: Vec<ParsedItem>) -> String {
        let token = uuid::Uuid::new_v4().to_string();
        let import = PendingImport::new(token.clone(), filename, items);
        self.inner.write().insert(token.clone(), import);
        token
    }

    /// Get import by token. Returns None if expired or missing.
    pub fn get_import(&self, token: &str) -> Option<PendingImport> {
        let guard = self.inner.read();
        let import = guard.get(token)?;
        if import.is_expired() {
            drop(guard);
            self.remove_expired();
            return None;
        }
        Some(import.clone())
    }

    /// Get paginated preview of parsed items.
    pub fn get_preview(
        &self,
        token: &str,
        page: usize,
        per_page: usize,
    ) -> Option<(Vec<ParsedItemSummary>, usize)> {
        let import = self.get_import(token)?;
        let total = import.items.len();
        let start = page * per_page;
        let end = std::cmp::min(start + per_page, total);
        let items: Vec<ParsedItemSummary> = if start < total {
            import.items[start..end]
                .iter()
                .map(|i| i.to_summary())
                .collect()
        } else {
            vec![]
        };
        Some((items, total))
    }

    /// Update a single item's overrides.
    pub fn update_item(
        &self,
        token: &str,
        index: usize,
        voice: Option<String>,
        model: Option<String>,
        title: Option<String>,
        context: Option<String>,
    ) -> Result<(), &'static str> {
        let mut guard = self.inner.write();
        let import = guard.get_mut(token).ok_or("import not found")?;
        if import.is_expired() {
            return Err("import expired");
        }
        let item = import.items.get_mut(index).ok_or("invalid index")?;
        if let Some(v) = voice {
            item.voice = Some(v);
        }
        if let Some(m) = model {
            item.model = Some(m);
        }
        if let Some(t) = title {
            item.title = Some(t);
        }
        if let Some(c) = context {
            item.context = Some(c);
        }
        Ok(())
    }

    /// Mark import as submitted (prevents double-submit).
    pub fn mark_submitted(&self, token: &str) -> bool {
        let mut guard = self.inner.write();
        let import = guard.get_mut(token).map(|i| {
            i.submitted = true;
        });
        import.is_some()
    }

    /// Extend TTL for an active import.
    pub fn extend_ttl(&self, token: &str) -> bool {
        let mut guard = self.inner.write();
        if let Some(import) = guard.get_mut(token) {
            if !import.is_expired() {
                import.extend_ttl();
                return true;
            }
        }
        false
    }

    /// Remove expired imports from memory.
    fn remove_expired(&self) {
        let mut guard = self.inner.write();
        guard.retain(|_, i| !i.is_expired());
    }

    /// Periodic cleanup of expired imports.
    pub fn cleanup_expired(&self) {
        self.remove_expired();
    }

    /// Get paginated per-file statistics for an import.
    pub fn get_file_stats(
        &self,
        token: &str,
        sort: &str,
        dir: &str,
        page: usize,
        per_page: usize,
    ) -> Option<(Vec<FileStat>, usize)> {
        let import = self.get_import(token)?;
        let mut stats = import.file_stats();
        // Sort
        match sort {
            "item_count" => stats.sort_by(|a, b| a.item_count.cmp(&b.item_count)),
            "char_count" => stats.sort_by(|a, b| a.char_count.cmp(&b.char_count)),
            "token_count" => stats.sort_by(|a, b| a.token_count.cmp(&b.token_count)),
            _ => stats.sort_by(|a, b| a.filename.cmp(&b.filename)), // default: filename
        }
        if dir == "desc" {
            stats.reverse();
        }
        let total = stats.len();
        let start = page * per_page;
        let end = std::cmp::min(start + per_page, total);
        let page_stats: Vec<FileStat> = if start < total {
            stats[start..end].to_vec()
        } else {
            vec![]
        };
        Some((page_stats, total))
    }

    /// Remove all items belonging to a specific file from an import.
    pub fn remove_file(&self, token: &str, filename: &str) -> Result<usize, &'static str> {
        let mut guard = self.inner.write();
        let import = guard.get_mut(token).ok_or("import not found")?;
        if import.is_expired() {
            return Err("import expired");
        }
        let before = import.items.len();
        import.remove_file_by_filename(filename);
        Ok(before - import.items.len())
    }
}
