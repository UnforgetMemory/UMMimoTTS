# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased] - 2026-05-30

### Backend

#### Fixed
- **SSE Stream**: Rewrote `SseStream` with mpsc bridge + `ReceiverStream` pattern to fix events not arriving (original `try_recv()` didn't register waker properly)
- **SSE Route Conflict**: Fixed `/api/v2/events` returning 404 by removing conflicting `web::scope("/api/v2")` in sse.rs
- **Duplicate Chunking Event**: Removed duplicate `TaskStatusChanged{status:"chunking"}` emission in task_queue.rs
- **SSE Bridge**: Fixed `spawn_sse_bridge` never being called, causing SSE events to not reach frontend
- **DomainEvent Serialization**: Added `#[serde(tag = "type")]` for correct JSON format (`{"type": "TaskEnqueued", ...}`)
- **Batch Items Endpoint**: Fixed 400 error when submitting batch items (expects plain array, not wrapped object)
- **Delete Cascade**: Fixed foreign key constraint errors when deleting batches with tasks/chunks
- **Task Processing**: Fixed tasks stuck in Processing state with local tokenizer fallback

#### Added
- **TaskStatusChanged Event**: New `DomainEvent::TaskStatusChanged { task_id, batch_id, status }` for real-time queue flow visualization
- **Queue Flow Events**: `TaskQueue::enqueue()` now emits TaskStatusChanged at each transition: Pending→Queued (status="queued"), Queued→Chunking (status="chunking"), Chunking→Processing (status="processing")
- **Inter-task Delay**: Batch submit background loop now has 100ms `tokio::time::sleep` between each task enqueue, allowing frontend to observe gradual status transitions
- **SSE Routing**: TaskStatusChanged with batch_id routes to both `batch:{bid}` and `task:{task_id}` channels
- `DELETE /api/v2/batches/{id}` endpoint for batch deletion
- `POST /api/v2/batches/{id}/items/batch` endpoint for bulk item insertion
- Background task enqueue with immediate response on submit
- SSE events for TaskEnqueued, TaskCompleted, TaskFailed, ChunkCompleted

### Frontend

#### Fixed
- **GroupDetailPanel Scroll**: Added `min-h-0` chain on kanban wrapper, columns, and individual columns to fix internal scroll not working
- **API Client**: Changed `addBatchItems` to send plain array instead of wrapped object
- **Delete Group**: Changed from v1 `api.deleteGroup` to v2 `apiV2.deleteBatch`
- **SSE Event Handling**: Updated to match new DomainEvent format with `type` field
- **Task Status Mapping**: Aligned frontend status labels with backend (Queued, Chunking, Processing, Merging, Done)

#### Added
- **TaskStatusChanged Handler**: Added `case 'TaskStatusChanged'` to `subscribeToGroupEvents()` switch — updates individual task status in group cache in real-time
- **Virtual Scrolling**: Added `@tanstack/vue-virtual` to GroupDetailPanel kanban columns for performance with large task lists
- **GroupKanban Virtual Scrolling**: Sidebar kanban with virtual scrolling for batch groups

#### Changed
- **GroupCard**: Removed inline task expansion, clicking now opens detail panel
- **GroupDetailPanel**: Rewritten with kanban board layout (4 columns: Queued, Processing, Done, Failed)
- **Task Status Display**: Added progress bars for active statuses with animation
- **Layout**: Group detail panel now uses full available width

## [Previous] - 2026-05-27

### Backend
- Dynamic version endpoint (`/api/version`)
- Groups API with batch management
- SQLite persistence layer

### Frontend
- Batch import wizard
- Group management sidebar
- Task list with virtual scrolling
