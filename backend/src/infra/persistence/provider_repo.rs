//! Provider repository trait and SQLite implementation.

use crate::shared::error::AppError;
use crate::infra::persistence::db::DbPool;
use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::{Serialize, Deserialize};

/// A configured MIMO provider stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    #[serde(skip_serializing)]
    pub api_key: String,
    pub is_configured: bool,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub trait ProviderRepo: Send + Sync {
    fn find_all(&self) -> Result<Vec<Provider>, AppError>;
    fn find_by_id(&self, id: &str) -> Result<Option<Provider>, AppError>;
    fn find_default(&self) -> Result<Option<Provider>, AppError>;
    fn update_api_key(&self, id: &str, api_key: &str) -> Result<(), AppError>;
    fn set_default(&self, id: &str) -> Result<(), AppError>;
    /// Find multiple providers by their IDs (for batch resolution).
    fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Provider>, AppError>;
}

pub struct SqliteProviderRepo {
    pub pool: DbPool,
}

impl SqliteProviderRepo {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    fn parse_datetime(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now())
    }

    fn row_to_provider(row: &rusqlite::Row) -> rusqlite::Result<Provider> {
        let is_conf: i32 = row.get("is_configured")?;
        let is_def: i32 = row.get("is_default")?;
        Ok(Provider {
            id: row.get::<_, String>("id")?,
            name: row.get::<_, String>("name")?,
            base_url: row.get::<_, String>("base_url")?,
            api_key: row.get::<_, String>("api_key")?,
            is_configured: is_conf != 0,
            is_default: is_def != 0,
            created_at: Self::parse_datetime(&row.get::<_, String>("created_at")?),
            updated_at: Self::parse_datetime(&row.get::<_, String>("updated_at")?),
        })
    }
}

impl ProviderRepo for SqliteProviderRepo {
    fn find_all(&self) -> Result<Vec<Provider>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM providers ORDER BY CASE WHEN is_default = 1 THEN 0 ELSE 1 END, name ASC",
        )?;
        let providers = stmt
            .query_map([], Self::row_to_provider)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(providers)
    }

    fn find_by_id(&self, id: &str) -> Result<Option<Provider>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT * FROM providers WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![id], Self::row_to_provider)?;
        match rows.next() {
            Some(Ok(provider)) => Ok(Some(provider)),
            _ => Ok(None),
        }
    }

    fn find_default(&self) -> Result<Option<Provider>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT * FROM providers WHERE is_default = 1 LIMIT 1")?;
        let mut rows = stmt.query_map([], Self::row_to_provider)?;
        match rows.next() {
            Some(Ok(provider)) => Ok(Some(provider)),
            _ => {
                // Fallback: return the first provider if none is marked default
                let mut stmt = conn.prepare("SELECT * FROM providers ORDER BY id ASC LIMIT 1")?;
                let mut rows = stmt.query_map([], Self::row_to_provider)?;
                match rows.next() {
                    Some(Ok(provider)) => Ok(Some(provider)),
                    _ => Ok(None),
                }
            }
        }
    }

    fn update_api_key(&self, id: &str, api_key: &str) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        let is_configured: i32 = if api_key.is_empty() { 0 } else { 1 };
        let affected = conn.execute(
            "UPDATE providers SET api_key = ?1, is_configured = ?2, updated_at = ?3 WHERE id = ?4",
            params![api_key, is_configured, Utc::now().to_rfc3339(), id],
        )?;
        if affected == 0 {
            return Err(AppError::NotFound(format!("Provider {} not found", id)));
        }
        Ok(())
    }

    fn set_default(&self, id: &str) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        // Clear existing default
        conn.execute("UPDATE providers SET is_default = 0", [])?;
        // Set new default
        let affected = conn.execute(
            "UPDATE providers SET is_default = 1, updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), id],
        )?;
        if affected == 0 {
            return Err(AppError::NotFound(format!("Provider {} not found", id)));
        }
        Ok(())
    }

    fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Provider>, AppError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders: Vec<String> = ids.iter().enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect();
        let sql = format!(
            "SELECT * FROM providers WHERE id IN ({})",
            placeholders.join(",")
        );
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::types::ToSql> =
            ids.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
        let providers = stmt
            .query_map(params.as_slice(), Self::row_to_provider)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(providers)
    }
}
