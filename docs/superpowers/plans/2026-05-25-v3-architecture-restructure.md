# v3 Architecture Restructure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax for tracking.  
> **Workflow rule:** Every code change follows: test-first → implement → verify → code review → regression. Failures trigger 手术刀 cycle (diagnose → fix → review → regress).  
> **Frontend:** Not in scope. All testing via API calls (actix-test integration tests).

**Goal:** Replace monolithic `models.rs` + `sled` + `BatchQueue` with layered DDD architecture: Batch Queue → Task Queue → Chunk Queue, SQLite persistence, UUIDv7 IDs, data-driven event model, and complete batch lifecycle.

**Context:** `backend/src/` — actix-web server, ~3500 lines. `rusqlite` + `r2d2` already in `Cargo.toml`. Dev deps: `actix-rt`, `actix-test`. No frontend changes in this plan. `models.rs` is a god file (~1000+ lines), routes mix HTTP+logic+SQL, queue is monolithic.

---

## Architecture Overview

```
HTTP Routes (actix-web)
     │
     ▼
Service Layer (orchestration)
     │
     ├── batch_service  →  Batch Lifecycle (4 phases)
     │       │              prepare → upload → edit → submit
     │       │              拆解为 N 个 Task →投递到 Task Queue
     │       ▼
     ├── task_service   →  Task Queue
     │       │             Chunker: 1 task → N chunks (N≥1)
     │       ▼
     └── chunk_service  →  Chunk Queue
                            rate-limited dispatch → MIMO TTS API
                            cache result → mark done → auto-next
```

### Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| **Single ChunkQueue for all tasks** | Short text = 1 chunk, long text = N chunks. Same pipeline. No separate "common" and "MIMO" queues. |
| **Chunk as minimal work unit** | Each chunk persisted independently. Crash recovery = scan chunks, retry pending/processing. |
| **Chunker before enqueue** | Task enters TaskQueue → immediate chunking → all chunks written to DB → ChunkQueue picks up chunks. |
| **Data-driven event model** | Chunk done → auto-check task completion → auto-dispatch next chunk or trigger merge. No external triggers. |
| **Two-level cache** | Memory (hot) + Disk (warm, `cache/wav/{task_id}/{chunk_seq}.wav`). Path stored in SQLite. TTL configurable (default 24h). |
| **UUIDv7 for all entities** | Time-sortable unique IDs. Sortable by creation time. No more `usize` indices or random tokens. |
| **SQLite fresh start** | v3 new database file. No migration from sled. Fresh DB on first start. |
| **No token for batch session** | `batch_id` (UUIDv7) is the identity token. No separate `PendingImport` + `extend-session` complexity. |
| **Custom task overrides** | Two-tier: batch defaults → item `custom_*` → `effective_*` (computed on write for fast reads). |
| **No frontend in scope** | All testing via Rust integration tests using `actix-test`. APIs must be fully testable without a browser. |

### Batch Lifecycle — Four Phases

```
Phase 1: Prepare              Phase 2: Edit              Phase 3: Submit            Phase 4: Track
─────────────                 ──────────                 ──────────                  ──────────
POST /batches                 GET /batches/{id}/items     POST /batches/{id}/submit   GET /batches/{id}
POST /batches/{id}/files      PATCH /batches/{id}/items/1                              (aggregated progress)
GET  /batches/{id}            PATCH /batches/{id}/items   creates tasks               POST /batches/{id}/continue
DELETE /batches/{id}           (batch update)             enqueues to TaskQueue       (retry failed)
                              DELETE /batches/{id}/items/1
```

### Three-Queue Hierarchy

```
Batch Service         Task Queue            Chunk Queue
─────────────         ──────────            ───────────
batch.submit()        task.submit()         chunk.enqueue()
  │                      │                      │
  ├─ validate items      ├─ chunker.split()     ├─ rate_limiter.acquire()
  ├─ create N tasks      ├─ write N chunks      ├─ dispatch → MIMO API
  └─ push to TaskQ       ├─ push to ChunkQ      ├─ save WAV → cache
                         └─ listen: all done?   ├─ mark done
                              → trigger merge    └─ auto-next (loop)
```

### WAV File Lifecycle

Every task produces a single WAV output file. The lifecycle traces through chunk generation, caching, merge, and final output.

```
Source Text (task.content)
      │
      ▼
┌──────────────┐
│  Chunker     │  Phase 1: Tokenize via MIMO SDK /v1/tokenize
│  (MimoChunker)│            Split into 2000-token segments
└──────┬───────┘
       │ N chunks
       ▼
┌──────────────┐
│  Cached?     │──Yes──→ reuse existing WAV, skip synthesis
│  (Cache hit) │
└──────┬───────┘
       │ No (cache miss)
       ▼
┌──────────────┐
│  MIMO API    │  Phase 2: Synthesize via /v1/audio/speech
│  Per Chunk   │            Input: { text, voice, model, speed }
└──────┬───────┘
       │
       ▼
┌──────────────┐
│  WAV Output  │  Phase 3: Raw WAV response saved to disk
│  Per Chunk   │            Path: cache/wav/{task_id}/{seq}.wav
└──────┬───────┘
       │ stored in chunk.audio_path + cache memory
       ▼
┌──────────────┐
│  All Chunks  │  Phase 4: Wait for AllChunksDone event
│  Done?       │            TaskQueue.listen triggers merge_task_audio()
└──────┬───────┘
       │ Yes
       ▼
┌──────────────┐
│  Merger      │  Phase 5: Concatenate WAV files in seq order
│  merge_wavs()│            Reads all cache/wav/{task_id}/*.wav
└──────┬───────┘
       │
       ▼
┌──────────────┐
│  Final WAV   │  Phase 6: output/wav/{task_id}/merged.wav
│              │            task.output_path = this path
│              │            task.output_duration = total seconds
└──────┬───────┘
       │
       ▼
Post-processing: SSE event (TaskDone) → frontend notification
                 REST download at GET /api/v2/tasks/{id}/download
```

**Disk layout:**
```
output/
├── wav/
│   ├── {task_id}/
│   │   └── merged.wav          ← final audio (Phase 6)
│   └── ...
cache/
├── wav/
│   ├── {task_id}/
│   │   ├── 0.wav               ← chunk 0 (Phase 3)
│   │   ├── 1.wav               ← chunk 1
│   │   └── 2.wav               ← chunk 2
│   └── ...
└── memory: LRU+TTL hash map    ← hot cache (Phase 3.4)
```

**Cleanup policy:**
| Item | Retention | Trigger |
|------|-----------|---------|
| Chunk WAV (cache) | Merge done OR TTL expired | Background cleaner (enforce_memory_limit + ttl_checker_loop) |
| Final WAV (output) | Until domain event expires | Not auto-deleted; user manages via API or TTL |
| Memory cache entry | TTL (default 24h) + LRU | Cache::enforce_memory_limit on put/get-miss |

**Crash recovery WAV behavior:**
1. On `continue_task()`: for each Done chunk, verify `chunk.audio_path` exists on disk.
2. If missing: reset chunk to Pending, re-enqueue for synthesis.
3. If present: skip (cache hit).
4. Merge step re-runs if all chunks Done but no merged output found: detect via `task.status == Merging && !output_path.exists()`.

**Note on Smart Chunking:** The MIMO SDK /v1/tokenize endpoint provides accurate per-segment token counts, eliminating the need for heuristic character-count-to-token estimates and reducing edge cases where chunks exceed model context limits.

### Common Entity Attributes (Every Level)

```
Level     ID         Chars      Tokens     Content Ref
─────     ──         ─────      ──────     ───────────
Batch     batch_id   total      total      file list
Task      task_id    total      total      original text
Chunk     chunk_id   char_count token_count segment text
```

---

## SQLite Schema

```sql
-- ========== BATCH TABLE ==========
CREATE TABLE batches (
    id              TEXT PRIMARY KEY,
    status          TEXT NOT NULL DEFAULT 'preparing',
    -- preparing → queued → processing → done | failed | cancelled
    voice           TEXT NOT NULL,
    model           TEXT NOT NULL,
    style           TEXT,
    speed           REAL NOT NULL DEFAULT 1.0,
    total_items     INTEGER NOT NULL DEFAULT 0,
    total_chars     INTEGER NOT NULL DEFAULT 0,
    total_tokens    INTEGER NOT NULL DEFAULT 0,
    done_tasks      INTEGER NOT NULL DEFAULT 0,
    failed_tasks    INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    completed_at    TEXT
);

-- ========== BATCH PENDING ITEMS (pre-submit cache) ==========
-- All computation here. Items editable until submit.
CREATE TABLE batch_pending_items (
    id              TEXT PRIMARY KEY,
    batch_id        TEXT NOT NULL,
    seq             INTEGER NOT NULL,
    filename        TEXT NOT NULL,
    content         TEXT NOT NULL,
    text_preview    TEXT NOT NULL,
    total_chars     INTEGER NOT NULL DEFAULT 0,
    token_estimate  INTEGER NOT NULL DEFAULT 0,
    custom_title    TEXT,
    custom_voice    TEXT,
    custom_model    TEXT,
    custom_style    TEXT,
    custom_speed    REAL,
    effective_title    TEXT NOT NULL,
    effective_voice    TEXT NOT NULL,
    effective_model    TEXT NOT NULL,
    effective_style    TEXT,
    effective_speed    REAL NOT NULL DEFAULT 1.0,
    status          TEXT NOT NULL DEFAULT 'pending',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    UNIQUE(batch_id, seq),
    FOREIGN KEY (batch_id) REFERENCES batches(id)
);

-- ========== CORE TASK TABLE ==========
CREATE TABLE tasks (
    id              TEXT PRIMARY KEY,
    task_type       TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending',
    group_id        TEXT,
    batch_id        TEXT,
    content         TEXT NOT NULL,
    content_ref     TEXT,
    title           TEXT NOT NULL,
    voice           TEXT NOT NULL,
    model           TEXT NOT NULL,
    style           TEXT,
    speed           REAL NOT NULL DEFAULT 1.0,
    priority        INTEGER NOT NULL DEFAULT 0,
    total_chars     INTEGER NOT NULL DEFAULT 0,
    total_tokens    INTEGER NOT NULL DEFAULT 0,
    total_chunks    INTEGER NOT NULL DEFAULT 0,
    done_chunks     INTEGER NOT NULL DEFAULT 0,
    failed_chunks   INTEGER NOT NULL DEFAULT 0,
    output_path     TEXT,
    output_duration REAL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    completed_at    TEXT
);

-- ========== BATCH-TASK ASSOCIATION ==========
CREATE TABLE batch_tasks (
    id              TEXT PRIMARY KEY,
    batch_id        TEXT NOT NULL,
    child_task_id   TEXT NOT NULL UNIQUE,
    seq             INTEGER NOT NULL,
    FOREIGN KEY (child_task_id) REFERENCES tasks(id)
);

-- ========== CHUNKS ==========
CREATE TABLE chunks (
    id              TEXT PRIMARY KEY,
    task_id         TEXT NOT NULL,
    seq             INTEGER NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending',
    text            TEXT NOT NULL,
    char_count      INTEGER NOT NULL DEFAULT 0,
    token_count     INTEGER NOT NULL DEFAULT 0,
    retry_count     INTEGER NOT NULL DEFAULT 0,
    max_retries     INTEGER NOT NULL DEFAULT 3,
    priority        INTEGER NOT NULL DEFAULT 0,   -- -1=bulk, 0=normal, 1=high
    audio_path      TEXT,
    audio_duration  REAL,
    error_msg       TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    FOREIGN KEY (task_id) REFERENCES tasks(id)
);
CREATE INDEX idx_chunks_priority ON chunks(priority DESC, created_at ASC)
    WHERE status = 'pending';

-- ========== GROUPS ==========
CREATE TABLE groups (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'active',
    type            TEXT NOT NULL DEFAULT 'custom',
    voice           TEXT,
    model           TEXT,
    speed           REAL DEFAULT 1.0,
    total_tasks     INTEGER DEFAULT 0,
    done_tasks      INTEGER DEFAULT 0,
    failed_tasks    INTEGER DEFAULT 0,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

-- ========== QUEUE PERSISTENCE (for crash recovery) ==========
CREATE TABLE task_queue (
    id              TEXT PRIMARY KEY,
    task_id         TEXT NOT NULL UNIQUE,
    priority        INTEGER NOT NULL DEFAULT 0,
    status          TEXT NOT NULL DEFAULT 'pending',
    created_at      TEXT NOT NULL,
    FOREIGN KEY (task_id) REFERENCES tasks(id)
);

CREATE TABLE chunk_queue (
    id              TEXT PRIMARY KEY,
    chunk_id        TEXT NOT NULL UNIQUE,
    task_id         TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending',
    created_at      TEXT NOT NULL,
    FOREIGN KEY (chunk_id) REFERENCES chunks(id)
);
```

---

## Complete API Specification

### Phase 1: Prepare

```
POST /api/v1/batches
  Request:  { "voice": "xx-01", "model": "tts-1", "style": "活泼", "speed": 1.0 }
  Response 201: { "id": "uuidv7", "status": "preparing", "voice": "...", "total_items": 0, ... }

POST /api/v1/batches/{batch_id}/files
  Request: multipart/form-data: files: File[] (multiple .txt, max 500KB each)
  Response 202: { "batch_id": "...", "status": "parsing" }
  SSE events (on batch topic):
    file_parsed:  { "filename": "a.txt", "seq": 1, "chars": 12345, "tokens": 16000 }
    file_too_large: { "filename": "big.txt", "size": 1048576, "max_size": 512000 }
    parsing_complete: { "total": 5, "parsed": 4, "failed": 1 }

GET /api/v1/batches/{batch_id}
  Response: { "id": "...", "status": "preparing",
    "defaults": { "voice": "xx-01", ... },
    "total_items": 5, "total_chars": 123456, "total_tokens": 160000, ... }

DELETE /api/v1/batches/{batch_id}
  Response 204  (cancels batch, deletes pending items)
```

### Phase 2: Edit

```
GET /api/v1/batches/{batch_id}/items?page=1&per_page=50
  Response: {
    "items": [{ "seq": 1, "filename": "a.txt", "text_preview": "从前...",
      "total_chars": 12345, "token_estimate": 16000,
      "effective_title": "a.txt", "effective_voice": "xx-01", ...,
      "custom_title": null, ... }],
    "total": 50, "page": 1, "per_page": 50, "total_pages": 1
  }

PATCH /api/v1/batches/{batch_id}/items/{seq}
  Request:  { "title": "第一章 初入异世", "voice": "xx-02", "style": "深情" }
  Response: { "seq": 1, "effective_title": "第一章 初入异世", "effective_voice": "xx-02", ... }

PATCH /api/v1/batches/{batch_id}/items
  Request:  { "seqs": [1,3,5], "voice": "xx-03" }  // omit seqs → apply to ALL pending
  Response: { "modified": 3 }

DELETE /api/v1/batches/{batch_id}/items/{seq}
  Response 204
```

### Phase 3: Submit

```
POST /api/v1/batches/{batch_id}/submit
  Processing:
    1. Validate: batch exists + status=preparing + at least 1 pending item
    2. For each pending item (seq asc):
       a. Create tasks row: task_type='batch_child', effective_* values, content, title
       b. Insert batch_tasks association
    3. Update batch: status=queued, aggregate totals
    4. Delete or mark submitted on pending_items
    5. Enqueue each task to TaskQueue
  Response 200: { "batch_id": "...", "status": "queued",
    "total_tasks": 5, "total_chars": 123456, "total_tokens": 160000 }
```

### Phase 4: Track

```
GET /api/v1/batches/{batch_id}
  Response: { "id": "...", "status": "processing",
    "total_tasks": 5, "done_tasks": 2, "failed_tasks": 0,
    "total_chunks": 25, "done_chunks": 12,
    "total_chars": 123456, "total_tokens": 160000 }

POST /api/v1/batches/{batch_id}/continue
  Processing per failed task:
    1. Load all chunks
    2. Check cache for each done chunk → cache miss = reset to pending
    3. Re-enqueue pending/failed chunks to ChunkQueue
  Response: { "batch_id": "...", "continued_tasks": 2, "status": "processing" }
```

---

## Testing Strategy

### Three Testing Layers

```
Layer 1: Unit Tests (#[cfg(test)] mod tests in each file)
  ┌─────────────────────────────────────────────────────────────┐
  │ Each file has its own test module.                          │
  │ Tests are fast (<1ms each), no I/O, no DB.                  │
  │ Run via: cargo test --lib                                    │
  ├─────────────────────────────────────────────────────────────┤
  │ Domain tests:    status machines, effective fields, events  │
  │ Chunker tests:   splitting correctness, edge cases           │
  │ Cache tests:     put/get/evict, TTL, disk persistence        │
  │ Merger tests:    WAV concat with synthetic test files        │
  │ Rate limiter:    acquire/block/refill timing                 │
  └─────────────────────────────────────────────────────────────┘

Layer 2: Integration Tests (tests/ directory, real SQLite)
  ┌─────────────────────────────────────────────────────────────┐
  │ Each module has integration tests with in-memory SQLite.     │
  │ Run via: cargo test --test <module>                          │
  ├─────────────────────────────────────────────────────────────┤
  │ Repo tests:   insert/find/update/delete against real SQLite │
  │ ChunkQueue:   enqueue/dequeue/process/recover (mock API)    │
  │ TaskQueue:    chunk → enqueue → listen → merge (mock API)   │
  │ MIMO client:  mock server with wiremock                     │
  └─────────────────────────────────────────────────────────────┘

Layer 3: E2E API Tests (tests/ directory, actix-test)
  ┌─────────────────────────────────────────────────────────────┐
  │ Full app initialization with test DB.                       │
  │ Run via: cargo test --test e2e                              │
  ├─────────────────────────────────────────────────────────────┤
  │ test_e2e_batch_full_flow:   create → upload → edit → submit │
  │ test_e2e_batch_validation:  invalid inputs → proper errors   │
  │ test_e2e_task_lifecycle:    create → progress → done        │
  │ test_e2e_sse_events:        subscribe → publish → receive   │
  │ test_e2e_rate_limiting:     exceed limit → 429 returned     │
  │ test_e2e_crash_recovery:    simulate crash → restart →      │
  │                             verify pending recovery         │
  └─────────────────────────────────────────────────────────────┘
```

### Test Configuration (Shared)

```rust
// tests/common/mod.rs
pub fn setup_test_db() -> DbPool {
    let manager = SqliteConnectionManager::memory();
    let pool = Pool::builder().max_size(4).build(manager).unwrap();
    let conn = pool.get().unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").unwrap();
    run_migrations(&conn).unwrap();
    pool
}

pub fn setup_test_app() -> AppInstance {
    let pool = setup_test_db();
    let config = AppConfig::test_default();
    let app_state = AppState::new_for_test(pool, config);
    // ... init services with mock MimoClient
    let app = test::init_service(
        App::new().app_data(app_state).configure(routes::configure)
    ).await;
    AppInstance { app, state: app_state }
}
```

### 手术刀修复 Cycle (MANDATORY)

```
FOR EVERY TEST FAILURE, execute this exact sequence:

  ┌──────────────────────────────────────────────────────────┐
  │  1. TEST FAILS                                           │
  │     ├─ Record exact error: $error_msg                    │
  │     └─ Record stack trace                                │
  │                                                          │
  │  2. DIAGNOSE (read-only, no code changes)                │
  │     ├─ Identify root cause                               │
  │     ├─ Is it a test bug? → fix test expectation          │
  │     └─ Is it an implementation bug? → continue           │
  │                                                          │
  │  3. FIX (minimal change only)                            │
  │     ├─ Change only the code needed for THIS test         │
  │     ├─ NO refactoring beyond the fix scope               │
  │     ├─ NO scope creep or "while I'm here"                │
  │     └─ NO `#[allow(...)]` or `.unwrap()` additions       │
  │                                                          │
  │  4. SELF CODE REVIEW (checklist)                         │
  │     ├─ Does fix directly address root cause?             │
  │     ├─ Any side effects on other tests?                  │
  │     ├─ Error handling proper (no silent failures)?       │
  │     ├─ LSP diagnostics clean on all changed files        │
  │     └─ Types correct, no unnecessary clones/allocations  │
  │                                                          │
  │  5. REGRESSION TEST                                      │
  │     ├─ cargo test --all-features                         │
  │     ├─ If regression (new failures) → go to step 2       │
  │     └─ If all pass → proceed                             │
  │                                                          │
  │  6. COMMIT                                               │
  │     ├─ git add -A                                        │
  │     └─ git commit -m "phase-N: brief description"        │
  │                                                          │
  └──────────────────────────────────────────────────────────┘
```

---

## Coding Standards

### File Structure (Every File)

```rust
// 1. Imports (std → external → crate)
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use crate::shared::error::AppError;

// 2. Types
pub struct TaskService { ... }

// 3. impl blocks
impl TaskService { ... }

// 4. Private helpers
fn validate_status_transition(from: &TaskStatus, to: &TaskStatus) -> Result<(), AppError> { ... }

// 5. Tests
#[cfg(test)]
mod tests { ... }
```

### Error Handling Rules

```rust
// ✅ CORRECT: Propagate errors with ?
fn find_task(repo: &TaskRepo, id: &str) -> Result<Task, AppError> {
    repo.find_by_id(id)?
        .ok_or(AppError::NotFound(format!("Task {} not found", id)))
}

// ❌ WRONG: Silent unwrap
fn find_task_bad(repo: &TaskRepo, id: &str) -> Task {
    repo.find_by_id(id).unwrap().unwrap()  // NO
}

// ✅ CORRECT: Test code can unwrap (test will panic on failure)
#[cfg(test)]
mod tests {
    fn test_find() {
        let task = repo.find_by_id("x").unwrap().unwrap();
    }
}
```

### AppError Definition

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Rate limited")]
    RateLimited,
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        }
    }
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(serde_json::json!({
            "error": self.to_string(),
            "code": format!("{:?}", self),
        }))
    }
}
```

### Async/Sync Boundary

```rust
// SQLite operations are sync (r2d2 blocks within pool limits)
// Wrap long-running CPU work in spawn_blocking
// Queue dispatch loops are async

// Repo trait: all methods sync
pub trait TaskRepo: Send + Sync {
    fn insert(&self, task: &Task) -> Result<(), AppError>;
    fn find_by_id(&self, id: &str) -> Result<Option<Task>, AppError>;
}

// Service: async (may call sync repo, async queue, or spawn_blocking)
pub struct TaskService {
    repo: Box<dyn TaskRepo>,
    queue: Arc<TaskQueue>,
}

impl TaskService {
    pub async fn create(&self, req: CreateTaskRequest) -> Result<Task, AppError> {
        let task = Task::new(req);
        self.repo.insert(&task)?;                   // sync DB
        self.queue.enqueue(task.id.as_str()).await?; // async queue
        Ok(task)
    }
}
```

### Testing Conventions

```rust
// Naming:
//   test_{module}_{scenario}  (snake_case)
#[test]
fn test_chunker_splits_long_text_into_multiple_chunks() { ... }
#[test]
fn test_repo_insert_and_find_by_id() { ... }

// Assert style:
assert_eq!(actual, expected, "context message: what we're checking");
assert!(condition, "why this must be true: explain");

// Always use helper functions for test setup:
fn create_test_task() -> Task { ... }
fn create_test_batch(item_count: i32) -> (Batch, Vec<BatchPendingItem>) { ... }
```

---

## File Structure

```
backend/src/
├── main.rs
├── config.rs
├── app.rs                   ← AppState: repos + services + queues
│
├── domain/
│   ├── mod.rs
│   ├── task.rs              ← Task aggregate + TaskStatus machine
│   ├── chunk.rs             ← Chunk value object + ChunkStatus
│   ├── batch.rs             ← Batch + BatchPendingItem + BatchStatus
│   ├── group.rs             ← Group aggregate
│   └── events.rs            ← DomainEvent enum
│
├── service/
│   ├── mod.rs
│   ├── batch_service.rs     ← create/upload/edit/submit/continue
│   ├── task_service.rs      ← create/query/status transitions
│   ├── chunk_service.rs     ← chunk dispatch lifecycle
│   └── group_service.rs
│
├── routes/
│   ├── mod.rs
│   ├── batches.rs           ← POST /batches, GET/DELETE /batches/{id}
│   ├── batch_items.rs       ← files, items CRUD, submit, continue
│   ├── tasks.rs
│   ├── groups.rs
│   ├── sse.rs
│   └── voices.rs
│
├── infra/
│   ├── mod.rs
│   ├── persistence/
│   │   ├── mod.rs
│   │   ├── db.rs            ← r2d2 pool init, WAL mode
│   │   ├── task_repo.rs
│   │   ├── chunk_repo.rs
│   │   ├── batch_repo.rs
│   │   ├── group_repo.rs
│   │   └── migrate.rs       ← all CREATE TABLEs
│   ├── queue/
│   │   ├── mod.rs
│   │   ├── task_queue.rs    ← chunk → enqueue → listen → merge
│   │   ├── chunk_queue.rs   ← dispatch loop: dequeue → API → cache
│   │   └── rate_limiter.rs
│   ├── mimo/
│   │   ├── mod.rs
│   │   ├── client.rs        ← OpenAI TTS API wrapper
│   │   └── chunker.rs       ← smart text splitting
│   ├── audio/
│   │   ├── mod.rs
│   │   └── merger.rs        ← WAV concatenation
│   ├── cache.rs             ← two-level (mem + disk)
│   └── sse_bus.rs           ← topic-based pub/sub
│
└── shared/
    ├── mod.rs
    ├── error.rs             ← AppError
    └── id.rs                ← UUIDv7

backend/tests/
├── common/
│   └── mod.rs               ← setup_test_db(), setup_test_app(), mock MimoClient
├── e2e/
│   ├── batch_flow.rs        ← full batch lifecycle E2E tests
│   ├── task_lifecycle.rs
│   └── sse_events.rs
└── integration/
    ├── chunk_queue_tests.rs
    ├── task_queue_tests.rs
    └── repo_tests.rs
```

---

## State Machine Design (Three-Layer)

### Design Principles

1. **Data-driven event model** — Chunk Done → auto-check Task → auto-check Batch. No external polling or cron.
2. **All states persisted to SQLite** — Crash recovery reads DB truth; ChunkQueue rebuilt from scratch.
3. **Three-layer independent pause** — Group pause cascades to Tasks and Chunk dispatch. Task pause stops only its new chunks. Chunks never individually paused (too granular).
4. **Idempotent recovery** — On restart, `Processing` chunks → `Queued`. Accepts occasional duplicate API calls over data loss.

---

### Layer 1: Chunk Status Machine

A Chunk = one MIMO TTS API invocation unit.

```
                    ┌──────────┐
                    │ Pending  │  ← chunker created, not queued
                    └────┬─────┘
                         │ ChunkQueue.enqueue()
                    ┌────▼─────┐
                    │ Queued   │  ← waiting in queue
                    └────┬─────┘
                         │ dispatcher picks up, calls MIMO API
                    ┌────▼──────────┐
                    │ Processing    │  ← API in-flight
                    └────┬──────────┘
                     ┌────┴─────┐
                    ╱            ╲
              ┌────▼───┐    ┌────▼────┐
              │ Done   │    │ Failed  │──retry──→ Queued
              │(cached)│    │(retry) │
              └────────┘    └────┬────┘
                                 │ retries exhausted
                          ┌──────▼──────┐
                          │   Dead      │  ← abandoned permanently
                          └─────────────┘
```

```rust
enum ChunkStatus {
    Pending,    // created by chunker, not in chunk_queue table yet
    Queued,     // in chunk_queue, waiting for dispatcher
    Processing, // dispatcher picked up, MIMO API call in-flight
    Done,       // API success, WAV cached
    Failed,     // API error / timeout (retryable)
    Dead,       // retry_count >= max_retries, abandoned
}
```

| from → to | trigger |
|---|---|
| Pending → Queued | chunker writes to `chunk_queue` table |
| Queued → Processing | dispatcher dequeues |
| Processing → Done | MIMO API returns valid audio |
| Processing → Failed | API error / timeout |
| Failed → Queued | retry_count < max_retries (default 3) |
| Failed → Dead | retry_count >= max_retries |
| Processing → Queued | **Crash recovery only**: distrust in-flight on restart |

**NOT needed:** `Paused` (too granular — Task layer controls dispatch throttling)

---

### Layer 2: Task Status Machine

A Task = one file / one text block's complete lifecycle.

```
                    ┌──────────────┐
                    │   Pending    │  ← created, not enqueued yet
                    └──────┬───────┘
                           │ enqueue to TaskQueue
                    ┌──────▼───────┐
                    │   Queued     │  ← waiting in TaskQueue
                    └──────┬───────┘
                           │ dispatcher picks up, starts chunker
                    ┌──────▼───────┐
                    │  Chunking    │  ← split_text_into_chunks
                    └──────┬───────┘
                           │ all chunks written to DB
                    ┌──────▼───────┐
                    │  Processing  │◄───────────────────────┐
                    └──────┬───────┘                        │
                    ┌──────┴────────┐                      │
                   ╱                 ╲                     │
          ┌────────▼──┐         ┌─────▼────────┐          │
          │ All Chunks│         │ Some Chunks  │──retry───┘
          │ Done      │         │ Failed       │(retry failed)
          └────────┬──┘         │(partial ok)  │
                   │            └──────────────┘
          ┌────────▼──┐                │
          │  Merging   │               │
          │(merge audio)│              │
          └────────┬───┘               │
                   │                   │
          ┌────────▼──┐       ┌───────▼──────┐
          │   Done    │       │   Failed     │
          │(complete) │       │(all failed)  │
          └───────────┘       └──────────────┘

    Pause/Resume/Cancel (horizontal):

    Queued ──→ Paused ──→ Queued (resume)
    Chunking ──→ Paused ──→ Chunking (resume)
    Processing ──→ Paused ──→ Processing (resume)
    * ──→ Cancelled
    Failed ──→ Queued (manual retry)
    Cancelled ──→ (terminal, no outgoing)
```

```rust
enum TaskStatus {
    Pending,    // created, awaiting TaskQueue
    Queued,     // in TaskQueue, awaiting chunker
    Chunking,   // chunker running split_text_into_chunks
    Processing, // ≥1 chunk in ChunkQueue or being processed
    Merging,    // all chunks Done, merging final audio
    MergingFailed, // merge attempt failed, waiting for retry_merge
    Paused,     // paused by user or parent group
    Done,       // Merging succeeded, task complete
    Failed,     // all chunks Failed/Dead, unrecoverable
    Cancelled,  // cancelled by user or parent group
}

| from → to | trigger |
|---|---|
| Pending → Queued | enqueue to TaskQueue |
| Queued → Chunking | dispatcher picks up, starts splitting |
| Chunking → Processing | all chunks persisted, ChunkQueue begins |
| Processing → Merging | `done+failed == total && done > 0` |
| Merging → Done | audio merge success |
| Merging → MergingFailed | audio merge failure |
| MergingFailed → Done | retry_merge success |
| MergingFailed → MergingFailed | retry_merge failure (retry_count incremented) |
| Processing → Failed | `done_chunks == 0 && all chunks terminal` |
| Queued → Paused | user/group pause |
| Chunking → Paused | user/group pause |
| Processing → Paused | user/group pause (in-flight chunks finish) |
| Merging → Paused | user/group pause |
| MergingFailed → Paused | user/group pause |
| Paused → Queued | resume → re-enqueue |
| Paused → Chunking | resume if was chunking |
| Paused → Processing | resume if had running chunks |
| Paused → Merging | resume if was merging |
| Paused → MergingFailed | resume if was merging-failed |
| Failed → Queued | user retry |
| * → Cancelled | user/group cancel |

---

### Layer 3: Batch / Group Status Machine

A Batch = a set of Tasks sharing default parameters and lifecycle.

```
                    ┌─────────────┐
                    │  Preparing  │  ← uploading files, editing
                    └──────┬──────┘
                           │ submit()
                    ┌──────▼──────┐
                    │   Queued    │  ← all Tasks created
                    └──────┬──────┘
                           │ first task starts processing
                    ┌──────▼──────────┐
                    │   Processing    │◄──────────────────────┐
                    └──┬────┬────┬────┘                       │
                       │    │    │                            │
              ┌────────┘    │    └──────────┐                │
              ▼             ▼               ▼                 │
        ┌──────────┐ ┌──────────┐  ┌──────────────┐         │
        │ All Done │ │ Partial  │  │ All Failed   │──retry──┘
        │          │ │ Failure  │  │              │(retry all)
        └────┬─────┘ └────┬─────┘  └──────┬───────┘
             │            │               │
        ┌────▼─────┐ ┌────▼───────┐ ┌────▼───────┐
        │ Completed│ │ Completed  │ │   Failed   │
        │(all good)│ │(partial)   │ │(all bad)   │
        └──────────┘ └────────────┘ └────────────┘
```

```rust
enum BatchStatus {
    Preparing,   // uploading files, editing items
    Queued,      // submitted, Tasks created/enqueuing
    Processing,  // ≥1 Task in processing
    Paused,      // paused by user (cascades to Tasks)
    Completed,   // all Tasks terminal (partial or full)
    Failed,      // all Tasks Failed/Cancelled
    Cancelled,   // cancelled by user
}
```

| from → to | trigger |
|---|---|
| Preparing → Queued | submit() |
| Queued → Processing | first Task starts |
| Processing → Paused | user pauses |
| Paused → Processing | user resumes |
| Processing → Completed | `done+failed == total` |
| Processing → Failed | all tasks Failed/Cancelled, zero done |
| Preparing → Cancelled | user cancels |
| Queued → Cancelled | user cancels |
| Processing → Cancelled | user cancels |
| Paused → Cancelled | user cancels |
| Failed → Queued | user retry all |

---

### Cross-Layer Propagation Rules

#### Group → Task cascade

```
On Batch → Paused:
  1. batch.status = Paused
  2. Find all non-terminal child Tasks (Queued/Chunking/Processing) → each.transition_to(Paused)
  3. ChunkQueue: skip chunks where chunk.task.batch_id = this batch

On Batch → Processing (resume):
  1. batch.status = Processing
  2. Find all Paused child Tasks → each.transition_to(previous_state or Queued)
  3. ChunkQueue: resume dispatch for this batch

On Batch → Cancelled:
  1. batch.status = Cancelled
  2. All child Tasks (except Done/Failed) → Cancelled
  3. ChunkQueue: remove all Queued chunks for this batch
  4. In-flight Processing chunks: allow finish or ignore
```

#### Task → Chunk cascade

```
On Task → Paused:
  ChunkQueue: stop dispatching new chunks for this Task
  In-flight chunks continue processing (results are valid)

On Task → Cancelled:
  ChunkQueue: remove Queued chunks for this task
  In-flight chunks: results are discarded (merge never triggered)
```

#### Chunk → Task bubble (DATA-DRIVEN — the core event model)

```
When any Chunk status changes to Done/Failed/Dead:

  On Chunk → Done:
    1. task.done_chunks += 1
    2. Check: done_chunks + failed_chunks == total_chunks?
       ├─ YES, failed_chunks == 0 → task → Merging → Done
       ├─ YES, done_chunks > 0    → task → Merging → Done (partial success)
       └─ NO                      → wait for more chunks

  On Chunk → Failed:
    1. task.failed_chunks += 1
    2. retry_count < max_retries? → chunk → Queued (retry)
    3. retries exhausted → chunk → Dead
       + if done_chunks + failed_chunks == total_chunks && done_chunks == 0
         → task → Failed

  On Chunk → Dead: same as Failed exhaustion path

  ★ On Task reaching any final state (Done/Failed):
    → batch.done_tasks++ or batch.failed_tasks++
    → Check batch: done+failed == total → batch → Completed | Failed
```

#### Task → Batch bubble

```
When any Task reaches terminal state (Done/Failed/Cancelled):
  1. If Done: batch.done_tasks += 1
     If Failed/Cancelled: batch.failed_tasks += 1
  2. Check: done_tasks + failed_tasks == total_tasks?
     ├─ YES, failed_tasks == 0 → batch → Completed
     ├─ YES, done_tasks > 0    → batch → Completed (partial)
     └─ YES, done_tasks == 0   → batch → Failed
     └─ NO                     → continue waiting
```

---

### Crash Recovery Path

```
Server restart → SQLite init → Rebuild ChunkQueue → Recover:

1. Scan batches:
   - Preparing    → retain (user decides to continue/cancel)
   - Queued/Processing → needs recovery
   - Paused       → retain paused, user resumes manually
   - Done/Failed/Cancelled → skip

2. Scan tasks (per recovery-needed batch):
   - Pending      → re-enqueue to TaskQueue
   - Queued       → re-enqueue to TaskQueue
   - Chunking     → if chunks exist in DB → Processing; else restart Chunking
   - Processing   → scan its chunks for recovery
   - Merging      → check chunks: all Done? → re-merge; else → Processing
   - Paused       → keep paused
   - Done/Failed/Cancelled → skip

3. Scan chunks (per recovery-needed task):
   - Pending/Queued → keep (ChunkQueue re-picks them up)
   - Processing → Queued ⚠️ (distrust in-flight requests)
   - Done       → skip (check WAV cache file exists)
   - Failed     → retry_count < max? → retry; else keep Dead
   - Dead       → skip

4. Rebuild ChunkQueue in memory, start dispatch
```

⚠️ **Processing → Queued tradeoff:** May cause duplicate API calls if MIMO API process the request but response is lost. **Mitigation:** Use chunk UUIDv7 as idempotency key. If API supports idempotency, duplicate returns cached result.

---

### Frontend State Mapping

Frontend doesn't need internal states; simplified mapping:

```typescript
type TaskCardStatus = 'pending' | 'queued' | 'processing' | 'paused' | 'done' | 'failed' | 'cancelled';
type BatchPageStatus = 'preparing' | 'queued' | 'processing' | 'paused' | 'completed' | 'failed' | 'cancelled';

// Internal → Frontend mapping:
//   Chunking  → 'queued' (user doesn't need to know chunking mid-state)
//   Merging   → transient, either skip or flash 'done'
//   Dead      → 'failed' (internal concept, don't expose)

interface TaskProgress {
  total_chunks: number;
  done_chunks: number;
  failed_chunks: number;
  progress_pct: number;  // done/total * 100, 0 if no chunks yet
  // short text (1 chunk) — no progress bar needed, just show status badge
}
```

---

## Implementation Phases (Detailed)

---

### Phase 0: Foundation — UUIDv7 + Error + SQLite

> Files to create: `shared/id.rs`, `shared/error.rs`, `infra/persistence/db.rs`, `infra/persistence/migrate.rs`

#### Step 0.1: Ensure Cargo.toml has required dependencies

```toml
# Already present. Verify features:
uuid = { version = "1.6", features = ["v4", "v7", "serde", "fast-rng"] }
chrono = { version = "0.4", features = ["serde"] }
rusqlite = { version = "0.31", features = ["bundled"] }
```

**Test:** `cargo check` passes with no warnings.

#### Step 0.2: Create shared/id.rs

```rust
use uuid::Uuid;
use crate::shared::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Id(String);

impl Id {
    pub fn new() -> Self { Self(Uuid::now_v7().to_string()) }
    pub fn from_str(s: &str) -> Result<Self, AppError> {
        Uuid::parse_str(s).map_err(|_| AppError::InvalidInput(format!("Invalid ID: {}", s)))?;
        Ok(Self(s.to_string()))
    }
    pub fn as_str(&self) -> &str { &self.0 }
    pub fn to_string(&self) -> String { self.0.clone() }
}
impl Display for Id { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{}", self.0) } }
impl Default for Id { fn default() -> Self { Self::new() } }
```

**Tests:**
```rust
#[test]
fn test_id_generates_unique_values() {
    let ids: Vec<String> = (0..100).map(|_| Id::new().to_string()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "UUIDv7 must be time-sortable");
    let unique: HashSet<_> = ids.into_iter().collect();
    assert_eq!(unique.len(), 100, "must generate unique IDs");
}
#[test]
fn test_id_from_str_invalid() {
    assert!(Id::from_str("not-a-uuid").is_err());
}
#[test]
fn test_id_roundtrip() {
    let id = Id::new();
    let parsed = Id::from_str(id.as_str()).unwrap();
    assert_eq!(id, parsed);
}
```

#### Step 0.3: Create shared/error.rs

```rust
use actix_web::{HttpResponse, ResponseError};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Rate limited")]
    RateLimited,
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self { Self::Internal(e.to_string()) }
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    code: String,
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        }
    }
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(ErrorResponse {
            error: self.to_string(),
            code: format!("{:?}", self),
        })
    }
}
```

**Tests:**
```rust
#[test]
fn test_error_http_status_mapping() {
    assert_eq!(AppError::NotFound("x".into()).status_code(), StatusCode::NOT_FOUND);
    assert_eq!(AppError::InvalidInput("x".into()).status_code(), StatusCode::BAD_REQUEST);
    assert_eq!(AppError::Conflict("x".into()).status_code(), StatusCode::CONFLICT);
    assert_eq!(AppError::Internal("x".into()).status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(AppError::RateLimited.status_code(), StatusCode::TOO_MANY_REQUESTS);
}
#[test]
fn test_error_response_contains_fields() {
    let err = AppError::NotFound("task_123".into());
    let resp = err.error_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
```

#### Step 0.4: Create infra/persistence/db.rs

```rust
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
pub type DbPool = Pool<SqliteConnectionManager>;

pub fn create_pool(db_path: &str, max_size: u32) -> Result<DbPool, AppError> {
    let manager = SqliteConnectionManager::file(db_path);
    let pool = Pool::builder().max_size(max_size).build(manager)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    // Ensure WAL mode on init
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    Ok(pool)
}

#[cfg(test)]
pub fn create_test_pool() -> DbPool {
    let manager = SqliteConnectionManager::memory();
    let pool = Pool::builder().max_size(2).build(manager).unwrap();
    let conn = pool.get().unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").unwrap();
    pool
}
```

**Tests:**
```rust
#[test]
fn test_db_pool_init_and_query() {
    let pool = create_test_pool();
    let conn = pool.get().unwrap();
    let count: i64 = conn.query_row("SELECT 1", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1);
}
```

#### Step 0.5: Create infra/persistence/migrate.rs

`run_migrations(pool)` creates all tables using `SCHEMA_SQL`. Idempotent via `IF NOT EXISTS`.

**Tests:**
```rust
#[test]
fn test_migrations_create_all_tables() {
    let pool = create_test_pool();
    let conn = pool.get().unwrap();
    run_migrations(&conn).unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0)).unwrap()
        .filter_map(|r| r.ok())
        .collect();
    assert!(tables.contains(&"batches".to_string()), "batches table must exist");
    assert!(tables.contains(&"tasks".to_string()), "tasks table must exist");
    assert!(tables.contains(&"chunks".to_string()), "chunks table must exist");
    assert!(tables.contains(&"chunk_queue".to_string()), "chunk_queue table must exist");
    assert!(tables.contains(&"task_queue".to_string()), "task_queue table must exist");
    assert!(tables.contains(&"groups".to_string()), "groups table must exist");
}
#[test]
fn test_migrations_idempotent() {
    let pool = create_test_pool();
    let conn = pool.get().unwrap();
    run_migrations(&conn).unwrap();
    run_migrations(&conn).unwrap();  // second run must not fail
}
```

#### Step 0.6: Wire into main.rs

```rust
fn main() -> std::io::Result<()> {
    let config = AppConfig::load();
    let pool = create_pool(&config.database_path, 8)?;
    {
        let conn = pool.get().map_err(|e| ...)?;
        run_migrations(&conn).map_err(|e| ...)?;
    }
    // ... rest of setup
}
```

**Verification:** `cargo run` starts successfully, database file created with all tables.

**Commit:** `git commit -m "phase-0: add UUIDv7 IDs, AppError, SQLite pool and schema migrations"`

---

### Phase 1: Domain Models

> Files to create: `domain/task.rs`, `domain/chunk.rs`, `domain/batch.rs`, `domain/group.rs`, `domain/events.rs`, `domain/mod.rs`

#### Step 1.1: domain/task.rs

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,    // created, awaiting TaskQueue
    Queued,     // in TaskQueue, awaiting chunker
    Chunking,   // chunker running split_text_into_chunks
    Processing, // ≥1 chunk in ChunkQueue or being processed
    Merging,    // all chunks Done, merging final audio
    Paused,     // paused by user or parent group
    Done,       // task complete (audio merged)
    Failed,     // all chunks Failed/Dead, unrecoverable
    Cancelled,  // cancelled by user or parent group
}

impl TaskStatus {
    pub fn can_transition_to(&self, next: &Self) -> bool {
        matches!((self, next),
            // Normal forward flow
            (Self::Pending, Self::Queued)
            | (Self::Queued, Self::Chunking)
            | (Self::Chunking, Self::Processing)
            | (Self::Processing, Self::Merging)
            | (Self::Merging, Self::Done)
            // Partial failure / retry
            | (Self::Processing, Self::Failed)
            | (Self::Failed, Self::Queued)  // manual retry
            // Pause/resume
            | (Self::Queued, Self::Paused)
            | (Self::Chunking, Self::Paused)
            | (Self::Processing, Self::Paused)
            | (Self::Paused, Self::Queued)
            | (Self::Paused, Self::Chunking)
            | (Self::Paused, Self::Processing)
            // Cancel from any active state
            | (Self::Pending, Self::Cancelled)
            | (Self::Queued, Self::Cancelled)
            | (Self::Chunking, Self::Cancelled)
            | (Self::Processing, Self::Cancelled)
            | (Self::Merging, Self::Cancelled)
            | (Self::Paused, Self::Cancelled)
            | (Self::Failed, Self::Cancelled)
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Id,
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub batch_id: Option<Id>,
    pub group_id: Option<Id>,
    pub content: String,
    pub content_ref: Option<String>,
    pub title: String,
    pub voice: String,
    pub model: String,
    pub style: Option<String>,
    pub speed: f64,
    pub total_chars: i64,
    pub total_tokens: i64,
    pub total_chunks: i32,
    pub done_chunks: i32,
    pub failed_chunks: i32,
    pub output_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Task {
    pub fn new(req: CreateTaskRequest) -> Self {
        let now = Utc::now();
        Self {
            id: Id::new(),
            task_type: req.task_type,
            status: TaskStatus::Pending,
            batch_id: req.batch_id,
            group_id: None,
            content: req.content,
            content_ref: req.content_ref,
            title: req.title,
            voice: req.voice,
            model: req.model,
            style: req.style,
            speed: req.speed,
            total_chars: req.total_chars,
            total_tokens: req.total_tokens,
            total_chunks: 0, done_chunks: 0, failed_chunks: 0,
            output_path: None,
            created_at: now, updated_at: now, completed_at: None,
        }
    }
    pub fn transition_to(&mut self, status: TaskStatus) -> Result<(), AppError> {
        if !self.status.can_transition_to(&status) {
            return Err(AppError::InvalidInput(
                format!("Cannot transition from {:?} to {:?}", self.status, status)
            ));
        }
        self.status = status;
        self.updated_at = Utc::now();
        if matches!(status, TaskStatus::Done | TaskStatus::Cancelled) {
            self.completed_at = Some(Utc::now());
        }
        Ok(())
    }
}
```

**Tests:**
```rust
#[test]
fn test_task_status_valid_transitions() {
    let transitions = [
        (TaskStatus::Pending, TaskStatus::Queued),
        (TaskStatus::Queued, TaskStatus::Chunking),
        (TaskStatus::Chunking, TaskStatus::Processing),
        (TaskStatus::Processing, TaskStatus::Merging),
        (TaskStatus::Merging, TaskStatus::Done),
        (TaskStatus::Processing, TaskStatus::Failed),
        (TaskStatus::Failed, TaskStatus::Queued),
        (TaskStatus::Processing, TaskStatus::Paused),
        (TaskStatus::Paused, TaskStatus::Processing),
        (TaskStatus::Pending, TaskStatus::Cancelled),
        (TaskStatus::Processing, TaskStatus::Cancelled),
    ];
    for (from, to) in &transitions {
        assert!(from.can_transition_to(to), "{:?} -> {:?} should be valid", from, to);
    }
}
#[test]
fn test_task_status_invalid_transitions() {
    assert!(!TaskStatus::Pending.can_transition_to(&TaskStatus::Done));
    assert!(!TaskStatus::Done.can_transition_to(&TaskStatus::Processing));
    assert!(!TaskStatus::Pending.can_transition_to(&TaskStatus::Failed));
    assert!(!TaskStatus::Done.can_transition_to(&TaskStatus::Cancelled)); // terminal
    assert!(!TaskStatus::Cancelled.can_transition_to(&TaskStatus::Queued)); // terminal
}
#[test]
fn test_task_new_sets_defaults() {
    let req = CreateTaskRequest { content: "hello".into(), ... };
    let task = Task::new(req);
    assert_eq!(task.status, TaskStatus::Pending);
    assert_eq!(task.total_chunks, 0);
    assert!(task.completed_at.is_none());
}
#[test]
fn test_task_transition_sets_completed_at() {
    let mut task = create_test_task();
    task.transition_to(TaskStatus::Processing).unwrap();
    task.transition_to(TaskStatus::Done).unwrap();
    assert!(task.completed_at.is_some());
}
#[test]
fn test_task_cancelled_also_sets_completed_at() {
    let mut task = create_test_task();
    task.transition_to(TaskStatus::Cancelled).unwrap();
    assert!(task.completed_at.is_some());
}
```

#### Step 1.2: domain/chunk.rs

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChunkStatus {
    Pending,    // created by chunker, not queued yet
    Queued,     // in chunk_queue, awaiting dispatcher
    Processing, // dispatcher dequeued, MIMO API in-flight
    Done,       // API success, WAV cached
    Failed,     // API error / timeout (retryable)
    Dead,       // retry_count >= max_retries, abandoned
}

impl ChunkStatus {
    pub fn can_transition_to(&self, next: &Self) -> bool {
        matches!((self, next),
            (Self::Pending, Self::Queued)
            | (Self::Queued, Self::Processing)
            | (Self::Processing, Self::Done)
            | (Self::Processing, Self::Failed)
            | (Self::Failed, Self::Queued)  // retry
            | (Self::Failed, Self::Dead)     // exhausted
            | (Self::Processing, Self::Queued)  // crash recovery only
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: Id,
    pub task_id: Id,
    pub seq: i32,             // order within task (1-based)
    pub content: String,      // actual text to synthesize
    pub status: ChunkStatus,
    pub retry_count: i32,
    pub max_retries: i32,
    pub output_path: Option<String>,  // relative `cache/wav/{task_id}/{seq}.wav`
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Chunk {
    pub fn new(task_id: Id, seq: i32, content: String) -> Self {
        Self {
            id: Id::new(),
            task_id,
            seq,
            content,
            status: ChunkStatus::Pending,
            retry_count: 0,
            max_retries: 3,
            output_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        }
    }

    pub fn transition_to(&mut self, status: ChunkStatus) -> Result<(), AppError> {
        if !self.status.can_transition_to(&status) {
            return Err(AppError::InvalidInput(format!(
                "Chunk {}: invalid transition {:?} -> {:?}", self.id, self.status, status
            )));
        }
        if status == ChunkStatus::Failed {
            self.retry_count += 1;
        }
        self.status = status;
        self.updated_at = Utc::now();
        if matches!(status, ChunkStatus::Done | ChunkStatus::Dead) {
            self.completed_at = Some(Utc::now());
        }
        Ok(())
    }
}
```

**Tests:**
```rust
#[test]
fn test_chunk_status_valid_transitions() {
    let cases = [
        (ChunkStatus::Pending, ChunkStatus::Queued),
        (ChunkStatus::Queued, ChunkStatus::Processing),
        (ChunkStatus::Processing, ChunkStatus::Done),
        (ChunkStatus::Processing, ChunkStatus::Failed),
        (ChunkStatus::Failed, ChunkStatus::Queued),
        (ChunkStatus::Failed, ChunkStatus::Dead),
    ];
    for (from, to) in &cases {
        assert!(from.can_transition_to(to), "{:?} -> {:?} should be valid", from, to);
    }
}
#[test]
fn test_chunk_status_invalid() {
    assert!(!ChunkStatus::Dead.can_transition_to(&ChunkStatus::Queued)); // terminal
    assert!(!ChunkStatus::Pending.can_transition_to(&ChunkStatus::Done)); // skip
}
#[test]
fn test_chunk_failed_increments_retry() {
    let mut chunk = Chunk::new(Id::new(), 1, "hello".into());
    chunk.transition_to(ChunkStatus::Failed).unwrap();
    assert_eq!(chunk.retry_count, 1);
}
#[test]
fn test_chunk_done_sets_completed_at() {
    let mut chunk = Chunk::new(Id::new(), 1, "hello".into());
    chunk.transition_to(ChunkStatus::Queued).unwrap();
    chunk.transition_to(ChunkStatus::Processing).unwrap();
    chunk.transition_to(ChunkStatus::Done).unwrap();
    assert!(chunk.completed_at.is_some());
}
```

#### Step 1.3: domain/batch.rs

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BatchStatus {
    Preparing,   // uploading files, editing items
    Queued,      // submitted, Tasks created
    Processing,  // ≥1 Task in processing
    Paused,      // paused by user (cascades to child Tasks)
    Completed,   // all child Tasks terminal (partial or full)
    Failed,      // all child Tasks Failed/Cancelled
    Cancelled,   // cancelled by user
}

impl BatchStatus {
    pub fn can_transition_to(&self, next: &Self) -> bool {
        matches!((self, next),
            (Self::Preparing, Self::Queued)
            | (Self::Preparing, Self::Cancelled)
            | (Self::Queued, Self::Processing)
            | (Self::Queued, Self::Cancelled)
            | (Self::Processing, Self::Completed)
            | (Self::Processing, Self::Failed)
            | (Self::Processing, Self::Paused)
            | (Self::Processing, Self::Cancelled)
            | (Self::Paused, Self::Processing)
            | (Self::Paused, Self::Cancelled)
            | (Self::Failed, Self::Queued)  // retry all
        )
    }
}
```

`BatchPendingItem` struct with:
- `effective_voice = custom_voice.clone().unwrap_or(batch.voice.clone())`
- `effective_title = custom_title.clone().unwrap_or(filename.clone())`
- etc. — computed in constructor

**Tests:**
```rust
#[test]
fn test_pending_item_effective_inherits_batch() {
    let batch = create_test_batch();
    let item = BatchPendingItem::new_for_test(&batch, "file.txt", "content", None);
    assert_eq!(item.effective_voice, batch.voice);
    assert_eq!(item.effective_title, "file.txt");
}
#[test]
fn test_pending_item_effective_with_custom() {
    let batch = create_test_batch();
    let mut overrides = ItemOverride::default();
    overrides.voice = Some("custom-voice".into());
    overrides.title = Some("My Title".into());
    let item = BatchPendingItem::with_overrides(&batch, "file.txt", "content", overrides);
    assert_eq!(item.effective_voice, "custom-voice");
    assert_eq!(item.effective_title, "My Title");
}
```

#### Step 1.4: domain/events.rs

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DomainEvent {
    FileParsed { batch_id: Id, filename: String, seq: i32, chars: i64, tokens: i64 },
    ParsingComplete { batch_id: Id, total: usize, parsed: usize, failed: usize },
    TaskEnqueued { task_id: Id, batch_id: Option<Id> },
    ChunkCompleted { chunk_id: Id, task_id: Id, seq: i32, audio_path: String, duration: f64 },
    ChunkFailed { chunk_id: Id, task_id: Id, seq: i32, error: String, retry_count: i32 },
    AllChunksDone { task_id: Id, total_chunks: i32 },
    TaskCompleted { task_id: Id, batch_id: Option<Id>, output_path: String, duration: f64 },
    TaskFailed { task_id: Id, error: String },
    BatchCompleted { batch_id: Id },
    BatchFailed { batch_id: Id, error: String, failed_count: i32 },
    GroupCompleted { group_id: Id, batch_id: Id },
    GroupFailed { group_id: Id, batch_id: Id, error: String },
}
```

**Test:** `test_event_serialization_roundtrip` — create each variant, serialize to JSON, deserialize, assert equal.

#### Step 1.5: domain/group.rs

Status machine + `Group` struct. Same pattern.

**Commit:** `git commit -m "phase-1: add domain models with status machines and event types"`

---

### Phase 2: Repositories

> Files: `infra/persistence/task_repo.rs`, `chunk_repo.rs`, `batch_repo.rs`, `group_repo.rs`

Each repo: define trait + `Sqlite*Repo` impl using r2d2 pool.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchProgressAggregate {
    pub batch_id: String,
    pub total_tasks: i32,
    pub done_tasks: i32,
    pub failed_tasks: i32,
    pub processing_tasks: i32,
}
```

#### Step 2.1: infra/persistence/task_repo.rs

```rust
pub trait TaskRepo: Send + Sync {
    fn insert(&self, task: &Task) -> Result<(), AppError>;
    fn find_by_id(&self, id: &str) -> Result<Option<Task>, AppError>;
    fn update_status(&self, id: &str, status: &TaskStatus) -> Result<(), AppError>;
    fn update_chunk_progress(&self, id: &str, total: i32, done: i32, failed: i32) -> Result<(), AppError>;
    fn set_output(&self, id: &str, path: &str, duration: f64) -> Result<(), AppError>;
    fn find_by_batch(&self, batch_id: &str) -> Result<Vec<Task>, AppError>;
    fn batch_progress(&self, batch_id: &str) -> Result<BatchProgressAggregate, AppError>;
}

pub struct SqliteTaskRepo { pool: DbPool }

impl TaskRepo for SqliteTaskRepo {
    fn insert(&self, task: &Task) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO tasks (id, task_type, status, group_id, batch_id, content, content_ref,
             title, voice, model, style, speed, priority, total_chars, total_tokens,
             total_chunks, done_chunks, failed_chunks, output_path, output_duration,
             created_at, updated_at, completed_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23)",
            params![task.id, task.task_type, task.status, ...]
        )?;
        Ok(())
    }
    // ... other methods
}
```

**Integration tests (all use `create_test_pool()` + `run_migrations()`):**
```rust
#[test]
fn test_task_insert_and_find() {
    let pool = create_test_pool();
    run_migrations(&pool.get().unwrap()).unwrap();
    let repo = SqliteTaskRepo::new(pool);
    let task = create_test_task();
    repo.insert(&task).unwrap();
    let found = repo.find_by_id(task.id.as_str()).unwrap().unwrap();
    assert_eq!(found.id, task.id);
    assert_eq!(found.content, task.content);
    assert_eq!(found.status, TaskStatus::Pending);
}
#[test]
fn test_task_update_status() {
    let pool = create_test_pool();
    run_migrations(&pool.get().unwrap()).unwrap();
    let repo = SqliteTaskRepo::new(pool);
    let task = create_test_task();
    repo.insert(&task).unwrap();
    repo.update_status(task.id.as_str(), &TaskStatus::Queued).unwrap();
    let found = repo.find_by_id(task.id.as_str()).unwrap().unwrap();
    assert_eq!(found.status, TaskStatus::Queued);
}
#[test]
fn test_task_find_by_batch() {
    let pool = create_test_pool();
    run_migrations(&pool.get().unwrap()).unwrap();
    let repo = SqliteTaskRepo::new(pool);
    let batch_id = Id::new();
    for i in 0..3 {
        let mut task = create_test_task();
        task.batch_id = Some(batch_id.clone());
        repo.insert(&task).unwrap();
    }
    let tasks = repo.find_by_batch(batch_id.as_str()).unwrap();
    assert_eq!(tasks.len(), 3);
}
```

#### Step 2.2: infra/persistence/chunk_repo.rs

```rust
pub trait ChunkRepo: Send + Sync {
    fn insert(&self, chunk: &Chunk) -> Result<(), AppError>;
    fn insert_batch(&self, chunks: &[Chunk]) -> Result<(), AppError>; // single transaction
    fn find_by_id(&self, id: &str) -> Result<Option<Chunk>, AppError>;
    fn find_by_task(&self, task_id: &str) -> Result<Vec<Chunk>, AppError>;
    fn find_pending(&self, limit: i64) -> Result<Vec<Chunk>, AppError>;
    /// Find pending chunks ordered by priority DESC, created_at ASC.
    /// This is the main query used by ChunkQueue::run for priority scheduling.
    fn find_pending_prioritized(&self, limit: i64) -> Result<Vec<Chunk>, AppError>;
    /// Find the single oldest pending chunk regardless of priority (for starvation guard).
    fn find_oldest_pending(&self) -> Result<Option<Chunk>, AppError>;
    fn update_status(&self, id: &str, status: &ChunkStatus) -> Result<(), AppError>;
    fn update_priority(&self, id: &str, priority: i64) -> Result<(), AppError>;
    fn mark_done(&self, id: &str, audio_path: &str, duration: f64) -> Result<(), AppError>;
    fn mark_failed(&self, id: &str, error: &str) -> Result<(), AppError>;
    fn count_by_task_status(&self, task_id: &str, status: &ChunkStatus) -> Result<i64, AppError>;
    fn count_by_task_all(&self, task_id: &str) -> Result<i64, AppError>;
    fn reset_processing_to_pending(&self) -> Result<usize, AppError>;
}
```

**Tests:**
```rust
#[test]
fn test_chunk_insert_batch_and_find() {
    // Insert 5 chunks for same task, find_by_task returns 5
}
#[test]
fn test_chunk_find_pending_prioritized_order() {
    // Insert 3 chunks: priority=1(fast), priority=0(old), priority=-1(bulk)
    // find_pending_prioritized returns sorted: [high, normal, bulk]
    // Within same priority, verify created_at ASC order
}
#[test]
fn test_chunk_find_oldest_pending() {
    // Insert 3 pending chunks with staggered created_at
    // find_oldest_pending returns the oldest
}
#[test]
fn test_chunk_update_priority() {
    // Set priority=1, verify returned by find_pending_prioritized first
}
#[test]
fn test_chunk_mark_done() {
    // Verify audio_path + duration set
}
#[test]
fn test_chunk_reset_processing() {
    // Set 3 chunks to processing, reset → all pending, count = 3
}
```

#### Step 2.3: infra/persistence/batch_repo.rs

```rust
pub trait BatchRepo: Send + Sync {
    // Batch CRUD
    fn insert_batch(&self, batch: &Batch) -> Result<(), AppError>;
    fn find_batch(&self, id: &str) -> Result<Option<Batch>, AppError>;
    fn update_batch_status(&self, id: &str, status: &BatchStatus) -> Result<(), AppError>;
    fn delete_batch(&self, id: &str) -> Result<(), AppError>;

    // Pending items
    fn insert_pending_item(&self, item: &BatchPendingItem) -> Result<(), AppError>;
    fn list_pending_items(&self, batch_id: &str, page: i64, per_page: i64) -> Result<PaginatedItems, AppError>;
    fn find_pending_item_by_seq(&self, batch_id: &str, seq: i32) -> Result<Option<BatchPendingItem>, AppError>;
    fn update_pending_item(&self, batch_id: &str, seq: i32, overrides: ItemOverride) -> Result<BatchPendingItem, AppError>;
    fn batch_update_pending_items(&self, batch_id: &str, seqs: Option<&[i32]>, overrides: ItemOverride) -> Result<usize, AppError>;
    fn delete_pending_item(&self, batch_id: &str, seq: i32) -> Result<(), AppError>;
    fn count_pending_items(&self, batch_id: &str) -> Result<i64, AppError>;

    // Submit — the big one
    fn submit_batch(&self, batch_id: &str, defaults: &BatchDefaults) -> Result<Vec<Task>, AppError>;

    // Batch-task association
    fn get_child_task_ids(&self, batch_id: &str) -> Result<Vec<String>, AppError>;
}
```

**`submit_batch` implementation (optimistic retry):**

> **Design:** The entire submit operation (read items → create tasks → clear pending → update batch status) must be atomic. If any step fails, nothing should be created. Use `BEGIN IMMEDIATE` (not `DEFERRED`) to acquire write lock upfront, and wrap in an optimistic retry loop to handle `SQLITE_BUSY` from concurrent writers.

```rust
const MAX_SUBMIT_RETRIES: u32 = 3;
const RETRY_BASE_MS: u64 = 10;

fn submit_batch(&self, batch_id: &str, defaults: &BatchDefaults) -> Result<Vec<Task>, AppError> {
    let mut last_error = None;
    for attempt in 0..MAX_SUBMIT_RETRIES {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(e) => { last_error = Some(e); continue; }
        };
        let batch = match self.find_batch(batch_id) {
            Ok(Some(b)) => b,
            Ok(None) => return Err(AppError::NotFound(...)),
            Err(e) => { last_error = Some(e); continue; }
        };

        // BEGIN IMMEDIATE acquires write lock upfront (avoids deadlock)
        if let Err(e) = conn.execute("BEGIN IMMEDIATE", []) {
            last_error = Some(e);
            continue;
        }

        // Read all pending items inside transaction
        let items = match self.list_pending_items_in_tx(&conn, batch_id) {
            Ok(items) => items,
            Err(e) => {
                let _ = conn.execute("ROLLBACK", []);
                last_error = Some(e);
                continue;
            }
        };

        let mut tasks = Vec::with_capacity(items.len());
        let mut all_ok = true;
        for item in &items {
            let task = Task::new(CreateTaskRequest {
                task_type: TaskType::BatchChild,
                batch_id: Some(Id::from_str(batch_id)?),
                content: item.content.clone(),
                title: item.effective_title.clone(),
                voice: item.effective_voice.clone(),
                model: item.effective_model.clone(),
                style: item.effective_style.clone(),
                speed: item.effective_speed,
                total_chars: item.total_chars,
                total_tokens: item.token_estimate,
                content_ref: Some(item.filename.clone()),
            });

            if conn.execute("INSERT INTO tasks (...) VALUES (...)", params![...]).is_err() {
                all_ok = false;
                break;
            }
            if conn.execute("INSERT INTO batch_tasks (...) VALUES (...)", params![...]).is_err() {
                all_ok = false;
                break;
            }
            tasks.push(task);
        }

        if !all_ok {
            let _ = conn.execute("ROLLBACK", []);
            last_error = Some(AppError::Internal("task creation failed".into()));
            continue;
        }

        // Clear pending items
        if conn.execute(
            "DELETE FROM batch_pending_items WHERE batch_id = ?1 AND status = 'pending'",
            params![batch_id],
        ).is_err() {
            let _ = conn.execute("ROLLBACK", []);
            continue;
        }

        // Update batch status
        if conn.execute(
            "UPDATE batches SET status = 'queued', total_items = ?1, total_chars = ?2,
             total_tokens = ?3, updated_at = datetime('now') WHERE id = ?4",
            params![tasks.len() as i64, ...],
        ).is_err() {
            let _ = conn.execute("ROLLBACK", []);
            continue;
        }

        // Success — commit and return
        conn.execute("COMMIT", [])?;
        return Ok(tasks);
    }

    Err(last_error.unwrap_or(AppError::Internal("submit failed after retries".into())))
}
```

> **Optimistic retry details:**
> - `SQLITE_BUSY` / `SQLITE_PROTOCOL` triggers retry with exponential backoff: `min(1000, RETRY_BASE_MS * 2^attempt)` ms sleep before reconnecting
> - `BEGIN IMMEDIATE` avoids the `SQLITE_SCHEMA` deadlock scenario where two deferred transactions try to upgrade to write simultaneously
> - Each attempt uses a fresh connection from pool (avoids stale transaction state)
> - 3 retries × ~500ms max = ~1.5s worst-case, acceptable for a user-triggered submit

**Tests:**
```rust
#[test]
fn test_batch_submit_creates_tasks_and_clears_items() {
    // Insert batch + 5 items → submit → 5 tasks created, pending_items deleted
}
#[test]
fn test_batch_submit_optimistic_retry_on_busy() {
    // Mock SQLITE_BUSY on first attempt → retry succeeds on second
}
#[test]
fn test_batch_submit_rollback_on_failure() {
    // Force task insertion to fail mid-way → verify no partial state in DB
}
#[test]
fn test_batch_submit_idempotent_double_submit() {
    // Submit twice: first succeeds, second returns error (no pending items)
}
#[test]
fn test_batch_pending_item_pagination() {
    // Insert 60 items → page 1 = 50, page 2 = 10
}
#[test]
fn test_batch_update_single_item() {
    // PATCH { voice: "new-voice" } → effective_voice updated
}
#[test]
fn test_batch_progress_aggregation() {
    // Insert batch + 3 tasks with different statuses → aggregate returns correct counts
}
```

#### Step 2.4: infra/persistence/group_repo.rs

CRUD operations.

**Commit:** `git commit -m "phase-2: add SQLite repositories with traits and integration tests"`

---

### Phase 3: Core Pipeline

> Files: `infra/mimo/chunker.rs`, `infra/mimo/client.rs`, `infra/audio/merger.rs`, `infra/cache.rs`

#### Step 3.1: infra/mimo/chunker.rs

> **Design decision:** Smart chunking delegates to MIMO TTS API's own tokenizer via `POST /v1/tokenize`, which returns per-sentence token counts. This gives accurate counts that match actual API behavior and avoids maintaining a parallel heuristic. Fallback: if API is unreachable, use character-length heuristic (Chinese=1.3, ASCII=0.4) as a safety net.

```rust
pub struct ChunkSegment { pub text: String, pub char_count: i64, pub token_count: i64 }

pub struct MimoChunker {
    client: reqwest::Client,
    base_url: String,
    /// Target tokens per chunk (2K-3K recommended)
    pub target_tokens: i64,
    /// Max tokens per chunk (hard cap, single sentence can exceed)
    pub hard_cap_tokens: i64,
}

impl MimoChunker {
    /// Tokenize via MIMO API, returns sentence-level breakdown
    pub async fn tokenize(&self, text: &str) -> Result<Vec<SentenceInfo>, AppError> {
        let resp = self.client
            .post(format!("{}/v1/tokenize", self.base_url))
            .json(&serde_json::json!({
                "text": text,
                "model": "tts-1",  // TTS-specific tokenizer model
            }))
            .send()
            .await?;
        let body: TokenizeResponse = resp.json().await?;
        Ok(body.sentences)
    }

    /// Split text into chunks using SDK tokenizer with smart paragraph/context grouping
    ///
    /// 1. Call tokenize() to get sentence-level token breakdown
    /// 2. Accumulate sentences until target_tokens is reached (prefer sentence boundaries)
    /// 3. Group by paragraph (double newline) when possible — keep related content together
    /// 4. Inject style/context prefix into each chunk's metadata for per-chunk voice preservation
    /// 5. Single sentence exceeding hard_cap_tokens → force split at hard_cap
    /// 6. If tokenize() fails → fallback to character-length heuristic
    pub async fn split(&self, text: &str, context_hint: Option<&str>) -> Result<Vec<ChunkSegment>, AppError> {
        // Normal implementation with SDK tokenizer
        ...
    }

    /// Fallback heuristic when API is unreachable
    fn split_heuristic(&self, text: &str, context_hint: Option<&str>) -> Vec<ChunkSegment> {
        // Same sentence-boundary logic as original split_text
        // Token estimate: Chinese chars*1.3 + ASCII*0.4
        // Prepend context_hint style prefix to first chunk where meaningful
        ...
    }
}
```

**Tests:**
```rust
#[test]
fn test_chunker_sdk_tokenize_returns_sentences() {
    // Uses wiremock to mock /v1/tokenize
    // Returns 3 sentences with correct token counts
}
#[test]
fn test_chunker_split_normal_text() {
    // 5 sentences, 200 tokens total, target=100 → 2 chunks
    // Verifies sentence-boundary split
}
#[test]
fn test_chunker_split_with_context_hint() {
    // Text with context_hint="激昂"
    // First chunk should include style/context metadata marker
}
#[test]
fn test_chunker_split_single_huge_sentence() {
    let chunker = MimoChunker::new_test(100);
    let result = chunker.split("a".repeat(2000), None).await;
    // Forced split at hard_cap
    assert!(result.len() >= 2);
}
#[test]
fn test_chunker_split_empty() {
    let chunker = MimoChunker::new_test(100);
    let result = chunker.split("", None).await;
    assert_eq!(result.len(), 0);
}
#[test]
fn test_chunker_fallback_heuristic() {
    // Mock tokenize to return 500 error → fallback to heuristic
    // Output should still produce valid ChunkSegments
}
#[test]
fn test_chunker_estimate_tokens_chinese() {
    // 4 Chinese chars → 4*1.3 = 5.2 ≈ 5 tokens (fallback heuristic)
}
#[test]
fn test_chunker_estimate_tokens_english() {
    // "hello world" → 11 ASCII * 0.4 = 4.4 ≈ 4 tokens (fallback heuristic)
}
```

#### Step 3.2: infra/mimo/client.rs

```rust
pub struct MimoClient {
    http_client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl MimoClient {
    pub fn new(config: &AppConfig) -> Self { ... }

    pub async fn synthesize(&self, text: &str, voice: &str, model: &str, speed: f64)
        -> Result<Vec<u8>, AppError>
    {
        let url = format!("{}/v1/audio/speech", self.base_url);
        let resp = self.http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&json!({
                "model": model, "input": text, "voice": voice,
                "response_format": "wav", "speed": speed,
            }))
            .send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return match status {
                StatusCode::TOO_MANY_REQUESTS => Err(AppError::RateLimited),
                _ => Err(AppError::Internal(format!("TTS API error {}: {}", status, body))),
            };
        }
        Ok(resp.bytes().await?.to_vec())
    }
}
```

**Tests:** Uses `wiremock` to simulate API server. Test timeout, rate limit, success, and server error responses.

#### Step 3.3: infra/audio/merger.rs

```rust
pub fn merge_wavs(chunk_paths: &[PathBuf], output_path: &Path) -> Result<(PathBuf, f64), AppError> {
    if chunk_paths.is_empty() {
        return Err(AppError::InvalidInput("No chunks to merge".into()));
    }
    if chunk_paths.len() == 1 {
        // Single chunk: copy directly
        std::fs::copy(&chunk_paths[0], output_path)?;
        let duration = get_wav_duration(&chunk_paths[0])?;
        return Ok((output_path.to_path_buf(), duration));
    }

    // Read header from first file
    let header = read_wav_header(&chunk_paths[0])?;
    let total_data_size: u32 = chunk_paths.iter()
        .map(|p| get_wav_data_size(p).unwrap_or(0))
        .sum();

    let mut output = Vec::new();
    output.extend_from_slice(&header);  // Write header from first file

    for path in chunk_paths {
        let data = get_wav_data(path)?;
        output.extend_from_slice(&data);
    }

    // Update data size in header
    let data_size_bytes = total_data_size.to_le_bytes();
    output[4..8].copy_from_slice(&data_size_bytes);  // RIFF chunk size
    output[40..44].copy_from_slice(&data_size_bytes); // data sub-chunk size

    std::fs::write(output_path, &output)?;
    let duration = total_data_size as f64 / (header.sample_rate * header.channels * header.bits_per_sample / 8) as f64;
    Ok((output_path.to_path_buf(), duration))
}
```

**Tests:** Create synthetic 1-second WAV files (sine wave), merge, verify output duration = input count * 1s.

#### Step 3.4: infra/cache.rs

> **Design decision:** Two-level eviction policy combining LRU (memory pressure) + TTL (time-based). This prevents unbounded memory growth while ensuring stale entries don't accumulate. A background cleaner runs periodically to prune expired entries from both memory and disk.

```rust
use std::collections::LinkedList;

pub struct Cache {
    // Memory: bounded by max_entries with LRU eviction
    memory: RwLock<HashMap<String, Entry>>,
    access_order: Mutex<LinkedList<String>>,  // LRU tracking (front=most recent)
    disk_root: PathBuf,
    default_ttl: Duration,
    max_memory_entries: usize,
}

struct Entry {
    data: Vec<u8>,
    expires_at: Instant,
    disk_path: Option<PathBuf>,
    size: usize,          // For memory accounting
    created_at: Instant,  // For TTL verification
}

impl Cache {
    pub fn new(disk_root: PathBuf, ttl_hours: i64, max_memory_entries: usize) -> Self { ... }

    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        // LRU: touch before returning
        let mut guard = self.access_order.lock();
        // Move key to front
        ...

        // Check memory first (fast path)
        if let Some(entry) = self.memory.read().get(key) {
            if entry.expires_at > Instant::now() {
                return Some(entry.data.clone());
            }
        }
        // Check disk
        let disk_path = self.disk_root.join(key);
        if disk_path.exists() {
            let data = std::fs::read(&disk_path).ok()?;
            // LRU evict if at capacity
            self.enforce_memory_limit();
            self.memory.write().insert(key.to_string(), Entry {
                data: data.clone(),
                expires_at: Instant::now() + self.default_ttl,
                disk_path: Some(disk_path),
                size: data.len(),
                created_at: Instant::now(),
            });
            return Some(data);
        }
        None
    }

    pub fn put(&self, key: &str, data: Vec<u8>) -> Result<(), AppError> {
        let disk_path = self.disk_root.join(key);
        std::fs::create_dir_all(disk_path.parent().unwrap())?;
        std::fs::write(&disk_path, &data)?;
        // LRU evict if at capacity
        self.enforce_memory_limit();
        self.memory.write().insert(key.to_string(), Entry {
            data,
            expires_at: Instant::now() + self.default_ttl,
            disk_path: Some(disk_path),
            size: data.len(),
            created_at: Instant::now(),
        });
        self.access_order.lock().push_front(key.to_string());
        Ok(())
    }

    pub fn evict(&self, key: &str) {
        self.memory.write().remove(key);
        let disk_path = self.disk_root.join(key);
        let _ = std::fs::remove_file(&disk_path);
    }

    pub fn exists_on_disk(&self, key: &str) -> bool {
        self.disk_root.join(key).exists()
    }

    /// LRU eviction: remove least recently used entries until under limit
    fn enforce_memory_limit(&self) {
        let mut guard = self.access_order.lock();
        while guard.len() > self.max_memory_entries {
            if let Some(lru_key) = guard.pop_back() {
                self.memory.write().remove(&lru_key);
            } else {
                break;
            }
        }
    }

    /// Background cleaner — run in tokio::spawn at startup
    pub async fn cleaner_loop(&self, interval: Duration) {
        loop {
            tokio::time::sleep(interval).await;
            let expired: Vec<String> = {
                let mem = self.memory.read();
                mem.iter()
                    .filter(|(_, e)| e.expires_at <= Instant::now())
                    .map(|(k, _)| k.clone())
                    .collect()
            };
            for key in &expired {
                self.evict(key);
            }
        }
    }
}
```

**Tests:**
- `test_cache_put_get` / `test_cache_miss` / `test_cache_evict` / `test_cache_expiry`
- `test_cache_disk_survives_restart` — put data, create new Cache with same disk_root, get returns data
- `test_cache_exists_check` — put then check exists_on_disk
- `test_cache_lru_eviction` — put N+1 entries where N=max_memory_entries, verify least recently used is evicted
- `test_cache_lru_get_refreshes_order` — access old key, verify it moves to front, different key evicted next
- `test_cache_background_cleaner` — start cleaner_loop, put expired entry, wait interval+epsilon, verify evicted

**Commit:** `git commit -m "phase-3: add chunker, MIMO client, audio merger, two-level cache"`

---

### Phase 4: Queue System

> Files: `infra/queue/rate_limiter.rs`, `chunk_queue.rs`, `task_queue.rs`

#### Step 4.1: infra/queue/rate_limiter.rs

```rust
pub struct TokenBucket {
    tokens: AtomicU64,
    capacity: u64,
    refill_per_sec: f64,
    last_refill: AtomicI64,
}

impl TokenBucket {
    pub fn new(rpm: u64) -> Self {
        Self {
            tokens: AtomicU64::new(rpm),
            capacity: rpm,
            refill_per_sec: rpm as f64 / 60.0,
            last_refill: AtomicI64::new(chrono::Utc::now().timestamp_millis()),
        }
    }
    pub fn try_acquire(&self) -> bool {
        self.refill();
        loop {
            let current = self.tokens.load(Ordering::Relaxed);
            if current == 0 { return false; }
            if self.tokens.compare_exchange(current, current - 1, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
                return true;
            }
        }
    }
    pub async fn acquire(&self) {
        while !self.try_acquire() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    fn refill(&self) {
        let now = chrono::Utc::now().timestamp_millis();
        let last = self.last_refill.load(Ordering::Relaxed);
        if now - last < 100 { return; } // Don't refill too often
        let elapsed_ms = now - last;
        let new_tokens = (elapsed_ms as f64 * self.refill_per_sec / 1000.0).round() as u64;
        if new_tokens > 0 && self.last_refill.compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
            self.tokens.fetch_add(new_tokens, Ordering::Relaxed);
            self.tokens.fetch_min(self.capacity, Ordering::Relaxed); // cap
        }
    }
}
```

**Tests:**
```rust
#[test]
fn test_rate_limiter_initial_capacity() {
    let limiter = TokenBucket::new(10);
    for _ in 0..10 { assert!(limiter.try_acquire()); }
    assert!(!limiter.try_acquire(), "11th acquire should be blocked");
}
#[test]
fn test_rate_limiter_refill() {
    let limiter = TokenBucket::new(60); // 1 per second
    for _ in 0..60 { limiter.try_acquire(); }
    assert!(!limiter.try_acquire());
    std::thread::sleep(Duration::from_secs(1));
    assert!(limiter.try_acquire(), "should have 1 token after 1s");
}
```

#### Step 4.2: infra/queue/chunk_queue.rs

```rust
pub struct ChunkQueue {
    pool: DbPool,
    chunk_repo: Arc<dyn ChunkRepo>,
    client: Arc<MimoClient>,
    cache: Arc<Cache>,
    rate_limiter: Arc<TokenBucket>,
    event_tx: broadcast::Sender<DomainEvent>,
    notify: Arc<Notify>,
    cancellation_token: CancellationToken,
    semaphore: Arc<Semaphore>,
    max_concurrent: usize,
    max_task_wait: Duration,  // Starvation guard threshold (default 300s)
}

// Priority constants
const PRIORITY_NORMAL: i64 = 0;
const PRIORITY_HIGH: i64 = 1;   // interactive / batch children
const PRIORITY_BULK: i64 = -1;  // non-interactive bulk
const STARVATION_GUARD_INTERVAL: u64 = 100;

impl ChunkQueue {
    pub fn new(
        max_concurrent: usize,
        max_task_wait: Duration,
        ...
    ) -> Self {
        Self {
            max_concurrent,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            notify: Arc::new(Notify::new()),
            cancellation_token: CancellationToken::new(),
            ...
        }
    }

    /// Enqueue a chunk for processing. Called by TaskQueue.
    /// Wakes one waiting worker (via Notify) so it picks up the new chunk.
    pub async fn enqueue(&self, chunk_id: &str, task_id: &str) -> Result<(), AppError> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO chunk_queue (id, chunk_id, task_id, status, created_at)
             VALUES (?1, ?2, ?3, 'pending', datetime('now'))",
            params![Id::new().as_str(), chunk_id, task_id],
        )?;
        self.notify.notify_one();  // Wake one worker
        Ok(())
    }

    /// Spawn `max_concurrent` worker tasks, each runs in its own tokio task.
    /// Workers share the same Semaphore — a second Semaphore layer ensures
    /// we never exceed the desired concurrency even if all workers are busy.
    ///
    /// Lifecycle:
    ///   1. Worker acquires Semaphore permit → proceeds to pick work
    ///   2. No work → await notify.notified() (zero CPU, instant wakeup on new chunk)
    ///   3. Work found → process chunk, release permit, loop back
    ///   4. CancellationToken fires → finish current chunk, exit gracefully
    pub async fn run_workers(&self) {
        let mut handles = Vec::with_capacity(self.max_concurrent);
        for worker_id in 0..self.max_concurrent {
            let semaphore = self.semaphore.clone();
            let notify = self.notify.clone();
            let cancel = self.cancellation_token.clone();
            // Each worker needs its own Arc references to shared state
            let queue = self.arc_clone();  // hypothetical clone for brevity
            handles.push(tokio::spawn(async move {
                queue.worker_loop(worker_id, semaphore, notify, cancel).await;
            }));
        }
        // Wait for all workers to exit (when cancelled)
        for h in handles {
            let _ = h.await;
        }
    }

    /// Single worker's event loop.
    async fn worker_loop(
        &self,
        worker_id: usize,
        semaphore: Arc<Semaphore>,
        notify: Arc<Notify>,
        cancel: CancellationToken,
    ) {
        loop {
            // --- Permit acquisition ---
            // Acquire before picking work to smoothly throttle dispatch.
            // If all permits are taken, workers queue up here naturally.
            let _permit = tokio::select! {
                permit = semaphore.acquire() => permit,
                _ = cancel.cancelled() => return,
            };

            // --- Pick next chunk (with fallback sleep+notify for steady state) ---
            let chunk = 'pick: loop {
                // Starvation guard
                self.boost_oldest_pending_if_due().await;

                match self.chunk_repo.find_pending_prioritized(1)
                    .ok().and_then(|v| v.into_iter().next())
                {
                    Some(c) => break 'pick c,
                    None => {
                        // No work: release permit and wait for notification
                        drop(_permit);
                        tokio::select! {
                            _ = notify.notified() => {
                                // Re-acquire permit before looping
                                let p = tokio::select! {
                                    permit = semaphore.acquire() => permit,
                                    _ = cancel.cancelled() => return,
                                };
                                // p shadows _permit; continue loop to retry pick
                                // (structured as re-entering the pick loop)
                            }
                            _ = cancel.cancelled() => return,
                        }
                        continue 'pick;
                    }
                }
            };

            // --- Starvation guard: individual boost ---
            if chunk.created_at.elapsed() > self.max_task_wait {
                let _ = self.chunk_repo.update_priority(&chunk.id, PRIORITY_HIGH);
            }

            // --- Mark processing (transactional handoff) ---
            if self.chunk_repo.update_status(&chunk.id, &ChunkStatus::Processing).is_err() {
                // Concurrent worker already picked it up; release and retry
                continue;
            }

            // --- Rate limit before API call ---
            self.rate_limiter.acquire().await;

            // --- Call MIMO API ---
            match self.client.synthesize(&chunk.text, &task_voice, &task_model, task_speed).await {
                Ok(audio) => {
                    let cache_key = format!("{}/{}", chunk.task_id, chunk.seq);
                    self.cache.put(&cache_key, audio).ok();
                    let cache_path = self.cache.disk_path(&cache_key);
                    self.chunk_repo.mark_done(&chunk.id, &cache_path, duration).ok();
                    let _ = self.event_tx.send(DomainEvent::ChunkCompleted { ... });
                }
                Err(AppError::RateLimited) => {
                    self.chunk_repo.update_status(&chunk.id, &ChunkStatus::Pending).ok();
                }
                Err(e) => {
                    let new_retry = chunk.retry_count + 1;
                    if new_retry < chunk.max_retries {
                        self.chunk_repo.update_status(&chunk.id, &ChunkStatus::Pending).ok();
                    } else {
                        self.chunk_repo.mark_failed(&chunk.id, &e.to_string()).ok();
                        let _ = self.event_tx.send(DomainEvent::ChunkFailed { ... });
                    }
                }
            }
            // _permit drops here → semaphore slot freed → next worker wakes
        }
    }

    /// Trigger graceful shutdown — all workers finish current chunk, then exit.
    pub async fn shutdown(&self) {
        self.cancellation_token.cancel();
        // Workers will exit on next cancel check. run_workers waits for join.
    }

    /// Crash recovery: reset all processing chunks to pending.
    pub async fn recover(&self) -> Result<usize, AppError> {
        self.chunk_repo.reset_processing_to_pending()
    }

    /// Starvation guard: find the oldest pending chunk (regardless of priority)
    /// and boost it to PRIORITY_HIGH so it gets picked up next cycle.
    async fn boost_oldest_pending(&self) {
        if let Ok(Some(chunk)) = self.chunk_repo.find_oldest_pending() {
            let _ = self.chunk_repo.update_priority(&chunk.id, PRIORITY_HIGH);
        }
    }
}
```

**Integration tests (in-memory SQLite + mock MimoClient):**
```rust
#[actix_rt::test]
async fn test_chunk_queue_process_success() {
    // Setup: in-memory DB, mock client returning OK
    // Insert 1 chunk → run queue for 1 cycle → verify status=done + event emitted
}
#[actix_rt::test]
async fn test_chunk_queue_retry_then_fail() {
    // Mock client returning error → verify retry_count incremented
    // After max_retries → status=Failed
}
#[actix_rt::test]
async fn test_chunk_queue_recovery() {
    // Set 2 chunks to 'processing' in DB → recover() → verify both 'pending'
}
#[actix_rt::test]
async fn test_chunk_queue_prioritized_order() {
    // Insert 3 chunks: high(1), normal(0), bulk(-1)
    // Run queue → verify high processed before normal before bulk
}
#[actix_rt::test]
async fn test_chunk_queue_starvation_guard() {
    // Insert 1 bulk chunk with old created_at
    // Run 100+ iterations → verify boost_oldest_pending fires → priority updated
}
```

#### Step 4.3: infra/queue/task_queue.rs

```rust
pub struct TaskQueue {
    pool: DbPool,
    task_repo: Arc<dyn TaskRepo>,
    chunk_repo: Arc<dyn ChunkRepo>,
    chunk_queue: Arc<ChunkQueue>,
    chunker: Arc<ChunkerComponents>,
    sse_bus: Arc<SseBus>,
}

impl TaskQueue {
    pub async fn enqueue(&self, task_id: &str) -> Result<(), AppError> {
        // Verify task exists and is in pending state
        let mut task = self.task_repo.find_by_id(task_id)?
            .ok_or(AppError::NotFound("Task not found".into()))?;

        // Update state to chunking
        self.task_repo.update_status(task_id, &TaskStatus::Chunking)?;

        // Chunk the text
        let segments = split_text(&task.content, 2000);
        let total = segments.len() as i32;

        // Create Chunk objects
        let chunks: Vec<Chunk> = segments.into_iter().enumerate().map(|(i, seg)| {
            Chunk::new(task_id, i as i32, &seg.text, seg.char_count, seg.token_count)
        }).collect();

        // Batch insert chunks
        self.chunk_repo.insert_batch(&chunks)?;

        // Update task with chunk count
        let task = self.task_repo.update_chunk_count(task_id, total)?;
        self.task_repo.update_status(task_id, &TaskStatus::Processing)?;

        // Insert into task_queue
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO task_queue (id, task_id, priority, status, created_at)
             VALUES (?1, ?2, ?3, 'processing', datetime('now'))",
            params![Id::new().as_str(), task_id, task.priority],
        )?;

        // Enqueue all chunks
        for chunk in &chunks {
            self.chunk_queue.enqueue(chunk.id.as_str(), task_id).await?;
        }

        Ok(())
    }

    /// Listen for chunk events → update task progress → trigger merge on all done.
    pub async fn listen(&self, mut event_rx: broadcast::Receiver<DomainEvent>) {
        while let Ok(event) = event_rx.recv().await {
            match event {
                DomainEvent::ChunkCompleted { task_id, .. } => {
                    let done = self.chunk_repo.count_by_task_status(&task_id, &ChunkStatus::Done).unwrap_or(0);
                    let total = self.chunk_repo.count_by_task_all(&task_id).unwrap_or(0);

                    self.task_repo.update_chunk_progress(
                        &task_id, total as i32, done as i32, 0
                    ).ok();

                    if done == total && total > 0 {
                        // All chunks done → trigger merge
                        let _ = self.event_tx.send(DomainEvent::AllChunksDone { task_id });
                    }
                }
                DomainEvent::AllChunksDone { task_id } => {
                    // Merge audio files into final WAV
                    let result = self.merge_task_audio(&task_id).await;
                    match result {
                        Ok((path, duration)) => {
                            self.task_repo.set_output(&task_id, &path, duration).ok();
                            self.task_repo.update_status(&task_id, &TaskStatus::Done).ok();
                            let batch = self.task_repo.find_batch_for_task(&task_id).ok().flatten();
                            let _ = self.event_tx.send(DomainEvent::TaskCompleted {
                                task_id, batch_id: batch, output_path: path, duration
                            });
                        }
                        Err(e) => {
                            self.task_repo.update_status(&task_id, &TaskStatus::Failed).ok();
                            let _ = self.event_tx.send(DomainEvent::TaskFailed {
                                task_id, error: e.to_string()
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Continue a task: check cache for done chunks, re-dispatch pending/failed.
    pub async fn continue_task(&self, task_id: &str) -> Result<(), AppError> {
        let chunks = self.chunk_repo.find_by_task(task_id)?;
        let mut re_dispatched = 0;
        for chunk in &chunks {
            match chunk.status {
                ChunkStatus::Done => {
                    if let Some(path) = &chunk.audio_path {
                        if !std::path::Path::new(path).exists() {
                            // Cache miss → reset
                            self.chunk_repo.update_status(&chunk.id, &ChunkStatus::Pending)?;
                            self.chunk_queue.enqueue(&chunk.id, task_id).await?;
                            re_dispatched += 1;
                        }
                    }
                }
                ChunkStatus::Failed | ChunkStatus::Pending => {
                    self.chunk_queue.enqueue(&chunk.id, task_id).await?;
                    re_dispatched += 1;
                }
                _ => {}
            }
        }
        if re_dispatched > 0 {
            self.task_repo.update_status(task_id, &TaskStatus::Processing)?;
        }
        Ok(())
    }

    /// Isolated merge retry: re-attempt audio merge for a task that has all chunks done
    /// but whose merge previously failed. This does NOT re-enqueue chunks — it only
    /// re-runs the merge step, providing clean error isolation from chunk processing.
    ///
    /// Call this when a Task is in MergingFailed state.
    ///
    /// Returns Ok if merge succeeds, AppError::MergeFailed on failure (caller decides retry).
    pub async fn retry_merge(&self, task_id: &str) -> Result<(), AppError> {
        let task = self.task_repo.find(task_id)?
            .ok_or(AppError::NotFound("Task not found".into()))?;

        // Only retry if task is in Merging or Failed state
        if task.status != TaskStatus::Merging && task.status != TaskStatus::Failed {
            return Err(AppError::InvalidInput(
                format!("retry_merge requires Merging/Failed state, got {:?}", task.status)
            ));
        }

        // Verify all chunks are done
        let chunks = self.chunk_repo.find_by_task(task_id)?;
        let all_done = chunks.iter().all(|c| c.status == ChunkStatus::Done);
        if !all_done {
            return Err(AppError::InvalidInput(
                "Cannot retry merge: not all chunks are done".into()
            ));
        }

        // Re-attempt merge
        match self.merge_task_audio(task_id).await {
            Ok((path, duration)) => {
                self.task_repo.set_output(task_id, &path, duration)?;
                self.task_repo.update_status(task_id, &TaskStatus::Done)?;
                let _ = self.event_tx.send(DomainEvent::TaskCompleted {
                    task_id: task_id.to_string(),
                    batch_id: self.task_repo.find_batch_for_task(task_id).ok().flatten(),
                    output_path: path,
                    duration,
                });
                Ok(())
            }
            Err(e) => {
                self.task_repo.update_status(task_id, &TaskStatus::MergingFailed)?;
                let _ = self.event_tx.send(DomainEvent::TaskFailed {
                    task_id: task_id.to_string(),
                    error: e.to_string(),
                });
                Err(AppError::MergeFailed(e.to_string()))
            }
        }
    }

    async fn merge_task_audio(&self, task_id: &str) -> Result<(String, f64), AppError> {
        let chunks = self.chunk_repo.find_by_task(task_id)?;
        let paths: Vec<PathBuf> = chunks.iter()
            .filter(|c| c.status == ChunkStatus::Done)
            .filter_map(|c| c.audio_path.as_ref().map(PathBuf::from))
            .collect();

        let output_dir = PathBuf::from("output").join("wav").join(task_id);
        let output_path = output_dir.join("merged.wav");
        merge_wavs(&paths, &output_path)
    }
}
```

**Integration tests:**
```rust
#[actix_rt::test]
async fn test_task_queue_enqueue_creates_chunks() {
    // Enqueue task with 5000-char text → verify 3 chunks created in DB
}
#[actix_rt::test]
async fn test_task_queue_continue_cache_miss() {
    // Mark chunk done with non-existent audio_path → continue → chunk reset to pending
}
#[actix_rt::test]
async fn test_task_queue_merge_on_all_done() {
    // Manually set all chunks done → listen receives AllChunksDone → merge triggered
}
#[actix_rt::test]
async fn test_task_queue_retry_merge_success() {
    // 1. Create task with 3 chunks all Done + audio_path valid
    // 2. Set task to MergingFailed
    // 3. Call retry_merge → verify TaskStatus::Done + output set
}
#[actix_rt::test]
async fn test_task_queue_retry_merge_fails_when_not_done() {
    // 1. Create task with 1 chunk Done, 1 chunk Pending
    // 2. Set task to MergingFailed
    // 3. Call retry_merge → verify AppError::InvalidInput returned
}
#[actix_rt::test]
async fn test_task_queue_retry_merge_idempotent() {
    // 1. retry_merge succeeds
    // 2. Call retry_merge again → verify error (task already Done)
}
```

#### Step 4.4: Wire queue startup (composition)

> This step adds the startup wiring that calls `recover()` then spawns the queue event loops in tokio.
> It should be called during app initialization in `main.rs` or `app.rs`.

```rust
/// Called once during app boot, before accepting requests.
pub async fn start_queues(app_state: &AppState) -> Result<(), AppError> {
    // Crash recovery: reset processing→pending so chunks get retried
    let recovered = app_state.chunk_queue.recover().await?;
    if recovered > 0 {
        log::info!("ChunkQueue: recovered {} chunks from previous session", recovered);
    }

    // Chunk dispatch loop (runs forever)
    let cq = app_state.chunk_queue.clone();
    tokio::spawn(async move {
        log::info!("ChunkQueue::run started");
        cq.run().await;
    });

    // Task event listener (listens for chunk progress → triggers merge)
    let tq = app_state.task_queue.clone();
    let rx = app_state.event_tx.subscribe();
    tokio::spawn(async move {
        log::info!("TaskQueue::listen started");
        tq.listen(rx).await;
    });

    Ok(())
}
```

**Integration test:**
```rust
#[actix_rt::test]
async fn test_queue_startup_recovery() {
    let state = setup_test_state().await;
    // Manually insert a chunk with status=Processing (simulating crash)
    state.chunk_repo.insert(Chunk::with_status("processing")).unwrap();
    // Call start_queues → verify chunk reset to pending + queues running
    start_queues(&state).await.unwrap();
    // Give tokio time to process
    tokio::time::sleep(Duration::from_millis(100)).await;
    let chunk = state.chunk_repo.find_pending(1).unwrap();
    assert!(chunk.is_some(), "recovered chunk should become pending");
}
```

**Commit:** `git commit -m "phase-4: add chunk queue, task queue, and rate limiter"`

---

### Phase 5: Service Layer

> Files: `service/batch_service.rs`, `service/task_service.rs`, `service/group_service.rs`

Service layer wires repos + queues together. Each service is what routes call.

#### Step 5.1: service/batch_service.rs

```rust
pub struct BatchService {
    batch_repo: Arc<dyn BatchRepo>,
    task_service: Arc<TaskService>,
    sse_bus: Arc<SseBus>,
    pool: DbPool,
    max_file_size: u64, // 512000 = 500KB
}

impl BatchService {
    pub async fn create(&self, req: CreateBatchRequest) -> Result<Batch, AppError> {
        let batch = Batch::new(req);
        self.batch_repo.insert_batch(&batch)?;
        Ok(batch)
    }

    pub async fn upload_files(
        &self,
        batch_id: &str,
        mut payload: Multipart,
    ) -> Result<UploadResponse, AppError> {
        let batch = self.batch_repo.find_batch(batch_id)?
            .ok_or(AppError::NotFound("Batch not found".into()))?;
        if batch.status != BatchStatus::Preparing {
            return Err(AppError::InvalidInput("Batch already submitted".into()));
        }

        let mut handles = Vec::new();
        let mut next_seq = self.batch_repo.count_pending_items(batch_id)? as i32 + 1;
        let mut seen_names = HashSet::new();

        while let Some(entry) = payload.next().await {
            let mut field = match entry {
                Ok(f) => f,
                Err(_) => continue,
            };

            // Read file metadata
            let content_type = field.content_disposition().clone();
            let original_name = content_type.get_filename()
                .unwrap_or("unknown.txt")
                .to_string();
            let size = field.size();

            // Skip oversized files
            if size > self.max_file_size {
                self.sse_bus.publish(&format!("batch:{}", batch_id),
                    &DomainEvent::FileTooLarge { ... });
                continue;
            }

            // Dedup filename
            let filename = if seen_names.contains(&original_name) {
                let stem = std::path::Path::new(&original_name)
                    .file_stem().unwrap().to_str().unwrap();
                let ext = std::path::Path::new(&original_name)
                    .extension().unwrap().to_str().unwrap();
                let deduped = format!("{}_{}.{}", stem, next_seq, ext);
                deduped
            } else {
                seen_names.insert(original_name.clone());
                original_name
            };

            let seq = next_seq;
            next_seq += 1;

            // Read file content
            let mut content = String::new();
            while let Some(chunk) = field.next().await {
                content.push_str(&String::from_utf8_lossy(&chunk?.to_vec()));
            }

            // Parse in background
            let batch_id_clone = batch_id.to_string();
            let filename_clone = filename.clone();
            let sse = self.sse_bus.clone();
            let repo = self.batch_repo.clone();
            let batch_defaults = BatchDefaults {
                voice: batch.voice.clone(),
                model: batch.model.clone(),
                style: batch.style.clone(),
                speed: batch.speed,
            };

            handles.push(tokio::spawn(async move {
                let chars = content.chars().count() as i64;
                let preview = content.chars().take(200).collect::<String>();
                let tokens = estimate_tokens(&content);

                let item = BatchPendingItem::new(
                    &batch_id_clone, seq, &filename_clone, &content,
                    &preview, chars, tokens, &batch_defaults,
                );
                repo.insert_pending_item(&item).ok();
                sse.publish(&format!("batch:{}", batch_id_clone),
                    &DomainEvent::FileParsed {
                        batch_id: Id::from_str(&batch_id_clone).unwrap(),
                        filename: filename_clone, seq, chars, tokens,
                    });
            }));
        }

        let total = handles.len();
        let results = futures::future::join_all(handles).await;
        let parsed = results.iter().filter(|r| r.is_ok()).count();
        let failed = total - parsed;

        self.sse_bus.publish(&format!("batch:{}", batch_id),
            &DomainEvent::ParsingComplete { batch_id: Id::from_str(batch_id).unwrap(), total, parsed, failed });

        Ok(UploadResponse { batch_id: batch_id.to_string(), status: "parsing_complete".into() })
    }

    pub async fn update_item(
        &self, batch_id: &str, seq: i32, overrides: ItemOverride
    ) -> Result<BatchPendingItem, AppError> {
        self.batch_repo.update_pending_item(batch_id, seq, overrides)
    }

    pub async fn submit(&self, batch_id: &str) -> Result<BatchSubmitResponse, AppError> {
        let mut batch = self.batch_repo.find_batch(batch_id)?
            .ok_or(AppError::NotFound("Batch not found".into()))?;
        if batch.status != BatchStatus::Preparing {
            return Err(AppError::InvalidInput("Batch already submitted".into()));
        }

        let item_count = self.batch_repo.count_pending_items(batch_id)?;
        if item_count == 0 {
            return Err(AppError::InvalidInput("No pending items to submit".into()));
        }

        let defaults = BatchDefaults {
            voice: batch.voice.clone(),
            model: batch.model.clone(),
            style: batch.style.clone(),
            speed: batch.speed,
        };

        let tasks = self.batch_repo.submit_batch(batch_id, &defaults)?;

        self.batch_repo.update_batch_status(batch_id, &BatchStatus::Queued)?;

        // Enqueue each task
        for task in &tasks {
            self.task_service.enqueue(task.id.as_str()).await?;
        }

        // Update batch aggregates
        let total_chars: i64 = tasks.iter().map(|t| t.total_chars).sum();
        let total_tokens: i64 = tasks.iter().map(|t| t.total_tokens).sum();

        Ok(BatchSubmitResponse {
            batch_id: batch_id.to_string(),
            status: "queued".into(),
            total_tasks: tasks.len() as i32,
            total_chars,
            total_tokens,
        })
    }
}
```

#### Step 5.2: service/task_service.rs

Wrap TaskQueue + TaskRepo. `create_single()`, `get()`, `enqueue()`, `continue_task()`.

**Tests:** All via actix-test route layer (Phase 6).

**Commit:** `git commit -m "phase-5: add batch service and task service layers"`

---

### Phase 6: Routes + E2E Tests

> Files: `routes/batches.rs`, `routes/batch_items.rs`, `routes/tasks.rs`, `routes/groups.rs`, `routes/sse.rs`, `routes/voices.rs`  
> Test files: `tests/e2e/batch_flow.rs`, `tests/e2e/task_lifecycle.rs`, `tests/e2e/sse_events.rs`

#### Step 6.1: Route handlers

Each handler is thin: validate input → call service → return JSON.

```rust
// routes/batches.rs
pub async fn create_batch(
    body: Json<CreateBatchRequest>,
    service: Data<AppState>,
) -> Result<HttpResponse, AppError> {
    let batch = service.batch_service.create(body.into_inner()).await?;
    Ok(HttpResponse::Created().json(batch))
}

pub async fn get_batch(
    path: Path<String>,
    service: Data<AppState>,
) -> Result<HttpResponse, AppError> {
    let batch_id = path.into_inner();
    let batch = service.batch_service.get_progress(&batch_id).await?;
    Ok(HttpResponse::Ok().json(batch))
}
```

#### Step 6.2: Route registration

```rust
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v1")
            .route("/batches", web::post().to(batches::create_batch))
            .route("/batches/{id}", web::get().to(batches::get_batch))
            .route("/batches/{id}", web::delete().to(batches::delete_batch))
            .route("/batches/{id}/files", web::post().to(batches::upload_files))
            .route("/batches/{id}/items", web::get().to(batch_items::list_items))
            .route("/batches/{id}/items/{seq}", web::patch().to(batch_items::update_item))
            .route("/batches/{id}/items", web::patch().to(batch_items::batch_update))
            .route("/batches/{id}/items/{seq}", web::delete().to(batch_items::delete_item))
            .route("/batches/{id}/submit", web::post().to(batch_items::submit))
            .route("/batches/{id}/continue", web::post().to(batch_items::continue_batch))
            .route("/tts/synthesize", web::post().to(tts::synthesize))
            .route("/tasks", web::get().to(tasks::list))
            .route("/tasks/{id}", web::get().to(tasks::get))
            .route("/groups", web::get().to(groups::list))
            .route("/groups", web::post().to(groups::create))
            .route("/voices", web::get().to(voices::list))
            .route("/sse", web::get().to(sse::stream))
    );
}
```

#### Step 6.3: E2E Tests (tests/e2e/batch_flow.rs)

```rust
use crate::common::{setup_test_app, create_multipart_file};

#[actix_rt::test]
async fn test_e2e_batch_full_flow() {
    let (app, state) = setup_test_app().await;

    // 1. Create batch
    let req = test::TestRequest::post()
        .uri("/api/v1/batches")
        .set_json(&json!({"voice": "xx-01", "model": "tts-1", "style": "活泼"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let batch: BatchResponse = test::read_body_json(resp).await;
    assert_eq!(batch.status, "preparing");

    // 2. Upload a file
    let file_content = "这是第一段文本内容。" .repeat(100);
    let multipart = create_multipart_form(&[("files", "test.txt", &file_content)]);
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/batches/{}/files", batch.id))
        .set_payload(multipart)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);

    // 3. List items
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/batches/{}/items?page=1&per_page=50", batch.id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let items: PaginatedItemsResponse = test::read_body_json(resp).await;
    assert_eq!(items.total, 1);
    assert_eq!(items.items[0].filename, "test.txt");
    assert!(items.items[0].total_chars > 0);

    // 4. Edit item
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/batches/{}/items/1", batch.id))
        .set_json(&json!({"title": "我的标题", "voice": "xx-02"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let item: BatchItemResponse = test::read_body_json(resp).await;
    assert_eq!(item.effective_title, "我的标题");
    assert_eq!(item.effective_voice, "xx-02");

    // 5. Submit
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/batches/{}/submit", batch.id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let submit: SubmitResponse = test::read_body_json(resp).await;
    assert_eq!(submit.total_tasks, 1);
    assert_eq!(submit.status, "queued");

    // 6. Verify batch progress
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/batches/{}", batch.id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let progress: BatchProgressResponse = test::read_body_json(resp).await;
    assert_eq!(progress.status, "queued");
    assert_eq!(progress.total_tasks, 1);
}

#[actix_rt::test]
async fn test_e2e_batch_validation_errors() {
    let (app, _) = setup_test_app().await;

    // Submit non-existent batch
    let req = test::TestRequest::post()
        .uri("/api/v1/batches/nonexistent/submit")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Submit empty batch (no files uploaded)
    let req = test::TestRequest::post()
        .uri("/api/v1/batches")
        .set_json(&json!({"voice": "xx-01", "model": "tts-1"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let batch: BatchResponse = test::read_body_json(resp).await;
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/batches/{}/submit", batch.id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
```

#### Step 6.4: E2E Task Lifecycle Test

```rust
#[actix_rt::test]
async fn test_e2e_task_lifecycle() {
    let (app, _) = setup_test_app().await;

    // Create single task (non-batch)
    let req = test::TestRequest::post()
        .uri("/api/v1/tts/synthesize")
        .set_json(&json!({
            "content": "Hello world, this is a test.",
            "voice": "xx-01",
            "model": "tts-1",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let task: TaskResponse = test::read_body_json(resp).await;
    assert_eq!(task.status, "pending");

    // Get task status
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/tasks/{}", task.id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let task: TaskResponse = test::read_body_json(resp).await;
    // Status should be processing or done (if mock API responds fast)
    assert!(task.status == "processing" || task.status == "done");
}
```

**Commit:** `git commit -m "phase-6: add route handlers and E2E API integration tests"`

---

### Phase 7: SSE Event Bus

> Files: `infra/sse_bus.rs`, `api/sse_events.rs`

#### Step 7.1: SSE wire format (`infra/sse_bus.rs`)

The SSE wire protocol uses a typed `SseEvent` wrapper that is JSON-serialized for the frontend. This separates internal domain events from the wire protocol.

```rust
/// Wire-level SSE event sent to frontend over SSE stream.
/// Designed for minimal payload — frontend fetches details via REST as needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum SseEvent {
    /// A chunk within a task has been synthesized.
    ChunkProgress {
        task_id: String,
        seq: i32,
        total_chunks: i32,
        done_chunks: i32,
        failed_chunks: i32,
    },
    /// A single task completed (Single or BatchChild).
    TaskDone {
        task_id: String,
        batch_id: Option<String>,
        output_path: String,
        duration: f64,
    },
    /// A single task failed.
    TaskFailed {
        task_id: String,
        batch_id: Option<String>,
        error: String,
    },
    /// All tasks in a batch completed successfully.
    BatchDone {
        batch_id: String,
        total_tasks: i32,
        total_duration: f64,
    },
    /// Some tasks in a batch failed; batch is considered failed.
    BatchFailed {
        batch_id: String,
        failed_count: i32,
        done_count: i32,
        error: String,
    },
    /// A group within a batch completed.
    GroupDone {
        group_id: String,
        batch_id: String,
    },
    /// Parsing progress during file upload.
    FileParsed {
        batch_id: String,
        filename: String,
        seq: i32,
        progress: (i32, i32), // (done, total)
    },
    /// Parsing of all uploaded files completed.
    ParsingComplete {
        batch_id: String,
        total: i32,
        failed: i32,
    },
}
```

**Wire format** (SSE `data:` field):
```
event: message
data: {"type":"ChunkProgress","data":{"task_id":"...","seq":1,"total_chunks":5,"done_chunks":2,"failed_chunks":0}}
```

**Commit:** `git commit -m "phase-7.1: add SseEvent wire format and conversions"`

#### Step 7.2: SSE broadcast infrastructure (`infra/sse_bus.rs`)

Internal bus using `DomainEvent` for type safety. A bridge actor converts `DomainEvent` → `SseEvent` for frontend SSE streams.

```rust
pub struct SseBus {
    subscribers: RwLock<HashMap<String, Vec<(String, UnboundedSender<SseEvent>)>>>,
}

impl SseBus {
    pub fn new() -> Self { Self { subscribers: RwLock::new(HashMap::new()) } }

    pub fn subscribe(&self, topic: &str) -> (String, UnboundedReceiver<SseEvent>) {
        let (tx, rx) = flume::unbounded();
        let id = Id::new().to_string();
        self.subscribers.write()
            .entry(topic.to_string())
            .or_default()
            .push((id.clone(), tx));
        (id, rx)
    }

    pub fn unsubscribe(&self, topic: &str, id: &str) {
        if let Some(subs) = self.subscribers.write().get_mut(topic) {
            subs.retain(|(sid, _)| sid != id);
        }
    }

    pub fn publish(&self, topic: &str, event: &SseEvent) {
        if let Some(subs) = self.subscribers.read().get(topic) {
            for (_, tx) in subs {
                let _ = tx.try_send(event.clone());
            }
        }
    }

    pub fn publish_global(&self, event: &DomainEvent) {
        self.publish("global", event);
    }
}
```

**Call queue startup (before SSE wiring):**
```rust
// In main.rs during boot, after building AppState:
start_queues(&app_state).await.map_err(|e| {
    eprintln!("FATAL: queue startup failed: {}", e);
    std::process::exit(1);
})?;
```

**Bridge actor: DomainEvent → SseEvent + auto-complete batches/groups:**

```rust
use SseEvent as E;

/// Converts internal DomainEvents to SseEvents and publishes to SseBus.
/// Also triggers batch/group auto-completion side effects.
pub fn spawn_sse_bridge(
    sse_bus: Arc<SseBus>,
    task_repo: Arc<dyn TaskRepo>,
    batch_repo: Arc<dyn BatchRepo>,
    group_repo: Arc<dyn GroupRepo>,
    event_tx: broadcast::Sender<DomainEvent>,
    mut event_rx: broadcast::Receiver<DomainEvent>,
) {
    tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            let sse = match &event {
                DomainEvent::ChunkCompleted { task_id, total_chunks, done_chunks, failed_chunks, .. } => {
                    E::ChunkProgress {
                        task_id: task_id.clone(),
                        seq: 0,
                        total_chunks: *total_chunks,
                        done_chunks: *done_chunks,
                        failed_chunks: *failed_chunks,
                    }
                }
                DomainEvent::ChunkFailed { task_id, seq, retry_count, .. } => {
                    E::TaskFailed {
                        task_id: task_id.clone(),
                        batch_id: None,
                        error: format!("chunk {} failed (retry {})", seq, retry_count),
                    }
                }
                DomainEvent::TaskCompleted { task_id, batch_id, output_path, duration } => {
                    // Publish task done
                    sse_bus.publish(&format!("task:{}", task_id),
                        &E::TaskDone {
                            task_id: task_id.clone(),
                            batch_id: batch_id.clone(),
                            output_path: output_path.clone(),
                            duration: *duration,
                        }
                    );

                    // If this task belongs to a batch, check batch completion
                    if let Some(bid) = batch_id {
                        if let Ok(progress) = task_repo.batch_progress(bid) {
                            if progress.total_tasks > 0
                                && progress.done_tasks == progress.total_tasks
                                && progress.failed_tasks == 0
                            {
                                batch_repo.update_batch_status(bid, &BatchStatus::Completed).ok();
                                let _ = event_tx.send(DomainEvent::BatchCompleted { batch_id: bid.clone() });
                            } else if progress.total_tasks > 0
                                && (progress.done_tasks + progress.failed_tasks) == progress.total_tasks
                                && progress.failed_tasks > 0
                            {
                                batch_repo.update_batch_status(bid, &BatchStatus::Failed).ok();
                                let _ = event_tx.send(DomainEvent::BatchFailed {
                                    batch_id: bid.clone(),
                                    error: format!("{} tasks failed", progress.failed_tasks),
                                    failed_count: progress.failed_tasks,
                                });
                            }
                        }
                    }

                    // Skip the generic SSE publish below — already handled above
                    continue;
                }
                DomainEvent::TaskFailed { task_id, error } => {
                    E::TaskFailed {
                        task_id: task_id.clone(),
                        batch_id: None,
                        error: error.clone(),
                    }
                }
                DomainEvent::BatchCompleted { batch_id } => {
                    if let Ok(tasks) = task_repo.find_by_batch(batch_id) {
                        let total_duration: f64 = tasks.iter()
                            .filter_map(|t| t.output_duration)
                            .sum();
                        sse_bus.publish(&format!("batch:{}", batch_id),
                            &E::BatchDone {
                                batch_id: batch_id.clone(),
                                total_tasks: tasks.len() as i32,
                                total_duration,
                            }
                        );
                    }
                    continue;
                }
                DomainEvent::BatchFailed { batch_id, error, failed_count } => {
                    let done = task_repo.batch_progress(batch_id)
                        .map(|p| p.done_tasks).unwrap_or(0);
                    sse_bus.publish(&format!("batch:{}", batch_id),
                        &E::BatchFailed {
                            batch_id: batch_id.clone(),
                            failed_count: *failed_count,
                            done_count: done,
                            error: error.clone(),
                        }
                    );
                    continue;
                }
                DomainEvent::GroupCompleted { group_id, batch_id } => {
                    sse_bus.publish(&format!("batch:{}", batch_id),
                        &E::GroupDone {
                            group_id: group_id.clone(),
                            batch_id: batch_id.clone(),
                        }
                    );
                    continue;
                }
                DomainEvent::FileParsed { batch_id, .. } => {
                    E::FileParsed {
                        batch_id: batch_id.clone(),
                        filename: "...".into(),
                        seq: 0,
                        progress: (0, 0),
                    }
                }
                DomainEvent::ParsingComplete { batch_id, total, failed } => {
                    E::ParsingComplete {
                        batch_id: batch_id.clone(),
                        total: *total as i32,
                        failed: *failed as i32,
                    }
                }
                _ => continue, // skip unhandled event types
            };

            // Publish converted event to relevant SSE topics
            sse_bus.publish("global", &sse);
        }
    });
}
```

**Usage in main.rs:**
```rust
spawn_sse_bridge(
    app_state.sse_bus.clone(),
    app_state.task_repo.clone(),
    app_state.batch_repo.clone(),
    app_state.group_repo.clone(),
    app_state.event_tx.clone(),
    app_state.event_tx.subscribe(),
);
```

**E2E tests:**
```rust
#[actix_rt::test]
async fn test_sse_wire_format_delivers_sse_event() {
    let (app, state) = setup_test_app().await;

    // Subscribe to SSE
    let (sub_id, mut rx) = state.sse_bus.subscribe("test:batch");

    // Publish SseEvent directly
    let sse = SseEvent::ParsingComplete {
        batch_id: "batch-001".into(),
        total: 10,
        failed: 0,
    };
    state.sse_bus.publish("test:batch", &sse);

    // Verify received
    let received = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await;
    assert!(received.is_ok());
    let received_event = received.unwrap();
    match received_event {
        SseEvent::ParsingComplete { batch_id, total, .. } => {
            assert_eq!(batch_id, "batch-001");
            assert_eq!(total, 10);
        }
        _ => panic!("Wrong event type"),
    }
}

#[actix_rt::test]
async fn test_domain_bridge_converts_and_publishes() {
    let (app, state) = setup_test_app().await;

    // Subscribe to SSE on global topic
    let (sub_id, mut rx) = state.sse_bus.subscribe("global");

    // Run the bridge in background
    spawn_sse_bridge(
        state.sse_bus.clone(),
        state.task_repo.clone(),
        state.batch_repo.clone(),
        state.group_repo.clone(),
        state.event_tx.clone(),
        state.event_tx.subscribe(),
    );

    // Publish a DomainEvent that triggers SSE output
    let _ = state.event_tx.send(DomainEvent::ChunkCompleted {
        chunk_id: "chunk-1".into(),
        task_id: "task-1".into(),
        seq: 0,
        audio_path: "/tmp/c0.wav".into(),
        duration: 2.5,
    });

    // Should receive ChunkProgress on global
    let received = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await;
    assert!(received.is_ok());
    match received.unwrap() {
        SseEvent::ChunkProgress { task_id, .. } => {
            assert_eq!(task_id, "task-1");
        }
        other => panic!("Expected ChunkProgress, got {:?}", other),
    }
}

#[actix_rt::test]
async fn test_e2e_batch_auto_completes_on_last_task_done() {
    let (app, state) = setup_test_app().await;
    let batch_id = "batch-e2e-001";

    // Insert a batch with 2 tasks
    let batch = Batch::new(CreateBatchRequest { name: "test".into(), .. });
    state.batch_repo.insert_batch(&batch).unwrap();
    state.batch_repo.update_batch_status(batch_id, &BatchStatus::Queued).unwrap();

    // Wire up the bridge
    spawn_sse_bridge(
        state.sse_bus.clone(),
        state.task_repo.clone(),
        state.batch_repo.clone(),
        state.group_repo.clone(),
        state.event_tx.clone(),
        state.event_tx.subscribe(),
    );

    let task1 = Task::new(...); // batch_id=batch-e2e-001
    let task2 = Task::new(...); // batch_id=batch-e2e-001
    state.task_repo.insert(&task1).unwrap();
    state.task_repo.insert(&task2).unwrap();

    // Mark both tasks done
    state.task_repo.update_status(&task1.id, &TaskStatus::Done).unwrap();
    state.task_repo.update_status(&task2.id, &TaskStatus::Done).unwrap();

    // Publish TaskCompleted for the last task — triggers batch check
    let _ = state.event_tx.send(DomainEvent::TaskCompleted {
        task_id: task2.id.clone(),
        batch_id: Some(batch_id.into()),
        output_path: "/tmp/out.wav".into(),
        duration: 10.0,
    });

    // Allow async handler to run
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Batch should be Completed
    let updated = state.batch_repo.find_batch(batch_id).unwrap().unwrap();
    assert_eq!(updated.status, BatchStatus::Completed);
}

#[actix_rt::test]
async fn test_e2e_batch_marks_failed_on_partial_failure() {
    let (app, state) = setup_test_app().await;
    let batch_id = "batch-e2e-002";

    let batch = Batch::new(CreateBatchRequest { name: "test".into(), .. });
    state.batch_repo.insert_batch(&batch).unwrap();
    state.batch_repo.update_batch_status(batch_id, &BatchStatus::Queued).unwrap();

    spawn_sse_bridge(
        state.sse_bus.clone(),
        state.task_repo.clone(),
        state.batch_repo.clone(),
        state.group_repo.clone(),
        state.event_tx.clone(),
        state.event_tx.subscribe(),
    );

    let task1 = Task::new(...); // batch_id=batch-e2e-002
    let task2 = Task::new(...); // batch_id=batch-e2e-002
    state.task_repo.insert(&task1).unwrap();
    state.task_repo.insert(&task2).unwrap();

    // One done, one failed
    state.task_repo.update_status(&task1.id, &TaskStatus::Done).unwrap();
    state.task_repo.update_status(&task2.id, &TaskStatus::Failed).unwrap();

    // Publish last TaskCompleted
    let _ = state.event_tx.send(DomainEvent::TaskCompleted {
        task_id: task1.id.clone(),
        batch_id: Some(batch_id.into()),
        output_path: "/tmp/out.wav".into(),
        duration: 5.0,
    });

    tokio::time::sleep(Duration::from_millis(200)).await;
    let updated = state.batch_repo.find_batch(batch_id).unwrap().unwrap();
    assert_eq!(updated.status, BatchStatus::Failed);
}
```

**Commit:** `git commit -m "phase-7: add SSE event bus, wire protocol, DomainEvent→SseEvent bridge, and batch auto-completion"`

---

### Phase 8: AppState Consolidation

Replace legacy AppState:

```rust
// OLD AppState (messy):
tasks: HashMap<String, TtsTask>
batch_groups: HashMap<String, BatchGroup>
batch_imports: BatchImportManager
task_events: HashMap<String, Vec<Sender<TaskEvent>>>

// NEW AppState (clean):
pub struct AppState {
    pub config: AppConfig,
    pub db_pool: DbPool,
    pub batch_repo: Arc<dyn BatchRepo>,
    pub task_repo: Arc<dyn TaskRepo>,
    pub chunk_repo: Arc<dyn ChunkRepo>,
    pub group_repo: Arc<dyn GroupRepo>,
    pub batch_service: Arc<BatchService>,
    pub task_service: Arc<TaskService>,
    pub group_service: Arc<GroupService>,
    pub chunk_queue: Arc<ChunkQueue>,
    pub task_queue: Arc<TaskQueue>,
    pub sse_bus: Arc<SseBus>,
    pub cache: Arc<Cache>,
    pub event_tx: broadcast::Sender<DomainEvent>,
}
```

Remove sled initialization. Replace with SQLite pool. Register new routes under `/api/v2/` prefix (version identifier, not coexistence).

**New route table:**
| Method | Path | Handler | Notes |
|--------|------|---------|-------|
| POST | `/api/v2/batches` | `batches::create` | Create batch with defaults |
| POST | `/api/v2/batches/{id}/files` | `batches::upload_files` | Multipart file upload |
| GET | `/api/v2/batches/{id}/items` | `batch_items::list` | Paginated pending items |
| PATCH | `/api/v2/batches/{id}/items/{seq}` | `batch_items::update_item` | Edit single item |
| POST | `/api/v2/batches/{id}/submit` | `batch_items::submit` | Commit batch → create tasks |
| POST | `/api/v2/batches/{id}/continue` | `batch_items::continue_batch` | Resume batch after crash |
| POST | `/api/v2/tts/synthesize` | `tts::synthesize` | Single TTS (same API, new impl) |
| GET | `/api/v2/tasks` | `tasks::list` | Task list with filters |
| GET | `/api/v2/tasks/{id}` | `tasks::get` | Single task detail |
| POST | `/api/v2/tasks/{id}/retry-merge` | `tasks::retry_merge` | Trigger merge retry |
| DELETE | `/api/v2/tasks/{id}` | `tasks::delete` | Cancel/delete task |
| POST | `/api/v2/tasks/{id}/retry` | `tasks::retry` | Retry failed task |
| GET | `/api/v2/groups` | `groups::list` | Batch groups list |
| POST | `/api/v2/groups` | `groups::create` | Create group |
| GET | `/api/v2/voices` | `voices::list` | Voice list (shared) |
| GET | `/api/v2/sse` | `sse::stream` | SSE stream |

Remove ALL old route files (`batch_import.rs` route handlers, old `tts/synthesize` route). Remove legacy `HashMap` fields from AppState entirely — no transitional code.

**Verification:** `cargo build` passes. `cargo test --all-features` — all E2E and integration tests pass. No references to old AppState fields or old route files remain.

**Commit:** `git commit -m "phase-8: consolidate AppState, replace with clean AppState + new route table, remove all legacy code"`

---

### Phase 9: sled Offboarding

1. Remove `sled = "0.34"` from Cargo.toml
2. Remove or comment out all sled-related code
3. Remove sled data dir from config
4. `cargo build` passes cleanly
5. `cargo test --all-features` — all tests still pass

**Commit:** `git commit -m "phase-9: remove sled dependency"`

---

## Dependencies

```toml
# Current Cargo.toml already has these, verify features:
uuid = { version = "1.6", features = ["v4", "v7", "serde", "fast-rng"] }
chrono = { version = "0.4", features = ["serde"] }
rusqlite = { version = "0.31", features = ["bundled"] }
r2d2 = "0.8"
r2d2_sqlite = "0.24"
thiserror = "1"
futures = "0.3"
flume = "0.11"
parking_lot = "0.12"
reqwest = { version = "0.12", features = ["json", "stream"] }
actix-multipart = "0.7"
tracing = "0.1"

# Dev dependencies (already present)
actix-rt = "2"
actix-test = "0.1"

# May want to add for testing:
[dev-dependencies]
wiremock = "0.6"  # For mocking MIMO API in integration tests
tempfile = "3"    # For temp directories in cache/merger tests
```

---

## Verification Checklist

### Per-Phase

| Phase | Command | Expected |
|-------|---------|----------|
| 0 | `cargo test shared:: --lib` | All Id, Error, DB tests pass |
| 1 | `cargo test domain:: --lib` | Status machine, events, effective fields |
| 2 | `cargo test --test integration -- repo` | All repo CRUD + edge cases |
| 3 | `cargo test --lib` | Chunker, merger, cache, client |
| 4 | `cargo test --test integration -- queue` | ChunkQueue + TaskQueue flow |
| 5 | `cargo test --test integration -- service` | BatchService + TaskService |
| 6 | `cargo test --test e2e` | Full API flow tests pass |
| 7 | `cargo test --test e2e -- sse` | SSE events delivered |
| 8 | `cargo build` | Compiles with new AppState |
| 9 | `cargo build` | No sled dependency |

### 手术刀 Fix Checklist

- [ ] Captured exact error + stack trace
- [ ] Root cause identified (test bug vs code bug)
- [ ] Change is minimal (only what this test needs)
- [ ] No `.unwrap()` / `#[allow]` / unsafe added in production code
- [ ] LSP diagnostics clean on changed files
- [ ] `cargo test --all-features` regressions = 0
- [ ] Committed: `git commit -m "phase-N: description"`

---

## Risk Register

| Risk | Mitigation |
|------|------------|
| Large refactor destabilizes | Incremental phases. Each phase independently testable. Old routes coexist until Phase 6. |
| SQLite concurrent access | r2d2 pool + WAL mode. ChunkQueue dispatch is single-threaded internally. |
| Cache eviction mid-task | Configurable TTL (24h default). Disk-based + path in SQLite. Resumable on cache miss. |
| MIMO API rate limiting | Token bucket + retry with backoff. Queue pauses on 429 responses. |
| Upload memory pressure | Stream-to-temp with 500KB per-file limit. Client-side pre-validation assumed. |
| sled data abandoned | Fresh start. Old sled file not deleted — user can manually archive. |
