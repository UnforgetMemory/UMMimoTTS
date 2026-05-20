use crate::models::task::TaskStatus;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SynthesizeResponse {
    pub task_id: String,
    pub status: TaskStatus,
    pub token_count: usize,
    pub char_count: usize,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct TaskResponse {
    pub id: String,
    pub status: TaskStatus,
    pub model: String,
    pub voice: Option<String>,
    pub text: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub error: Option<String>,
    pub progress: f32,
    pub token_count: usize,
    pub char_count: usize,
    pub elapsed_secs: Option<f64>,
    pub has_audio: bool,
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
