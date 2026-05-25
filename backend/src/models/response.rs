use crate::models::batch::GroupStatus;
use crate::models::task::TaskStatus;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Query parameters for task listing
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TaskListQuery {
    pub page: Option<usize>,
    pub per_page: Option<usize>,
    pub status: Option<String>,
    pub search: Option<String>,
    pub sort: Option<String>,
    pub group_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Lightweight TaskSummary (no text/context/model)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub id: String,
    pub custom_title: Option<String>,
    pub status: TaskStatus,
    pub voice: Option<String>,
    pub char_count: usize,
    pub token_count: usize,
    pub progress: f32,
    pub has_audio: bool,
    pub group_id: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub elapsed_secs: Option<f64>,
    pub current_chunk: Option<usize>,
    pub total_chunks: Option<usize>,
}

// ---------------------------------------------------------------------------
// Generic paginated response
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub items: Vec<T>,
    pub total: usize,
    pub page: usize,
    pub per_page: usize,
    pub total_pages: usize,
}

impl<T: Serialize> PaginatedResponse<T> {
    pub fn new(items: Vec<T>, total: usize, page: usize, per_page: usize) -> Self {
        let total_pages = if per_page > 0 {
            (total + per_page - 1) / per_page
        } else {
            1
        };
        Self {
            items,
            total,
            page,
            per_page,
            total_pages,
        }
    }
}

// ---------------------------------------------------------------------------
// Stats summary
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsSummary {
    pub total_tasks: usize,
    pub completed: usize,
    pub failed: usize,
    pub processing: usize,
    pub total_tokens: usize,
    pub total_chars: usize,
}

// ---------------------------------------------------------------------------
// Lightweight GroupSummary (no tasks embedded)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupSummary {
    pub id: String,
    pub name: String,
    pub status: GroupStatus,
    pub voice: Option<String>,
    pub model: String,
    pub context: Option<String>,
    pub created_at: String,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
    pub total_tokens: usize,
}

// ---------------------------------------------------------------------------
// Existing types (unchanged)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SynthesizeResponse {
    pub task_id: String,
    pub status: TaskStatus,
    pub token_count: usize,
    pub char_count: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResponse {
    pub id: String,
    pub custom_title: Option<String>,
    pub status: TaskStatus,
    pub model: String,
    pub voice: Option<String>,
    pub text: String,
    pub context: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub error: Option<String>,
    pub progress: f32,
    pub token_count: usize,
    pub char_count: usize,
    pub elapsed_secs: Option<f64>,
    pub has_audio: bool,
    // 分片进度信息
    pub total_chunks: Option<usize>,
    pub current_chunk: Option<usize>,
    /** 所属批量分组 ID */
    pub group_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VoiceInfo {
    pub id: String,
    pub name: String,
    pub language: String,
    pub gender: String,
    pub style: String,
    pub preview_url: Option<String>,  // CDN 预览音频 URL
}

#[derive(Debug, Serialize)]
pub struct VoiceListResponse {
    pub voices: Vec<VoiceInfo>,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
    pub code: Option<String>,
}
