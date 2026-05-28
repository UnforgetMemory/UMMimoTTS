# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased] - 2026-05-28

### Backend

#### Fixed
- **SSE Bridge**: Fixed `spawn_sse_bridge` never being called, causing SSE events to not reach frontend
- **DomainEvent Serialization**: Added `#[serde(tag = "type")]` for correct JSON format (`{"type": "TaskEnqueued", ...}`)
- **Batch Items Endpoint**: Fixed 400 error when submitting batch items (expects plain array, not wrapped object)
- **Delete Cascade**: Fixed foreign key constraint errors when deleting batches with tasks/chunks
- **Task Processing**: Fixed tasks stuck in Processing state with local tokenizer fallback

#### Added
- `DELETE /api/v2/batches/{id}` endpoint for batch deletion
- `POST /api/v2/batches/{id}/items/batch` endpoint for bulk item insertion
- Background task enqueue with immediate response on submit
- SSE events for TaskEnqueued, TaskCompleted, TaskFailed, ChunkCompleted

### Frontend

#### Fixed
- **API Client**: Changed `addBatchItems` to send plain array instead of wrapped object
- **Delete Group**: Changed from v1 `api.deleteGroup` to v2 `apiV2.deleteBatch`
- **SSE Event Handling**: Updated to match new DomainEvent format with `type` field
- **Task Status Mapping**: Aligned frontend status labels with backend (Queued, Chunking, Processing, Merging, Done)

#### Changed
- **GroupCard**: Removed inline task expansion, clicking now opens detail panel
- **GroupDetailPanel**: Rewritten with kanban board layout (4 columns: Queued, Processing, Done, Failed)
- **Task Status Display**: Added progress bars for active statuses with animation
- **Layout**: Group detail panel now uses full available width

#### Added
- Real-time task status updates via SSE
- Kanban columns with task counts and status indicators
- Progress animation for active tasks (queued/chunking/processing/merging)

## [Previous] - 2026-05-27

### Backend
- Dynamic version endpoint (`/api/version`)
- Groups API with batch management
- SQLite persistence layer

### Frontend
- Batch import wizard
- Group management sidebar
- Task list with virtual scrolling
