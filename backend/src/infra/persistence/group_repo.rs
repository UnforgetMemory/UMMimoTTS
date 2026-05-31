//! Group repository trait and SQLite implementation.

#![allow(dead_code)]

use crate::shared::id::Id;
use crate::shared::error::AppError;
use crate::domain::group::{Group, GroupStatus};
use crate::infra::persistence::db::DbPool;
use chrono::{DateTime, Utc};
use rusqlite::params;

pub trait GroupRepo: Send + Sync {
    fn insert(&self, group: &Group) -> Result<(), AppError>;
    fn find_by_id(&self, id: &str) -> Result<Option<Group>, AppError>;
    fn update_status(&self, id: &str, status: &GroupStatus) -> Result<(), AppError>;
    fn find_by_batch(&self, batch_id: &str) -> Result<Vec<Group>, AppError>;
    fn find_all(&self) -> Result<Vec<Group>, AppError>;
    /// Atomically increment `done_tasks` and return the updated group.
    fn increment_done_tasks(&self, id: &str) -> Result<Group, AppError>;
    /// Atomically increment `failed_tasks` and return the updated group.
    fn increment_failed_tasks(&self, id: &str) -> Result<Group, AppError>;
}

pub struct SqliteGroupRepo {
    pub pool: DbPool,
}

impl SqliteGroupRepo {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    fn parse_datetime(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now())
    }

    fn row_to_group(row: &rusqlite::Row) -> rusqlite::Result<Group> {
        Ok(Group {
            id: Id::from_str(&row.get::<_, String>("id")?)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
            batch_id: Id::from_str(&row.get::<_, String>("batch_id")?)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
            status: serde_json::from_str(&row.get::<_, String>("status")?).unwrap(),
            title: row.get::<_, String>("name")?,
            total_tasks: row.get("total_tasks")?,
            done_tasks: row.get("done_tasks")?,
            failed_tasks: row.get("failed_tasks")?,
            created_at: Self::parse_datetime(&row.get::<_, String>("created_at")?),
            updated_at: Self::parse_datetime(&row.get::<_, String>("updated_at")?),
            completed_at: row
                .get::<_, Option<String>>("completed_at")?
                .map(|s| Self::parse_datetime(&s)),
        })
    }
}

impl GroupRepo for SqliteGroupRepo {
    fn insert(&self, group: &Group) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO groups (id, batch_id, name, status, voice, model, style, speed,
             priority, total_tasks, done_tasks, failed_tasks, created_at, updated_at, completed_at)
             VALUES (?1,?2,?3,?4,NULL,NULL,NULL,NULL,0,?5,?6,?7,?8,?9,?10)",
            params![
                group.id.to_string(),
                group.batch_id.to_string(),
                group.title,
                serde_json::to_string(&group.status).unwrap(),
                group.total_tasks,
                group.done_tasks,
                group.failed_tasks,
                group.created_at.to_rfc3339(),
                group.updated_at.to_rfc3339(),
                group.completed_at.map(|dt| dt.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    fn find_by_id(&self, id: &str) -> Result<Option<Group>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT * FROM groups WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![id], Self::row_to_group)?;
        match rows.next() {
            Some(Ok(group)) => Ok(Some(group)),
            _ => Ok(None),
        }
    }

    fn update_status(&self, id: &str, status: &GroupStatus) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        let is_terminal = matches!(status, GroupStatus::Completed | GroupStatus::Cancelled);
        let completed_at = if is_terminal {
            Some(Utc::now().to_rfc3339())
        } else {
            None
        };
        let affected = conn.execute(
            "UPDATE groups SET status = ?1, updated_at = ?2, completed_at = COALESCE(?3, completed_at) WHERE id = ?4",
            params![
                serde_json::to_string(status).unwrap(),
                Utc::now().to_rfc3339(),
                completed_at,
                id,
            ],
        )?;
        if affected == 0 {
            return Err(AppError::NotFound(format!("Group {} not found", id)));
        }
        Ok(())
    }

    fn find_by_batch(&self, batch_id: &str) -> Result<Vec<Group>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM groups WHERE batch_id = ?1 ORDER BY created_at ASC",
        )?;
        let groups = stmt
            .query_map(params![batch_id], Self::row_to_group)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(groups)
    }

    fn find_all(&self) -> Result<Vec<Group>, AppError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT * FROM groups ORDER BY created_at DESC")?;
        let groups = stmt
            .query_map([], Self::row_to_group)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(groups)
    }

    fn increment_done_tasks(&self, id: &str) -> Result<Group, AppError> {
        let now = Utc::now().to_rfc3339();
        let conn = self.pool.get()?;
        // Atomically increment done_tasks. SQLite doesn't have RETURNING in older versions,
        // so we do UPDATE + SELECT.
        let affected = conn.execute(
            "UPDATE groups SET done_tasks = done_tasks + 1, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        if affected == 0 {
            return Err(AppError::NotFound(format!("Group {} not found", id)));
        }
        // If all tasks are now terminal, transition status.
        // Done: done_tasks + failed_tasks >= total_tasks
        conn.execute(
            "UPDATE groups SET status = CASE
                WHEN done_tasks + failed_tasks >= total_tasks AND done_tasks > 0 THEN '\"completed\"'
                WHEN done_tasks + failed_tasks >= total_tasks AND done_tasks = 0 THEN '\"failed\"'
                ELSE status
            END,
            completed_at = CASE
                WHEN done_tasks + failed_tasks >= total_tasks THEN ?1
                ELSE completed_at
            END
            WHERE id = ?2 AND done_tasks + failed_tasks >= total_tasks",
            params![now, id],
        )?;
        // Re-read the updated group.
        self.find_by_id(id)?.ok_or_else(|| AppError::NotFound(format!("Group {} not found after increment", id)))
    }

    fn increment_failed_tasks(&self, id: &str) -> Result<Group, AppError> {
        let now = Utc::now().to_rfc3339();
        let conn = self.pool.get()?;
        let affected = conn.execute(
            "UPDATE groups SET failed_tasks = failed_tasks + 1, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        if affected == 0 {
            return Err(AppError::NotFound(format!("Group {} not found", id)));
        }
        // If all tasks are now terminal, transition status.
        conn.execute(
            "UPDATE groups SET status = CASE
                WHEN done_tasks + failed_tasks >= total_tasks AND done_tasks > 0 THEN '\"completed\"'
                WHEN done_tasks + failed_tasks >= total_tasks AND done_tasks = 0 THEN '\"failed\"'
                ELSE status
            END,
            completed_at = CASE
                WHEN done_tasks + failed_tasks >= total_tasks THEN ?1
                ELSE completed_at
            END
            WHERE id = ?2 AND done_tasks + failed_tasks >= total_tasks",
            params![now, id],
        )?;
        self.find_by_id(id)?.ok_or_else(|| AppError::NotFound(format!("Group {} not found after increment", id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::persistence::db::create_test_pool;
    use crate::infra::persistence::migrate::run_migrations;

    fn create_test_group() -> Group {
        Group::new(Id::new(), "Test Group".into())
    }

    #[test]
    fn test_group_insert_and_find() {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let repo = SqliteGroupRepo::new(pool);
        let group = create_test_group();
        repo.insert(&group).unwrap();
        let found = repo.find_by_id(group.id.as_str()).unwrap().unwrap();
        assert_eq!(found.title, group.title);
        assert_eq!(found.status, GroupStatus::Pending);
    }

    #[test]
    fn test_group_update_status() {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let repo = SqliteGroupRepo::new(pool);
        let group = create_test_group();
        repo.insert(&group).unwrap();
        repo.update_status(group.id.as_str(), &GroupStatus::Queued)
            .unwrap();
        let found = repo.find_by_id(group.id.as_str()).unwrap().unwrap();
        assert_eq!(found.status, GroupStatus::Queued);
    }

    #[test]
    fn test_group_find_by_batch() {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let repo = SqliteGroupRepo::new(pool);
        let batch_id = Id::new();
        for i in 0..3 {
            let group = Group::new(batch_id.clone(), format!("Group {}", i));
            repo.insert(&group).unwrap();
        }
        let groups = repo.find_by_batch(batch_id.as_str()).unwrap();
        assert_eq!(groups.len(), 3);
    }

    #[test]
    fn test_group_update_status_not_found() {
        let pool = create_test_pool();
        run_migrations(&pool.get().unwrap()).unwrap();
        let repo = SqliteGroupRepo::new(pool);
        let result = repo.update_status("nonexistent", &GroupStatus::Queued);
        assert!(result.is_err());
    }
}
