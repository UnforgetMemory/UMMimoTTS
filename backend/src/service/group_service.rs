//! Group service — thin wrapper around GroupRepo.
//!
//! Groups organise tasks under a batch (e.g. "import session #3").
//! Currently provides basic CRUD: create a group and list groups by batch.

#![allow(dead_code)]

use crate::domain::group::Group;
use crate::infra::persistence::group_repo::GroupRepo;
use crate::shared::error::AppError;
use crate::shared::id::Id;
use std::sync::Arc;

/// Stateless service wrapping GroupRepo.
pub struct GroupService {
    group_repo: Arc<dyn GroupRepo>,
}

impl GroupService {
    pub fn new(group_repo: Arc<dyn GroupRepo>) -> Self {
        Self { group_repo }
    }

    /// Create a new group under the given batch.
    pub fn create(&self, batch_id: &str, title: &str) -> Result<Group, AppError> {
        let id = Id::from_str(batch_id)?;
        let group = Group::new(id, title.to_string());
        self.group_repo.insert(&group)?;
        Ok(group)
    }

    /// List groups. If batch_id is provided, filter by batch; otherwise return all.
    pub fn list(&self, batch_id: Option<&str>) -> Result<Vec<Group>, AppError> {
        match batch_id {
            Some(id) => self.group_repo.find_by_batch(id),
            None => self.group_repo.find_all(),
        }
    }
}
