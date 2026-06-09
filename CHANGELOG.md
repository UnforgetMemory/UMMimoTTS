# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased] - 2026-06-04

### Backend — Reliability & Provider Resilience

#### Added
- **Provider load balancer** (`provider_balancer.rs`): LeastConnections strategy with per-provider circuit breaker (5-failure threshold, 60s recovery timeout, half-open probe).
- **TCP link error handling**: Connection refused / reset errors now mapped to `ServerOverload` variant, triggering graceful degradation instead of panics.
- **Multi-layer degradation strategy**: 429/5xx → speed reduction; network errors → aggressive slowdown; global circuit breaker → pause all workers.
- **Provider rate limiter map**: Per-provider independent quota with exponential backoff recovery.

#### Fixed
- **Circuit breaker half-open recovery**: `on_success()` now clears `circuit_open_since` when half-open probe succeeds, allowing full CLOSED transition.
- **Atomic active_requests decrement**: `on_request_end()` uses `fetch_update` with `saturating_sub(1)` instead of non-atomic load-store pattern.
- **LRU cache O(1) operations**: Replaced `HashMap` + linear scan with `LinkedHashMap` for O(1) eviction.

### E2E Testing — Concurrency & Scale

#### Added
- **Concurrent user tests** (`concurrent_users.rs`): 5/10 virtual users via `tokio::join!` with full lifecycle (create → add → submit → poll).
- **Large-scale text fixture** (`large-text-data.ts`): Generators for 120-segment, 200-segment, and 1000-file stress tests with mixed Chinese/English content.
- **1000-file stress test** (`large-scale-text.spec.ts`): Batched upload (100/batch), memory monitoring, pagination stress.
- **Full-chain E2E tests** (`full-chain.spec.ts`): Single-user journey + 5-user concurrent E2E.
- **Frontend concurrent user tests** (`concurrent-users.spec.ts`): 3-user import, 5-user mixed (importer + viewer), rapid-click stress.
- **Performance metrics collector** (`metrics-collector.ts`): Record/aggregate/summarize timing data.
- **Batch task list page object** (`batch-task-list.page.ts`).

#### Fixed
- **`page.evaluate` scope bug**: External `userId` variable now passed as parameter to browser-context evaluation.
- **Temp directory cleanup**: Increased timeout 3s→10s, added `trackedDirs` + `afterAll` sweep for orphaned `pw-upload-*` directories.

### Infrastructure

#### Changed
- **`.gitignore`**: Added patterns for `*_stderr.txt`, `*_stdout.txt`, `*.ps1`, `backend_run*.txt`, `server_*.txt`.
- **`playwright.config.ts`**: Added `large-scale` project with 60s timeout.

---

## [Unreleased] - 2026-05-31

### Queue System Overhaul

#### Fixed
- **Rate limiter integer division bug**: `refill_per_sec` used `rpm/60` (integer division), causing 10 rpm to become 60 rpm. Replaced with `nanos_per_token = 60_000_000_000 / rpm` for precise nanosecond-based refill.
- **Rate limiter refill deadlock**: `last_refill` was updated every 100ms even when `earned == 0`, resetting the timer so tokens never accumulated. Now only updates via CAS when `earned > 0`.
- **Chunk queue deadlock**: Workers used `try_acquire()` + `notify.notified().await` which caused infinite wait when rate limiter had no tokens. Changed to `rate_limiter.acquire().await` (sleeps internally with 100ms retry).
- **Batch status never updated in DB**: `check_batch_completion()` emitted SSE events but never wrote to `batches` table. Added `UPDATE batches SET status` with timestamp.
- **Queue "parking lot" bug**: After `tokio::spawn`, workers looped back to sleep without waking next worker. Added `notify.notify_one()` after spawn.
- **Duplicate TaskEnqueued events**: Removed 3 redundant `sse_bus.publish(TaskEnqueued)` calls from `batch_service.rs`.
- **Misleading Chunking status**: Tasks showed "chunking" while waiting in queue. Removed Chunking transition — tasks stay Queued until worker picks first chunk.
- **Group ID mismatch**: Tasks had `group_id = batch_id` instead of actual group UUID. Fixed `batch_repo.rs` to look up group by batch_id.
- **Group counters NULL crash**: `increment_done/failed_tasks` SQL failed on NULL values. Added `COALESCE(done_tasks, 0)`.
- **Task-level concurrency gap**: No task-level gate — all tasks instantly became "processing". Added `task_semaphore` (20) + `active_tasks` HashSet.

#### Added
- **Chunk recovery module** (`chunk_recovery.rs`): Resets orphaned Processing chunks (>2min) to Pending every 30s.
- **Task watchdog** (`watchdog.rs`): Patrols every 15s for stuck Processing/Merging tasks (>60s). Marks Failed + emits events.
- **Poll and reconcile DB fallback**: Finds tasks with all chunks resolved, triggers completion. On broadcast Lagged + every 60s.
- **Cancel/stop APIs**: `POST /tasks/{id}/cancel`, `POST /tasks/cancel-all`, `POST /batches/{id}/cancel`.
- **Frontend cancel buttons**: Per-task cancel, per-group cancel, "一键清空" button.
- **Group status auto-transitions**: `check_group_completion()` auto-sets Completed/Failed when all tasks terminal.
- **Batch status auto-transitions**: `check_batch_completion()` now updates DB status.

#### Changed
- **Broadcast channel**: 256 → 4096
- **Rate limiter**: `TokenBucket::new(100)` → `TokenBucket::new(10)` (10 rpm)
- **Max concurrent chunks**: 2 → 10
- **Max active tasks**: New `MAX_ACTIVE_TASKS` env var, default 20
- **Watchdog timing**: patrol 30s→15s, stale 300s→60s
- **Pagination cap**: 1000 → 5000

### Frontend (2026-05-31)

#### Fixed
- **Group detail kanban empty data**: Used `batch_id` instead of `group_id` for `listTasks` API call.
- **Card/CardContent imports**: GroupCard.vue missing shadcn-vue component imports.

#### Changed
- **Layout refactoring**: Improved spacing, typography, visual hierarchy.
- **Loading skeleton**: Structured skeleton with column headers and card placeholders.
- **Kanban column height**: `flex-1` → `calc(100vh - 280px); max-height: 700px`.

---

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
