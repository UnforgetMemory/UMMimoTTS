//! Engine — the v4 orchestration core (ADR-004 / ADR-012).
//!
//! - Memory priority queue seeded from SQLite (durable source of truth)
//! - `notify` wakeups, idle backoff 50→500ms (v3's 200-QPS polling is gone)
//! - Per-provider AIMD gate + per-budget-group RPM/TPM buckets
//! - Streaming pcm16 synthesis → raw PCM on disk → single WAV merge
//! - ADR-013 error routing: 421 never retried, 400 context overflow re-chunks ×0.8

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use futures_util::StreamExt;
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use tokio::sync::Notify;

use mimotts_core::audio;
use mimotts_core::chunking::{self, ChunkConfig};
use mimotts_core::crypto::{hash_token, Crypto, MasterKey};
use mimotts_core::domain::{Chunk, CreateTaskInput, Id, Task, TaskStatus};
use mimotts_core::events::DomainEvent;

use crate::bus::Bus;
use crate::error::EngineError;
use crate::mimo::{AudioChunk, MimoClient, SynthesisRequest, VoiceSpec};
use crate::storage::{create_pool, run_migrations, Storage};
use crate::throttle::{AimdGate, AimdGateConfig, BudgetGroup, TokenBucket};

// ── config ───────────────────────────────────────────────────────────────

pub struct EngineConfig {
    pub db_path: String,
    pub data_dir: PathBuf,
    pub output_dir: PathBuf,
    pub workers: usize,
    pub rpm_headroom: u64,
    pub tpm_budget: u64,
    pub chunk: ChunkConfig,
    pub max_window: u32,
    pub stream_audio: bool,
    pub announcement: Option<String>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            db_path: "data/mimo.db".into(),
            data_dir: PathBuf::from("data"),
            output_dir: PathBuf::from("data/output"),
            workers: 32,
            rpm_headroom: 90,          // 10% under official 100 RPM
            tpm_budget: 10_000_000,    // official 10M TPM
            chunk: ChunkConfig::default(),
            max_window: 32,            // long chunks need real concurrency
            stream_audio: true,
            announcement: None,
        }
    }
}

// ── in-memory queue ──────────────────────────────────────────────────────

struct Queue {
    buckets: Mutex<BTreeMap<i64, VecDeque<(String, String)>>>, // priority → (chunk_id, task_id)
    /// Membership set: a chunk may be queued at most once. Recovery re-seeds
    /// DB-pending chunks every 30s; without dedup each pass duplicates the
    /// in-queue survivors (RPM pacing keeps them resident for minutes) and
    /// the queue grows without bound.
    ids: Mutex<HashSet<String>>,
    len: AtomicUsize,
    notify: Notify,
}

impl Queue {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            buckets: Mutex::new(BTreeMap::new()),
            ids: Mutex::new(HashSet::new()),
            len: AtomicUsize::new(0),
            notify: Notify::new(),
        })
    }
    /// Push (no-op when the chunk is already queued).
    fn push(&self, priority: i64, chunk_id: String, task_id: String) {
        if !self.ids.lock().insert(chunk_id.clone()) {
            return;
        }
        self.buckets
            .lock()
            .entry(priority)
            .or_default()
            .push_back((chunk_id, task_id));
        self.len.fetch_add(1, Ordering::AcqRel);
        self.notify.notify_one();
    }
    fn pop(&self) -> Option<(String, String)> {
        let mut buckets = self.buckets.lock();
        // Highest priority first: BTreeMap is ascending, so take the LAST
        // (max-key) entry. Empty buckets are removed to avoid lock spinning.
        while let Some(mut entry) = buckets.last_entry() {
            if let Some(item) = entry.get_mut().pop_front() {
                self.len.fetch_sub(1, Ordering::AcqRel);
                self.ids.lock().remove(&item.0);
                return Some(item);
            }
            entry.remove();
        }
        None
    }
    fn len(&self) -> usize {
        self.len.load(Ordering::Acquire)
    }
}

// ── provider runtime (gate per provider, budget per account group) ───────

struct ProviderRuntime {
    gate: Arc<AimdGate>,
    group: String,
}

// ── per-task incremental assembler ────────────────────────────────────────
//
// Every done chunk's raw PCM is appended to ONE shared raw stream per task,
// immediately, in completion order (finalize reorders by seq using the byte
// ranges recorded in the DB). The per-chunk PCM file is deleted right after
// the append, so audio is never stored twice and the merge stream grows in
// REAL TIME with O(1) memory.
//
// Crash-resume: ranges live in the DB and the stream survives on disk; a
// restarted engine re-opens the stream at its existing size (stale tails
// beyond recorded ranges are never read).

pub(crate) struct Assembler {
    /// Current end-of-stream offset (append position).
    end: Mutex<u64>,
    raw_path: PathBuf,
    /// Set once finalize starts: no further appends (stray workers no-op).
    finished: std::sync::atomic::AtomicBool,
}

impl Assembler {
    fn new(raw_path: PathBuf) -> Self {
        // Resume at the stream's current size (ranges already recorded).
        let end = std::fs::metadata(&raw_path).map(|m| m.len()).unwrap_or(0);
        Self {
            end: Mutex::new(end),
            raw_path,
            finished: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn raw_path(&self) -> &Path {
        &self.raw_path
    }

    /// Append a chunk's raw PCM to the shared stream; returns (offset, len)
    /// for the DB range record.
    fn append(&self, chunk_path: &Path) -> Result<(u64, u64), EngineError> {
        let mut end = self.end.lock();
        if self.finished.load(Ordering::Acquire) {
            return Ok((*end, 0));
        }
        let mut out = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.raw_path)?;
        let mut inp = std::fs::File::open(chunk_path)?;
        let n = match std::io::copy(&mut inp, &mut out) {
            Ok(n) => n,
            Err(e) => {
                // Roll the shared stream back: a partial write must not
                // desync `end` from the real file length, or the next append
                // records a range that includes garbage bytes.
                let _ = std::fs::OpenOptions::new()
                    .write(true)
                    .open(&self.raw_path)
                    .and_then(|f| f.set_len(*end));
                return Err(EngineError::Internal(format!("pcm copy: {e}")));
            }
        };
        let offset = *end;
        *end += n;
        Ok((offset, n))
    }

    /// Stop accepting appends (finalize owns the task's output now).
    fn finish(&self) {
        self.finished.store(true, Ordering::Release);
    }
}

// ── engine ───────────────────────────────────────────────────────────────

pub struct Engine {
    pub storage: Storage,
    pub bus: Arc<Bus>,
    cfg: EngineConfig,
    crypto: Crypto,
    master_key: MasterKey,
    client: MimoClient,
    queue: Arc<Queue>,
    runtimes: RwLock<HashMap<String, ProviderRuntime>>,
    budgets: RwLock<HashMap<String, Arc<BudgetGroup>>>,
    /// Live per-task PCM assembly (real-time merge; rebuilt on restart).
    assemblers: RwLock<HashMap<String, Arc<Assembler>>>,
    /// Per-task re-chunk escalation depth (cumulative budget shrink).
    rechunk_depth: Mutex<HashMap<String, u32>>,
    cancelled: AtomicBool,
    pub workers: AtomicUsize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub kind: String,
    pub is_configured: bool,
    pub is_default: bool,
    pub budget_group: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    pub session_id: String,
    pub files_received: usize,
    pub tasks_created: usize,
    pub rejected: Vec<String>,
}

impl Engine {
    pub fn open(cfg: EngineConfig) -> Result<Arc<Self>, EngineError> {
        std::fs::create_dir_all(&cfg.data_dir)?;
        let chunks_dir = cfg.data_dir.join("chunks");
        std::fs::create_dir_all(&chunks_dir)?;
        std::fs::create_dir_all(&cfg.output_dir)?;

        let master_key = load_or_create_master_key(&cfg.data_dir)?;
        let crypto = Crypto::new(&master_key);
        let pool = create_pool(&cfg.db_path)?;
        run_migrations(&*pool.get()?)?;
        let storage = Storage::new(pool);
        // Fresh process: every `inflight` row belongs to a worker that is
        // already dead — reset immediately so a restarted engine resumes
        // instantly (periodic recovery still covers runtime panics).
        let revived = storage.reset_stale_inflight(0)?;
        if revived > 0 {
            tracing::warn!("startup recovery: reset {revived} stale inflight chunks");
        }
        let bus = Bus::new();
        let queue = Queue::new();

        let engine = Arc::new(Self {
            storage,
            bus,
            crypto,
            master_key,
            client: MimoClient::new(),
            queue,
            runtimes: RwLock::new(HashMap::new()),
            budgets: RwLock::new(HashMap::new()),
            assemblers: RwLock::new(HashMap::new()),
            rechunk_depth: Mutex::new(HashMap::new()),
            cancelled: AtomicBool::new(false),
            workers: AtomicUsize::new(cfg.workers),
            cfg,
        });

        engine.seed_queue()?;
        engine.start_workers();
        engine.start_recovery();
        Ok(engine)
    }

    pub fn config(&self) -> EngineConfigSnapshot {
        EngineConfigSnapshot {
            chunk_target_tokens: self.cfg.chunk.target_tokens,
            chunk_hard_cap_tokens: self.cfg.chunk.hard_cap_tokens,
            context_window_tokens: chunking::CONTEXT_WINDOW_TOKENS,
            workers: self.cfg.workers,
            stream_audio: self.cfg.stream_audio,
            announcement: self.cfg.announcement.clone(),
        }
    }

    pub fn shutdown(&self) {
        self.cancelled.store(true, Ordering::Release);
        // Wake parked workers/recovery immediately instead of waiting out
        // their sleep (up to 500ms workers / 30s recovery).
        self.queue.notify.notify_waiters();
    }

    // ── providers ────────────────────────────────────────────────────────

    pub fn providers(&self) -> Result<Vec<ProviderInfo>, EngineError> {
        let rows = self.storage.providers()?;
        Ok(rows
            .into_iter()
            .map(|r| ProviderInfo {
                id: r.id,
                name: r.name,
                base_url: r.base_url,
                kind: r.kind,
                is_configured: r.is_configured,
                is_default: r.is_default,
                budget_group: r.budget_group,
            })
            .collect())
    }

    pub fn set_provider_key(&self, id: &str, api_key: &str) -> Result<(), EngineError> {
        if self.storage.provider(id)?.is_none() {
            return Err(EngineError::NotFound(format!("provider {id}")));
        }
        let sealed = if api_key.trim().is_empty() {
            String::new()
        } else {
            self.crypto.seal(api_key.trim())
        };
        self.storage
            .set_provider_key(id, &sealed, !sealed.is_empty())?;
        // Reset breaker state on key change (fresh credentials) and stop the
        // old gate's health loop so it can drop.
        if let Some(rt) = self.runtimes.write().remove(id) {
            rt.gate.close();
        }
        Ok(())
    }

    pub fn set_default_provider(&self, id: &str) -> Result<(), EngineError> {
        self.storage.set_default_provider(id)
    }

    pub fn edit_provider(
        &self,
        id: &str,
        name: Option<&str>,
        base_url: Option<&str>,
        budget_group: Option<&str>,
    ) -> Result<(), EngineError> {
        if let Some(b) = base_url {
            let b = b.trim();
            if !(b.starts_with("http://") || b.starts_with("https://")) {
                return Err(EngineError::InvalidInput(format!(
                    "base_url must be http(s): {b}"
                )));
            }
        }
        self.storage.edit_provider(id, name, base_url, budget_group)?;
        // base_url changes invalidate in-flight runtimes (gate state); stop
        // the old gate's health loop so it can drop.
        if base_url.is_some() {
            if let Some(rt) = self.runtimes.write().remove(id) {
                rt.gate.close();
            }
        }
        Ok(())
    }

    /// Test/demo hook: repoint every seeded provider at one upstream base URL.
    /// Used by e2e (mock MiMo server) and local experiments.
    pub fn override_provider_base_urls(&self, base_url: &str) -> Result<(), EngineError> {
        let conn = self.storage.pool.get()?;
        conn.execute(
            "UPDATE providers SET base_url = ?1",
            rusqlite::params![base_url],
        )?;
        Ok(())
    }

    fn resolve_provider(&self, provider_id: Option<&str>) -> Result<Option<crate::storage::ProviderRow>, EngineError> {
        if let Some(pid) = provider_id {
            return self.storage.provider(pid);
        }
        // default → first configured → first row
        let all = self.storage.providers()?;
        Ok(all
            .iter()
            .find(|p| p.is_default)
            .or_else(|| all.iter().find(|p| p.is_configured))
            .or_else(|| all.first())
            .cloned())
    }

    // ── sessions ─────────────────────────────────────────────────────────

    pub fn create_session(&self, name: &str) -> Result<mimotts_core::domain::Session, EngineError> {
        self.storage.create_session(name)
    }

    pub fn list_sessions(
        &self,
        page: i64,
        page_size: i64,
    ) -> Result<(Vec<crate::storage::SessionRow>, i64), EngineError> {
        self.storage.sessions(page, page_size)
    }

    pub fn session(&self, id: &str) -> Result<Option<crate::storage::SessionRow>, EngineError> {
        self.storage.session(id)
    }

    pub fn delete_session(&self, id: &str) -> Result<(), EngineError> {
        // Per-task deletion reclaims merged outputs AND chunk PCM files.
        let task_ids = self.storage.session_task_ids(id)?;
        for tid in task_ids {
            let _ = self.delete_task(&tid);
        }
        self.storage.delete_session(id)
    }

    pub fn cancel_session(&self, id: &str) -> Result<(), EngineError> {
        let task_ids = self.storage.session_task_ids(id)?;
        for tid in task_ids {
            let _ = self.cancel_task(&tid);
        }
        Ok(())
    }

    // ── tasks ────────────────────────────────────────────────────────────

    pub fn submit_task(&self, input: CreateTaskInput) -> Result<Task, EngineError> {
        // Validate before insert: a failed enqueue must not leave an orphaned
        // 0-chunk Pending row (umreview finding).
        if input.content.trim().is_empty() {
            return Err(EngineError::InvalidInput("empty content".into()));
        }
        let task = Task::new(input);
        self.storage.insert_task(&task)?;
        self.enqueue_task_inner(&task, self.cfg.chunk)?;
        Ok(task)
    }

    fn enqueue_task_inner(&self, task: &Task, chunk_cfg: ChunkConfig) -> Result<(), EngineError> {
        let segments = chunking::split(&task.content, task.style.as_deref(), &chunk_cfg);
        if segments.is_empty() {
            return Err(EngineError::InvalidInput("empty content".into()));
        }
        let chunks: Vec<Chunk> = segments
            .into_iter()
            .enumerate()
            .map(|(i, seg)| Chunk::new(task.id.clone(), (i + 1) as i32, seg.text, seg.token_estimate))
            .collect();
        let total = chunks.len() as i32;
        self.storage.insert_chunks(&chunks)?;
        self.storage
            .update_task_progress(&task.id.to_string(), total, 0, 0)?;
        self.storage.update_task_status(&task.id.to_string(), &TaskStatus::Queued)?;
        self.bus.publish(&DomainEvent::TaskStatusChanged {
            task_id: task.id.clone(),
            session_id: task.session_id.clone(),
            status: "queued".into(),
        });
        for c in &chunks {
            self.queue.push(task.priority, c.id.to_string(), task.id.to_string());
        }
        Ok(())
    }

    pub fn list_tasks(
        &self,
        page: i64,
        page_size: i64,
        status: Option<&str>,
        session_id: Option<&str>,
        search: Option<&str>,
    ) -> Result<(Vec<crate::storage::TaskRow>, i64), EngineError> {
        self.storage.tasks(page, page_size, status, session_id, search)
    }

    pub fn task(&self, id: &str) -> Result<Option<(Task, Vec<crate::storage::ChunkRow>)>, EngineError> {
        self.storage.task(id)
    }

    pub fn cancel_task(&self, id: &str) -> Result<(), EngineError> {
        let meta = self
            .storage
            .task_meta(id)?
            .ok_or_else(|| EngineError::NotFound(format!("task {id}")))?;
        if matches!(
            meta.status.as_str(),
            "done" | "failed" | "cancelled"
        ) {
            return Err(EngineError::InvalidInput(format!(
                "task {id} is {}",
                meta.status
            )));
        }
        self.storage.mark_task_cancelled(id)?;
        self.storage.cancel_pending_chunks(id)?;
        self.assemblers.write().remove(id);
        self.bus.publish(&DomainEvent::TaskStatusChanged {
            task_id: Id::from_str(id).unwrap_or_default(),
            session_id: meta.session_id.as_deref().and_then(|s| Id::from_str(s).ok()),
            status: "cancelled".into(),
        });
        Ok(())
    }

    pub fn retry_task(&self, id: &str) -> Result<(), EngineError> {
        let meta = self
            .storage
            .task_meta(id)?
            .ok_or_else(|| EngineError::NotFound(format!("task {id}")))?;
        if !matches!(meta.status.as_str(), "failed" | "cancelled") {
            return Err(EngineError::InvalidInput(format!(
                "task {id} is {} — only failed/cancelled tasks retry",
                meta.status
            )));
        }
        self.storage.reset_failed_chunks(id)?;
        self.storage.update_task_progress(id, 0, 0, 0)?;
        self.storage.set_task_error(id, "")?;
        self.storage.update_task_status(id, &TaskStatus::Queued)?;
        let priority = meta.priority;
        for c in self.storage.chunks(id)? {
            if c.status == "pending" {
                self.queue.push(priority, c.id, c.task_id);
            }
        }
        self.bus.publish(&DomainEvent::TaskStatusChanged {
            task_id: Id::from_str(id).unwrap_or_default(),
            session_id: meta.session_id.as_deref().and_then(|s| Id::from_str(s).ok()),
            status: "queued".into(),
        });
        Ok(())
    }

    pub fn delete_task(&self, id: &str) -> Result<(), EngineError> {
        let audio_files = self.storage.task_audio_files(id)?;
        let output = self.storage.delete_task(id)?;
        if let Some(path) = output {
            let _ = std::fs::remove_file(path);
        }
        for path in audio_files {
            if !path.is_empty() {
                let _ = std::fs::remove_file(path);
            }
        }
        self.assemblers.write().remove(id);
        Ok(())
    }

    pub fn task_audio_path(&self, id: &str) -> Result<Option<String>, EngineError> {
        self.storage.task_output_path(id)
    }

    // ── import (batch TXT) ───────────────────────────────────────────────

    pub fn import_files(
        &self,
        session_id: Option<String>,
        session_name: Option<String>,
        voice: &str,
        model: &str,
        style: Option<String>,
        provider_id: Option<String>,
        files: Vec<(String, Vec<u8>)>,
    ) -> Result<ImportResult, EngineError> {
        // Validate provider up front (umreview: silently orphaned tasks must fail loudly).
        if let Some(pid) = &provider_id {
            if self.storage.provider(pid)?.is_none() {
                return Err(EngineError::NotFound(format!("provider {pid}")));
            }
        }
        let session_id = match session_id {
            Some(sid) => sid,
            None => self
                .create_session(session_name.as_deref().unwrap_or("TXT 批量导入"))?
                .id
                .to_string(),
        };
        let mut created = 0usize;
        let mut rejected = Vec::new();
        let mut total_delta = 0i32;
        for (filename, bytes) in &files {
            let content = match decode_txt(bytes) {
                Ok(c) => c,
                Err(e) => {
                    rejected.push(format!("{filename}: {e}"));
                    continue;
                }
            };
            let task = Task::new(CreateTaskInput {
                session_id: Some(Id::from_str(&session_id).unwrap_or_default()),
                title: filename.clone(),
                content,
                voice: voice.to_string(),
                model: model.to_string(),
                style: style.clone(),
                priority: 0,
                provider_id: provider_id.clone(),
            });
            self.storage.insert_task(&task)?;
            match self.enqueue_task_inner(&task, self.cfg.chunk) {
                Ok(()) => {
                    created += 1;
                    total_delta += 1;
                }
                Err(e) => {
                    rejected.push(format!("{filename}: {e}"));
                    let _ = self.storage.delete_task(&task.id.to_string());
                }
            }
        }
        self.storage.add_session_total(&session_id, total_delta)?;
        self.bus.publish(&DomainEvent::SessionUpdated {
            session_id: Id::from_str(&session_id).unwrap_or_default(),
        });
        Ok(ImportResult {
            session_id,
            files_received: files.len(),
            tasks_created: created,
            rejected,
        })
    }

    pub fn session_outputs(&self, session_id: &str) -> Result<Vec<(String, String)>, EngineError> {
        self.storage.session_outputs(session_id)
    }

    // ── auth tokens ──────────────────────────────────────────────────────

    pub fn issue_token(&self, label: &str) -> Result<String, EngineError> {
        let mut bytes = [0u8; 32];
        use aes_gcm::aead::rand_core::RngCore;
        aes_gcm::aead::OsRng.fill_bytes(&mut bytes);
        let token = hex::encode(bytes);
        self.storage.store_token_hash(&hash_token(&token), label)?;
        Ok(token)
    }

    pub fn check_token(&self, token: &str) -> Result<bool, EngineError> {
        self.storage.token_exists(&hash_token(token))
    }

    /// True when at least one API token exists (used for first-run bootstrap).
    pub fn has_any_token(&self) -> Result<bool, EngineError> {
        Ok(self.storage.token_count()? > 0)
    }

    /// Issue a short-lived, scope-bound credential for URL use (umreview B).
    /// Scopes: `audio:{task_id}`, `events:{channel}`, `preview:{voice_id}`.
    pub fn issue_scoped_token(
        &self,
        scope: &str,
        ttl_secs: u64,
    ) -> Result<String, EngineError> {
        let valid = scope.starts_with("audio:")
            || scope.starts_with("events:")
            || scope.starts_with("preview:");
        if !valid {
            return Err(EngineError::InvalidInput(format!(
                "unknown scope prefix: {scope}"
            )));
        }
        Ok(mimotts_core::crypto::sign_scoped(
            &self.master_key,
            scope,
            ttl_secs.clamp(30, 900),
        ))
    }

    pub fn check_scoped(&self, token: &str, expected_scope: &str) -> bool {
        mimotts_core::crypto::verify_scoped(&self.master_key, token, expected_scope)
    }

    // ── runtime stats ────────────────────────────────────────────────────

    pub fn stats(&self) -> serde_json::Value {
        let providers = self
            .runtimes
            .read()
            .iter()
            .map(|(id, rt)| {
                serde_json::json!({
                    "provider_id": id,
                    "group": rt.group,
                    "window": rt.gate.window(),
                    "inflight": rt.gate.inflight(),
                    "open": rt.gate.is_open(),
                    "retry_after_secs": rt.gate.retry_after_secs(),
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "queue_depth": self.queue.len(),
            "workers": self.workers.load(Ordering::Acquire),
            "providers": providers,
        })
    }

    // ── internals ────────────────────────────────────────────────────────

    fn seed_queue(&self) -> Result<(), EngineError> {
        for (chunk_id, task_id, priority) in self.storage.pending_chunk_ids(5000)? {
            self.queue.push(priority, chunk_id, task_id);
        }
        Ok(())
    }

    fn start_workers(self: &Arc<Self>) {
        for i in 0..self.cfg.workers {
            let engine = self.clone();
            tokio::spawn(async move { engine.worker_loop(i).await });
        }
    }

    fn start_recovery(self: &Arc<Self>) {
        let engine = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                if engine.cancelled.load(Ordering::Acquire) {
                    break;
                }
                // 1. In-flight chunks stale >600s → reset to pending. The
                //    threshold must exceed the worst-case legitimate retry
                //    envelope (5 attempts × 120s stream stall + backoff);
                //    anything shorter re-claims chunks whose worker is still
                //    synthesizing → duplicate API spend.
                match engine.storage.reset_stale_inflight(600) {
                    Ok(n) if n > 0 => {
                        tracing::warn!("recovery: reset {n} stale inflight chunks");
                    }
                    Ok(_) => {}
                    Err(e) => tracing::error!("recovery scan failed: {e}"),
                }
                // 2. Always re-seed pending chunks (cheap, bounded): any chunk
                //    lost from the in-memory queue (panic / restart window)
                //    self-heals here.
                let _ = engine.seed_queue();
                // 3. Merge-slot recovery (umreview C2): a task that claimed
                //    `merging` but whose merge never ran gets re-resolved.
                match engine.storage.reset_stale_merging(300) {
                    Ok(ids) => {
                        for id in ids {
                            tracing::warn!("recovery: re-resolving stale merging task {id}");
                            let e = engine.clone();
                            tokio::spawn(async move {
                                let _ = e.on_chunk_resolved(&id).await;
                            });
                        }
                    }
                    Err(e) => tracing::error!("merge recovery scan failed: {e}"),
                }
            }
        });
    }

    async fn worker_loop(self: Arc<Self>, id: usize) {
        let mut idle_ms: u64 = 50;
        loop {
            if self.cancelled.load(Ordering::Acquire) {
                tracing::info!("worker {id} shutting down");
                break;
            }
            let Some((chunk_id, task_id)) = self.queue.pop() else {
                idle_ms = (idle_ms * 2).min(500);
                tokio::select! {
                    _ = self.queue.notify.notified() => { idle_ms = 50; }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(idle_ms)) => {}
                }
                continue;
            };
            idle_ms = 50;
            // Panic guard: a panicking chunk must never silently lose its row
            // state — surface it as a failed chunk instead.
            let engine = self.clone();
            let chunk_id_owned = chunk_id.clone();
            let task_id_owned = task_id.clone();
            let handle = tokio::spawn(async move {
                engine.process_chunk(&chunk_id_owned, &task_id_owned).await
            });
            let result = match handle.await {
                Ok(r) => r,
                Err(join_err) => {
                    tracing::error!("worker {id}: chunk {chunk_id} panicked: {join_err}");
                    Err(EngineError::Internal(format!("worker panic: {join_err}")))
                }
            };
            if let Err(e) = result {
                tracing::error!("worker {id}: chunk {chunk_id} failed: {e}");
                let _ = self.storage.fail_chunk(&chunk_id, &e.to_string());
                let _ = self.on_chunk_resolved(&task_id).await;
            }
        }
    }

    async fn process_chunk(&self, chunk_id: &str, task_id: &str) -> Result<(), EngineError> {
        // 1. task state
        let meta = self
            .storage
            .task_meta(task_id)?
            .ok_or_else(|| EngineError::NotFound(format!("task {task_id}")))?;
        match meta.status.as_str() {
            "cancelled" | "failed" => {
                return Err(EngineError::InvalidInput(format!(
                    "parent task {}",
                    meta.status
                )));
            }
            "done" => return Ok(()), // merged already
            _ => {}
        }

        let chunk = self
            .storage
            .chunk_row(chunk_id)?
            .ok_or_else(|| EngineError::NotFound(format!("chunk {chunk_id}")))?;
        if chunk.status != "pending" {
            return Ok(()); // double dispatch guard
        }

        // 2. provider + runtime
        let provider = self
            .resolve_provider(meta.provider_id.as_deref())?
            .ok_or_else(|| EngineError::NoProvider)?;
        if !provider.is_configured || provider.api_key_sealed.is_empty() {
            // No credentials yet: do NOT fail the chunk — wait for a key.
            // (A restart with an unconfigured provider must never destroy
            // resumable tasks; the chunk stays `pending` in the queue.)
            self.queue
                .push(meta.priority, chunk_id.to_string(), task_id.to_string());
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            return Ok(());
        }
        let api_key = self
            .crypto
            .open(&provider.api_key_sealed)
            .ok_or_else(|| EngineError::Unauthorized("master key mismatch — provider secret unreadable".into()))?;

        let gate = self.ensure_runtime(&provider).await;
        let permit = gate.acquire().await;

        let budget = self.budget_for(&provider.budget_group);
        let tokens = chunk.token_estimate.max(1) as u64;
        if !budget.reserve(tokens) {
            drop(permit);
            // Re-queue at the task's own priority and back off: without the
            // longer sleep, 32 workers hammer pop→reserve-fail→push at ~10 Hz
            // each (SQLite churn for zero progress).
            self.queue.push(meta.priority, chunk_id.to_string(), task_id.to_string());
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            return Ok(());
        }

        // 3. optimistic claim
        if !self.storage.claim_chunk(chunk_id)? {
            budget.refund(tokens);
            drop(permit);
            return Ok(());
        }

        // 4. task transition
        if meta.status == "queued" || meta.status == "pending" {
            self.storage
                .update_task_status(task_id, &TaskStatus::Synthesizing)?;
            self.bus.publish(&DomainEvent::TaskStatusChanged {
                task_id: Id::from_str(task_id).unwrap_or_default(),
                session_id: meta.session_id.as_deref().and_then(|s| Id::from_str(s).ok()),
                status: "synthesizing".into(),
            });
        }

        // 5. synthesize with ADR-013 routing
        let voice = if meta.voice.starts_with("data:") {
            VoiceSpec::CloneDataUri(meta.voice.clone())
        } else {
            VoiceSpec::Preset(meta.voice.clone())
        };
        let format = if self.cfg.stream_audio { "pcm16" } else { "wav" }.to_string();
        let pcm_path = self
            .cfg
            .data_dir
            .join("chunks")
            .join(format!("{chunk_id}.pcm"));
        let mut req = SynthesisRequest {
            model: meta.model.clone(),
            style: meta.style.clone(),
            text: chunk.text.clone(),
            voice,
            format,
            stream: self.cfg.stream_audio,
            optimize_text_preview: false,
            api_key: api_key.clone(),
            base_url: provider.base_url.clone(),
        };

        let was_open = gate.is_open();
        let result: Result<(u64, u64), EngineError> = if self.cfg.stream_audio {
            // Stream the audio straight into the chunk file: memory stays
            // O(1) per in-flight chunk (huge chunks × high concurrency
            // would otherwise buffer hundreds of MB of PCM).
            let n = self
                .stream_to_file_with_retries(&mut req, &gate, &pcm_path)
                .await?;
            Ok((n, audio::pcm16_duration_ms(n as usize)))
        } else {
            let (pcm, duration_ms) = self.synth_with_retries(&mut req, &gate).await?;
            tokio::fs::write(&pcm_path, &pcm).await?;
            Ok((pcm.len() as u64, duration_ms))
        };

        match result {
            Ok((_bytes, duration_ms)) => {
                // Append into the shared raw stream and record the byte
                // range, then reclaim the per-chunk file immediately —
                // audio is never stored twice.
                let (audio_path, range) = match self.ensure_assembler(task_id) {
                    Some(asm) => match asm.append(&pcm_path) {
                        Ok((offset, len)) => {
                            let _ = tokio::fs::remove_file(&pcm_path).await;
                            (
                                asm.raw_path().to_string_lossy().into_owned(),
                                Some((offset as i64, len as i64)),
                            )
                        }
                        Err(e) => {
                            // Fall back to whole-file ownership; finalize
                            // handles both layouts.
                            tracing::error!("task {task_id} assembler append failed: {e}");
                            (pcm_path.to_string_lossy().into_owned(), None)
                        }
                    },
                    None => (pcm_path.to_string_lossy().into_owned(), None),
                };
                // Guarded finish (umreview C3): cancelled mid-flight → no events.
                let finished = self.storage.finish_chunk(
                    chunk_id,
                    &audio_path,
                    range,
                    duration_ms as i64,
                )?;
                if !finished && range.is_none() {
                    // Whole-file layout: drop the orphan PCM. Range layout:
                    // the shared stream must survive (other chunks use it).
                    let _ = tokio::fs::remove_file(&audio_path).await;
                }
                gate.on_success();
                if was_open {
                    self.bus.publish(&DomainEvent::ProviderHealth {
                        provider_id: provider.id.clone(),
                        state: "closed".into(),
                        retry_after_secs: None,
                    });
                }
                if finished {
                    self.bus.publish(&DomainEvent::ChunkCompleted {
                        chunk_id: Id::from_str(chunk_id).unwrap_or_default(),
                        task_id: Id::from_str(task_id).unwrap_or_default(),
                        seq: chunk.seq,
                        audio_path,
                        duration_ms: duration_ms as i64,
                    });
                }
                budget.refund(tokens);
                self.on_chunk_resolved(task_id).await?;
                Ok(())
            }
            Err(e) => {
                budget.refund(tokens);
                match &e {
                    EngineError::RateLimited => {
                        if gate.on_throttle(true) {
                            self.bus.publish(&DomainEvent::ProviderHealth {
                                provider_id: provider.id.clone(),
                                state: "open".into(),
                                retry_after_secs: gate.retry_after_secs(),
                            });
                        }
                        Err(e)
                    }
                    EngineError::ServerOverload(_) => {
                        gate.on_server_error();
                        Err(e)
                    }
                    EngineError::ContextOverflow(_) => {
                        // ADR-013: re-chunk whole task at ×0.8, this chunk is obsolete.
                        tracing::warn!("chunk {chunk_id} overflowed context — re-chunking task {task_id}");
                        let _ = self.rechunk_task(task_id).await;
                        Err(e)
                    }
                    EngineError::ContentBlocked => {
                        let msg = "内容被安全审核拦截（421），已跳过该分片".to_string();
                        self.storage.fail_chunk(chunk_id, &msg)?;
                        self.bus.publish(&DomainEvent::ChunkFailed {
                            chunk_id: Id::from_str(chunk_id).unwrap_or_default(),
                            task_id: Id::from_str(task_id).unwrap_or_default(),
                            seq: chunk.seq,
                            error: msg,
                        });
                        self.on_chunk_resolved(task_id).await?;
                        Ok(())
                    }
                    _ => Err(e),
                }
            }
        }
    }

    /// Non-streaming (wav) synthesis with bounded retries → raw PCM + duration.
    async fn synth_with_retries(
        &self,
        req: &mut SynthesisRequest,
        gate: &Arc<AimdGate>,
    ) -> Result<(Vec<u8>, u64), EngineError> {
        const MAX_ATTEMPTS: u32 = 5;
        let mut attempt = 0u32;
        loop {
            let result = self.client.synthesize_once(req).await;
            match result {
                Ok(bytes) => {
                    let (pcm, duration_ms) = normalize_audio(&bytes, false)?;
                    return Ok((pcm, duration_ms));
                }
                Err(e) if e.is_retryable() && attempt < MAX_ATTEMPTS => {
                    attempt += 1;
                    retry_wait(&e, attempt, gate).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Streaming synthesis written DIRECTLY to `path` (file truncated per
    /// attempt) — O(1) memory per in-flight chunk, unlike buffering the whole
    /// PCM. Returns the total raw PCM byte count.
    async fn stream_to_file_with_retries(
        &self,
        req: &mut SynthesisRequest,
        gate: &Arc<AimdGate>,
        path: &Path,
    ) -> Result<u64, EngineError> {
        const MAX_ATTEMPTS: u32 = 5;
        let mut attempt = 0u32;
        loop {
            let mut file = tokio::fs::File::create(path).await?;
            let mut stream = match self.client.synthesize_stream(req).await {
                Ok(s) => s,
                Err(e) if e.is_retryable() && attempt < MAX_ATTEMPTS => {
                    attempt += 1;
                    retry_wait(&e, attempt, gate).await;
                    continue;
                }
                Err(e) => return Err(e),
            };
            let mut written = 0u64;
            let outcome = loop {
                let item =
                    match tokio::time::timeout(std::time::Duration::from_secs(120), stream.next())
                        .await
                    {
                        Ok(Some(item)) => item,
                        Ok(None) => {
                            break Err(EngineError::ServerOverload(
                                "stream ended without [DONE]".into(),
                            ))
                        }
                        Err(_) => {
                            break Err(EngineError::ServerOverload("stream stalled 120s".into()))
                        }
                    };
                match item {
                    Ok(AudioChunk::Bytes(b)) => {
                        use tokio::io::AsyncWriteExt;
                        if let Err(e) = file.write_all(&b).await {
                            break Err(EngineError::Internal(format!("pcm write: {e}")));
                        }
                        written += b.len() as u64;
                    }
                    Ok(AudioChunk::Done) => break Ok(()),
                    Err(e) => break Err(e),
                }
            };
            match outcome {
                Ok(()) => {
                    use tokio::io::AsyncWriteExt;
                    file.flush().await?;
                    return Ok(written);
                }
                Err(e) if e.is_retryable() && attempt < MAX_ATTEMPTS => {
                    attempt += 1;
                    retry_wait(&e, attempt, gate).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn on_chunk_resolved(&self, task_id: &str) -> Result<(), EngineError> {
        // Cancel race guard (umreview C3): a cancelled/failed/done task must
        // never be resurrected by late chunk events.
        let meta = self.storage.task_meta(task_id)?;
        let Some(meta) = meta else {
            return Ok(());
        };
        if matches!(
            meta.status.as_str(),
            "cancelled" | "failed" | "done" | "merging"
        ) {
            return Ok(());
        }
        let (total, done, failed, active) = self.storage.chunk_stats(task_id)?;
        self.storage
            .update_task_progress(task_id, total as i32, done as i32, failed as i32)?;
        if active > 0 || total == 0 {
            return Ok(());
        }
        if done == 0 {
            // Atomic terminal claim (all-failed): two workers resolving the
            // last two chunks concurrently must not BOTH fail the task and
            // double-decrement the session counter.
            if !self.storage.claim_task_failed(task_id)? {
                return Ok(());
            }
            self.storage
                .set_task_error(task_id, &format!("all {total} chunks failed"))?;
            self.emit_task_failed(task_id, &format!("all {total} chunks failed"))
                .await?;
            return Ok(());
        }
        // Merge claim (umreview C2): exactly one resolver wins the transition
        // into `merging`; concurrent losers return without touching files.
        if !self.storage.claim_merge(task_id)? {
            return Ok(());
        }
        let storage = self.storage.clone();
        let task_id_owned = task_id.to_string();
        let output_dir = self.cfg.output_dir.clone();
        let asm = self.assemblers.read().get(task_id).cloned();
        let merge_result = tokio::task::spawn_blocking(move || {
            finalize_task_audio(&storage, &task_id_owned, &output_dir, asm)
        })
        .await
        .map_err(|e| EngineError::Internal(format!("merge join: {e}")))?;
        // Finalize consumed the live assembly stream (success or failure).
        self.assemblers.write().remove(task_id);
        match merge_result {
            Ok((path, duration_ms)) => {
                self.storage.set_task_output(task_id, &path, duration_ms as i64)?;
                self.storage.update_task_status(task_id, &TaskStatus::Done)?;
                let meta = self.storage.task_meta(task_id)?;
                self.bus.publish(&DomainEvent::TaskCompleted {
                    task_id: Id::from_str(task_id).unwrap_or_default(),
                    session_id: meta
                        .as_ref()
                        .and_then(|m| m.session_id.as_deref())
                        .and_then(|s| Id::from_str(s).ok()),
                    output_path: path,
                    duration_ms: duration_ms as i64,
                });
                if let Some(meta) = meta {
                    if let Some(sid) = meta.session_id {
                        let _ = self.storage.update_session_progress(&sid, 1, 0);
                        self.bus.publish(&DomainEvent::SessionUpdated {
                            session_id: Id::from_str(&sid).unwrap_or_default(),
                        });
                    }
                }
            }
            Err(e) => {
                self.storage.update_task_status(task_id, &TaskStatus::Failed)?;
                self.storage.set_task_error(task_id, &e.to_string())?;
                self.emit_task_failed(task_id, &e.to_string()).await?;
            }
        }
        Ok(())
    }

    async fn emit_task_failed(&self, task_id: &str, error: &str) -> Result<(), EngineError> {
        let meta = self.storage.task_meta(task_id)?;
        self.bus.publish(&DomainEvent::TaskFailed {
            task_id: Id::from_str(task_id).unwrap_or_default(),
            session_id: meta
                .as_ref()
                .and_then(|m| m.session_id.as_deref())
                .and_then(|s| Id::from_str(s).ok()),
            error: error.into(),
        });
        if let Some(meta) = meta {
            if let Some(sid) = meta.session_id {
                let _ = self.storage.update_session_progress(&sid, 0, 1);
                self.bus.publish(&DomainEvent::SessionUpdated {
                    session_id: Id::from_str(&sid).unwrap_or_default(),
                });
            }
        }
        Ok(())
    }

    async fn rechunk_task(&self, task_id: &str) -> Result<(), EngineError> {
        let meta = self
            .storage
            .task_meta(task_id)?
            .ok_or_else(|| EngineError::NotFound(format!("task {task_id}")))?;
        // Cumulative scale: each overflow shrinks the budget by another ×0.8;
        // after MAX_RECHUNK_DEPTH the task fails instead of re-chunking
        // forever.
        const MAX_RECHUNK_DEPTH: u32 = 3;
        let depth = {
            let mut map = self.rechunk_depth.lock();
            let d = map.entry(task_id.to_string()).or_insert(0);
            *d += 1;
            *d
        };
        if depth > MAX_RECHUNK_DEPTH {
            self.storage.update_task_status(task_id, &TaskStatus::Failed)?;
            self.storage.set_task_error(
                task_id,
                "context overflow persists after repeated re-chunking",
            )?;
            self.emit_task_failed(task_id, "context overflow persists after repeated re-chunking")
                .await?;
            return Ok(());
        }
        // Re-chunk invalidates EVERY existing chunk — including `done` ones:
        // their audio was synthesized for a stale (larger) budget text and
        // must never merge in, otherwise the output repeats/overlaps segments.
        // In-flight chunks become no-ops (rows gone → guarded finish affects
        // 0 rows) and their orphan PCM is dropped by the worker.
        let stale_files = self.storage.reset_all_chunks(task_id)?;
        for f in stale_files {
            let _ = std::fs::remove_file(f);
        }
        self.assemblers.write().remove(task_id);
        let base_seq = self.storage.max_chunk_seq(task_id)?;
        let task = Task {
            id: Id::from_str(task_id).unwrap_or_default(),
            session_id: meta.session_id.as_deref().and_then(|s| Id::from_str(s).ok()),
            title: meta.title,
            content: meta.content,
            voice: meta.voice,
            model: meta.model,
            style: meta.style,
            status: TaskStatus::Synthesizing,
            priority: meta.priority,
            total_chars: 0,
            total_tokens: 0,
            total_chunks: 0,
            done_chunks: 0,
            failed_chunks: 0,
            output_path: None,
            duration_ms: None,
            provider_id: meta.provider_id,
            error: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
        };
        let segments =
            chunking::rechunk_at_depth(&task.content, task.style.as_deref(), &self.cfg.chunk, depth);
        let chunks: Vec<Chunk> = segments
            .into_iter()
            .enumerate()
            .map(|(i, seg)| {
                Chunk::new(
                    task.id.clone(),
                    base_seq + (i + 1) as i32,
                    seg.text,
                    seg.token_estimate,
                )
            })
            .collect();
        self.storage.insert_chunks(&chunks)?;
        // Progress reflects the full surviving set (in-flight + fresh).
        let (total, done, failed, _active) = self.storage.chunk_stats(task_id)?;
        self.storage.update_task_progress(
            task_id,
            total as i32,
            done as i32,
            failed as i32,
        )?;
        for c in &chunks {
            self.queue.push(meta.priority, c.id.to_string(), task_id.to_string());
        }
        tracing::info!(
            "task {task_id} re-chunked (depth={depth}) into {} segments",
            chunks.len()
        );
        Ok(())
    }

    async fn ensure_runtime(&self, provider: &crate::storage::ProviderRow) -> Arc<AimdGate> {
        if let Some(rt) = self.runtimes.read().get(&provider.id) {
            return rt.gate.clone();
        }
        let gate = AimdGate::new(AimdGateConfig {
            max_window: self.cfg.max_window,
            ..Default::default()
        });
        self.runtimes.write().insert(
            provider.id.clone(),
            ProviderRuntime {
                gate: gate.clone(),
                group: provider.budget_group.clone(),
            },
        );
        gate
    }

    fn budget_for(&self, group: &str) -> Arc<BudgetGroup> {
        if let Some(b) = self.budgets.read().get(group) {
            return b.clone();
        }
        let b = Arc::new(BudgetGroup {
            rpm: TokenBucket::new(self.cfg.rpm_headroom, self.cfg.rpm_headroom),
            tpm: TokenBucket::new(self.cfg.tpm_budget, self.cfg.tpm_budget),
        });
        self.budgets.write().insert(group.to_string(), b.clone());
        b
    }

    /// Get-or-create the live assembler for a task. Returns None once the
    /// task has moved past the synthesis phase (finalize/terminal) — stray
    /// completions then never touch the raw stream.
    fn ensure_assembler(&self, task_id: &str) -> Option<Arc<Assembler>> {
        if let Ok(Some(meta)) = self.storage.task_meta(task_id) {
            if !matches!(
                meta.status.as_str(),
                "pending" | "queued" | "synthesizing"
            ) {
                return None;
            }
        }
        let mut map = self.assemblers.write();
        if let Some(a) = map.get(task_id) {
            return Some(a.clone());
        }
        // Never truncate: ranges already recorded in the DB may address the
        // existing stream; stale tails beyond those ranges are never read.
        let raw_path = self.cfg.output_dir.join(format!("{task_id}.pcm.tmp"));
        let asm = Arc::new(Assembler::new(raw_path));
        map.insert(task_id.to_string(), asm.clone());
        Some(asm)
    }
}

// ── free functions ───────────────────────────────────────────────────────

fn load_or_create_master_key(data_dir: &Path) -> Result<MasterKey, EngineError> {
    let path = data_dir.join("master.key");
    if path.exists() {
        // Verify existing key is not world-readable (umreview: never loosen).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path)?.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                return Err(EngineError::Internal(format!(
                    "master.key permissions too open ({mode:o}); chmod 600 {}",
                    path.display()
                )));
            }
        }
        let hex = std::fs::read_to_string(&path)?;
        return MasterKey::from_hex(hex.trim())
            .ok_or_else(|| EngineError::Internal("master.key is corrupt".into()));
    }
    let key = MasterKey::generate();
    // Create with restrictive permissions from the very first byte
    // (no 0644 exposure window), then fail loudly if tightening fails.
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        f.write_all(key.to_hex().as_bytes())?;
        f.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        // Windows: no std mode control; the data dir lives under the user
        // profile. Reject if the file already exists (create_new race guard).
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        f.write_all(key.to_hex().as_bytes())?;
        f.sync_all()?;
    }
    tracing::info!("generated master key at {}", path.display());
    Ok(key)
}

/// Bounded retry backoff + gate feedback: transient 5xx/network hiccups
/// retry fast; 429 stays short (the circuit gate handles long pauses).
async fn retry_wait(e: &EngineError, attempt: u32, gate: &Arc<AimdGate>) {
    let base_ms = match e {
        EngineError::RateLimited => (500u64 << attempt.min(3)).min(4000),
        _ => (500u64 << attempt.min(3)).min(8000),
    };
    let wait = base_ms + fastrand::u64(..base_ms / 2 + 1);
    tracing::warn!("synth retry {attempt} after ({e}) — wait {wait}ms");
    tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
    if matches!(e, EngineError::RateLimited) {
        gate.on_throttle(true);
    } else {
        gate.on_server_error();
    }
}

/// Normalize API audio bytes to raw PCM16LE mono + duration.
/// Streaming pcm16 arrives raw; non-streaming wav needs its data range.
fn normalize_audio(bytes: &[u8], was_stream: bool) -> Result<(Vec<u8>, u64), EngineError> {
    if was_stream {
        let pcm = bytes.to_vec();
        let dur = audio::pcm16_duration_ms(pcm.len());
        return Ok((pcm, dur));
    }
    match audio::find_wav_data_range(bytes) {
        Some((off, len)) => {
            let pcm = bytes[off..off + len].to_vec();
            let dur = audio::pcm16_duration_ms(pcm.len());
            Ok((pcm, dur))
        }
        None => {
            // mp3 or unknown: store as-is (treated as opaque); duration unknown → 0
            let dur = 0u64;
            Ok((bytes.to_vec(), dur))
        }
    }
}

/// Finalize a task's audio into a WAV file — fully streaming, O(1) memory
/// (64KB buffers), never loads a whole task's PCM into RAM.
///
/// Done chunks are replayed in seq order from the shared raw stream via
/// their recorded byte ranges (legacy whole-file rows stream the file
/// directly), so the output is always in order no matter the completion
/// order. This is also the crash-resume path: ranges live in the DB and the
/// stream survives on disk across restarts.
///
/// Audio files are reclaimed afterwards (they previously accumulated for
/// the life of the task).
fn finalize_task_audio(
    storage: &Storage,
    task_id: &str,
    output_dir: &Path,
    assembler: Option<Arc<Assembler>>,
) -> Result<(String, u64), EngineError> {
    let parts = storage.chunk_audio_ranges(task_id)?;
    if parts.is_empty() {
        return Err(EngineError::InvalidInput("no done chunks to merge".into()));
    }
    // Stop live appends before touching files (stray workers become no-ops).
    if let Some(a) = &assembler {
        a.finish();
    }
    let out_path = output_dir.join(format!("{task_id}.wav"));
    let mut total = 0u64;
    {
        use std::io::{Read, Seek, SeekFrom, Write};
        let mut out = std::io::BufWriter::with_capacity(
            64 * 1024,
            std::fs::File::create(&out_path)?,
        );
        // Placeholder header; exact sizes are patched below.
        out.write_all(&audio::wav_header(0))?;
        for (_seq, path, offset, len, _dur) in &parts {
            let mut f = std::fs::File::open(path)?;
            match (offset, len) {
                (Some(o), Some(l)) => {
                    f.seek(SeekFrom::Start(*o as u64))?;
                    total += std::io::copy(&mut f.take(*l as u64), &mut out)?;
                }
                // Legacy whole-file rows (pre-stream layout).
                _ => total += std::io::copy(&mut f, &mut out)?,
            }
        }
        out.flush()?;
    }
    if total == 0 {
        let _ = std::fs::remove_file(&out_path);
        return Err(EngineError::InvalidInput("assembled audio is empty".into()));
    }
    // Patch exact WAV sizes (byte-exact duration).
    {
        use std::io::{Seek, SeekFrom, Write};
        let mut f = std::fs::OpenOptions::new().write(true).open(&out_path)?;
        f.seek(SeekFrom::Start(0))?;
        f.write_all(&audio::wav_header(total as u32))?;
        f.flush()?;
    }
    // Reclaim the shared stream and any legacy chunk files.
    for path in storage.task_audio_files(task_id)? {
        let _ = std::fs::remove_file(&path);
    }
    let _ = std::fs::remove_file(output_dir.join(format!("{task_id}.pcm.tmp")));
    let duration_ms = audio::pcm16_duration_ms(total as usize);
    Ok((out_path.to_string_lossy().to_string(), duration_ms))
}

/// UTF-8 strict → GB18030 fallback (Chinese TXT files are frequently GBK).
fn decode_txt(bytes: &[u8]) -> Result<String, String> {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Ok(s.to_string());
    }
    let (text, _, had_errors) = encoding_rs::GB18030.decode(bytes);
    if had_errors {
        Err("无法识别的编码（仅支持 UTF-8 / GB18030）".into())
    } else {
        Ok(text.into_owned())
    }
}

pub struct EngineConfigSnapshot {
    pub chunk_target_tokens: i64,
    pub chunk_hard_cap_tokens: i64,
    pub context_window_tokens: i64,
    pub workers: usize,
    pub stream_audio: bool,
    pub announcement: Option<String>,
}

/// Local tuning overrides (headless / ops): `MIMOTTS_RPM_HEADROOM`,
/// `MIMOTTS_TPM_BUDGET`, `MIMOTTS_WORKERS`, `MIMOTTS_CHUNK_TARGET`,
/// `MIMOTTS_CHUNK_HARD_CAP`. Invalid values are ignored (never fatal).
pub fn apply_env_overrides(cfg: &mut EngineConfig) {
    if let Some(v) = env_u64("MIMOTTS_RPM_HEADROOM") {
        cfg.rpm_headroom = v;
    }
    if let Some(v) = env_u64("MIMOTTS_TPM_BUDGET") {
        cfg.tpm_budget = v;
    }
    if let Some(v) = env_u64("MIMOTTS_WORKERS") {
        cfg.workers = v.max(1) as usize;
    }
    if let Some(v) = env_i64("MIMOTTS_CHUNK_TARGET") {
        cfg.chunk.target_tokens = v;
    }
    if let Some(v) = env_i64("MIMOTTS_CHUNK_HARD_CAP") {
        cfg.chunk.hard_cap_tokens = v;
    }
}

fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok().and_then(|v| v.trim().parse().ok())
}

fn env_i64(key: &str) -> Option<i64> {
    std::env::var(key).ok().and_then(|v| v.trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn assembler_appends_in_arrival_order_and_returns_ranges() {
        let dir = std::env::temp_dir().join(format!("asm-test-{}", fastrand::u64(..)));
        std::fs::create_dir_all(&dir).unwrap();
        let raw = dir.join("task.pcm.tmp");
        std::fs::write(&raw, []).unwrap();
        let asm = Assembler::new(raw.clone());
        let (off1, len1) = asm.append(&tmp_file(&dir, "c1.pcm", b"AAA")).unwrap();
        assert_eq!((off1, len1), (0, 3));
        let (off2, len2) = asm.append(&tmp_file(&dir, "c2.pcm", b"BBBB")).unwrap();
        assert_eq!((off2, len2), (3, 4));
        assert_eq!(std::fs::read(&raw).unwrap(), b"AAABBBB");
        // Finalize stops new appends (stray workers no-op).
        asm.finish();
        let (_off3, len3) = asm.append(&tmp_file(&dir, "c3.pcm", b"C")).unwrap();
        assert_eq!(len3, 0);
        assert_eq!(std::fs::read(&raw).unwrap(), b"AAABBBB");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn assembler_resumes_at_existing_stream_size() {
        // Post-restart: the raw stream already holds recorded ranges.
        let dir = std::env::temp_dir().join(format!("asm-test-{}", fastrand::u64(..)));
        std::fs::create_dir_all(&dir).unwrap();
        let raw = dir.join("task.pcm.tmp");
        std::fs::write(&raw, b"PREVIOUS").unwrap();
        let asm = Assembler::new(raw.clone());
        let (off, len) = asm.append(&tmp_file(&dir, "c2.pcm", b"NEW")).unwrap();
        assert_eq!((off, len), (8, 3));
        assert_eq!(std::fs::read(&raw).unwrap(), b"PREVIOUSNEW");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn queue_dedups_recovery_reseeds() {
        let q = Queue::new();
        // Recovery re-seeds pending chunks every 30s; duplicates must no-op.
        q.push(0, "c1".into(), "t1".into());
        q.push(0, "c1".into(), "t1".into());
        q.push(5, "c2".into(), "t1".into());
        assert_eq!(q.len(), 2, "duplicate push must not grow the queue");
        // Higher priority pops first.
        assert_eq!(q.pop(), Some(("c2".to_string(), "t1".to_string())));
        assert_eq!(q.pop(), Some(("c1".to_string(), "t1".to_string())));
        assert_eq!(q.len(), 0);
        // A popped chunk may be re-queued (budget wait) exactly once.
        q.push(0, "c1".into(), "t1".into());
        assert_eq!(q.len(), 1);
    }
}
