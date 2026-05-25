use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum GroupStatus {
    Pending,
    Processing,
    Paused,
    Completed,
    Failed,
}

impl GroupStatus {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(GroupStatus::Pending),
            "processing" => Some(GroupStatus::Processing),
            "paused" => Some(GroupStatus::Paused),
            "completed" => Some(GroupStatus::Completed),
            "failed" => Some(GroupStatus::Failed),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

impl std::fmt::Display for GroupStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "待处理"),
            Self::Processing => write!(f, "处理中"),
            Self::Paused => write!(f, "已暂停"),
            Self::Completed => write!(f, "已完成"),
            Self::Failed => write!(f, "失败"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchGroup {
    pub id: String,                    // UUIDv7
    pub name: String,                  // Custom or auto-generated name
    pub status: GroupStatus,
    pub voice: Option<String>,         // Default voice for group
    pub model: String,                 // Default model
    pub context: Option<String>,       // Default style/context
    pub created_at: DateTime<Utc>,
    pub task_ids: Vec<String>,         // Ordered list of task IDs
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
    pub total_tokens: usize,
}

impl BatchGroup {
    pub fn new(name: String, voice: Option<String>, model: String, context: Option<String>) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            name,
            status: GroupStatus::Pending,
            voice,
            model,
            context,
            created_at: Utc::now(),
            task_ids: Vec::new(),
            total_tasks: 0,
            completed_tasks: 0,
            failed_tasks: 0,
            total_tokens: 0,
        }
    }

    /// Convert to lightweight GroupSummary for list endpoints
    pub fn to_summary(&self) -> super::response::GroupSummary {
        super::response::GroupSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            status: self.status.clone(),
            voice: self.voice.clone(),
            model: self.model.clone(),
            context: self.context.clone(),
            created_at: self.created_at.to_rfc3339(),
            total_tasks: self.total_tasks,
            completed_tasks: self.completed_tasks,
            failed_tasks: self.failed_tasks,
            total_tokens: self.total_tokens,
        }
    }

    pub fn update_progress(&mut self, completed: usize, failed: usize, tokens: usize) {
        self.completed_tasks = completed;
        self.failed_tasks = failed;
        self.total_tokens = tokens;
        
        if self.completed_tasks + self.failed_tasks >= self.total_tasks {
            if self.failed_tasks == 0 {
                self.status = GroupStatus::Completed;
            } else if self.completed_tasks == 0 {
                self.status = GroupStatus::Failed;
            } else {
                self.status = GroupStatus::Completed; // Partial success
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchImportRequest {
    pub group_name: Option<String>,    // Custom name or auto UUIDv7
    pub voice: Option<String>,         // Group default voice
    pub model: String,                 // Group default model
    pub context: Option<String>,       // Group default style
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCreateResponse {
    pub group_id: String,
    pub group_name: String,
    pub task_count: usize,
    pub task_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupResponse {
    pub id: String,
    pub name: String,
    pub status: GroupStatus,
    pub voice: Option<String>,
    pub model: String,
    pub context: Option<String>,
    pub created_at: String,
    pub task_ids: Vec<String>,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
    pub total_tokens: usize,
    pub tasks: Vec<crate::models::response::TaskResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupListResponse {
    pub groups: Vec<GroupResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupUpdateRequest {
    pub name: Option<String>,
    pub voice: Option<String>,
    pub context: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_group_new() {
        let group = BatchGroup::new(
            "测试分组".to_string(),
            Some("nova".to_string()),
            "mimo-v2.5".to_string(),
            Some("测试上下文".to_string()),
        );

        assert!(!group.id.is_empty());
        assert_eq!(group.name, "测试分组");
        assert_eq!(group.voice, Some("nova".to_string()));
        assert_eq!(group.model, "mimo-v2.5".to_string());
        assert_eq!(group.context, Some("测试上下文".to_string()));
        assert_eq!(group.status, GroupStatus::Pending);
        assert!(group.task_ids.is_empty());
        assert_eq!(group.total_tasks, 0);
        assert_eq!(group.completed_tasks, 0);
        assert_eq!(group.failed_tasks, 0);
        assert_eq!(group.total_tokens, 0);
    }

    #[test]
    fn test_batch_group_update_progress() {
        let mut group = BatchGroup::new(
            "测试".to_string(),
            None,
            "mimo-v2.5".to_string(),
            None,
        );
        group.task_ids = vec!["task1".to_string(), "task2".to_string(), "task3".to_string()];
        group.total_tasks = 3;

        group.update_progress(2, 1, 5000);
        assert_eq!(group.completed_tasks, 2);
        assert_eq!(group.failed_tasks, 1);
        assert_eq!(group.total_tokens, 5000);
        assert_eq!(group.status, GroupStatus::Completed);
    }

    #[test]
    fn test_batch_group_status_display() {
        assert_eq!(GroupStatus::Pending.to_string(), "待处理");
        assert_eq!(GroupStatus::Processing.to_string(), "处理中");
        assert_eq!(GroupStatus::Paused.to_string(), "已暂停");
        assert_eq!(GroupStatus::Completed.to_string(), "已完成");
        assert_eq!(GroupStatus::Failed.to_string(), "失败");
    }

    #[test]
    fn test_batch_group_status_serialization() {
        let status = GroupStatus::Processing;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"processing\"");

        let deserialized: GroupStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, GroupStatus::Processing);
    }

    #[test]
    fn test_group_response_construction() {
        let mut group = BatchGroup::new(
            "测试".to_string(),
            Some("nova".to_string()),
            "mimo-v2.5".to_string(),
            None,
        );
        group.task_ids = vec!["task1".to_string()];
        group.total_tasks = 1;
        group.completed_tasks = 1;

        // Construct GroupResponse like the routes do
        let response = GroupResponse {
            id: group.id.clone(),
            name: group.name.clone(),
            status: group.status.clone(),
            voice: group.voice.clone(),
            model: group.model.clone(),
            context: group.context.clone(),
            created_at: group.created_at.to_rfc3339(),
            task_ids: group.task_ids.clone(),
            total_tasks: group.total_tasks,
            completed_tasks: group.completed_tasks,
            failed_tasks: group.failed_tasks,
            total_tokens: group.total_tokens,
            tasks: Vec::new(),
        };
        assert_eq!(response.name, "测试");
        assert_eq!(response.voice, Some("nova".to_string()));
        assert_eq!(response.model, "mimo-v2.5".to_string());
        assert_eq!(response.total_tasks, 1);
        assert_eq!(response.completed_tasks, 1);
    }
}
