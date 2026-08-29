//! UM-MimoTTS v4 CLI — headless-first (ADR-009).
//!
//! ```text
//! mimotts serve [--headless] [--port 30231] [--bind 127.0.0.1]
//! mimotts run --txt a.txt --txt dir/ --voice 冰糖 --out out/
//! mimotts run --session <id> --out out/     # resume a crashed/interrupted run
//! mimotts key issue [--label ui]
//! mimotts migrate --legacy-db backend/task_texts.db
//! ```

use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod serve;

#[derive(Parser)]
#[command(name = "mimotts", version, about = "MiMo-TTS automation workbench v4")]
struct Cli {
    /// Data directory (master.key / chunk audio / cache)
    #[arg(long, global = true, default_value = "data")]
    data_dir: PathBuf,
    /// SQLite database path
    #[arg(long, global = true, default_value = "data/mimo.db")]
    db: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start HTTP server (WebUI + API; --headless serves API only)
    Serve {
        #[arg(long, default_value_t = 30231)]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
        #[arg(long)]
        headless: bool,
        /// WebUI static directory (defaults to ../apps/web/dist)
        #[arg(long)]
        ui_dist: Option<PathBuf>,
        /// Synthesis worker count
        #[arg(long, default_value_t = 32)]
        workers: usize,
        /// Disable streaming synthesis (fall back to non-streaming wav)
        #[arg(long)]
        no_stream: bool,
    },
    /// Headless batch synthesis (no UI)
    Run {
        /// TXT file or directory (repeatable)
        #[arg(short = 't', long = "txt")]
        txt: Vec<PathBuf>,
        /// Resume monitoring an existing session (no re-import)
        #[arg(long)]
        session: Option<String>,
        #[arg(long, default_value = "mimo_default")]
        voice: String,
        #[arg(long, default_value = "mimo-v2.5-tts")]
        model: String,
        #[arg(long)]
        style: Option<String>,
        /// Output directory
        #[arg(long, default_value = "data/output")]
        out: PathBuf,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long, default_value_t = 32)]
        workers: usize,
        /// Machine-readable progress (one JSON object per line)
        #[arg(long)]
        json: bool,
    },
    /// API token management
    Key {
        #[command(subcommand)]
        sub: KeyCmd,
    },
    /// Import tasks from a legacy v3 database
    Migrate {
        #[arg(long)]
        legacy_db: PathBuf,
    },
}

#[derive(Subcommand)]
enum KeyCmd {
    /// Issue a new API token (hashed at rest; plaintext shown once)
    Issue {
        #[arg(long, default_value = "default")]
        label: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    // Shared env: `.env`, then local-only `.env.local` (gitignored, never
    // committed). dotenvy never overrides real process env.
    let _ = dotenvy::dotenv();
    let _ = dotenvy::from_filename(".env.local");
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    match cli.cmd {
        Cmd::Serve {
            port,
            bind,
            headless,
            ui_dist,
            workers,
            no_stream,
        } => {
            if let Err(e) = serve::run(serve::ServeOptions {
                data_dir: cli.data_dir,
                db: cli.db,
                port,
                bind,
                headless,
                ui_dist,
                workers,
                stream_audio: !no_stream,
            })
            .await
            {
                tracing::error!("serve failed: {e}");
                std::process::exit(1);
            }
        }
        Cmd::Run {
            txt,
            session,
            voice,
            model,
            style,
            out,
            provider,
            workers,
            json,
        } => {
            if let Err(e) = run_batch(
                cli.data_dir,
                cli.db,
                txt,
                session,
                voice,
                model,
                style,
                out,
                provider,
                workers,
                json,
            )
            .await
            {
                tracing::error!("run failed: {e}");
                std::process::exit(1);
            }
        }
        Cmd::Key { sub } => match sub {
            KeyCmd::Issue { label } => {
                if let Err(e) = issue_token(cli.data_dir, cli.db, label).await {
                    tracing::error!("key issue failed: {e}");
                    std::process::exit(1);
                }
            }
        },
        Cmd::Migrate { legacy_db } => {
            if let Err(e) = migrate(cli.data_dir, cli.db, legacy_db).await {
                tracing::error!("migrate failed: {e}");
                std::process::exit(1);
            }
        }
    }
}

async fn issue_token(
    data_dir: PathBuf,
    db: PathBuf,
    label: String,
) -> Result<(), mimotts_engine::EngineError> {
    let cfg = mimotts_engine::EngineConfig {
        data_dir: data_dir.clone(),
        db_path: db.to_string_lossy().to_string(),
        ..Default::default()
    };
    let engine = mimotts_engine::Engine::open(cfg)?;
    let token = engine.issue_token(&label)?;
    println!("╔══════════════════════════════════════════════════╗");
    println!("║  API Token（仅显示一次，请妥善保存）              ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!("{token}");
    println!("");
    println!("在 WebUI 设置页粘贴该 token 即可访问 API。");
    Ok(())
}

async fn migrate(
    data_dir: PathBuf,
    db: PathBuf,
    legacy_db: PathBuf,
) -> Result<(), mimotts_engine::EngineError> {
    let cfg = mimotts_engine::EngineConfig {
        data_dir,
        db_path: db.to_string_lossy().to_string(),
        ..Default::default()
    };
    let engine = mimotts_engine::Engine::open(cfg)?;
    let legacy = r2d2::Pool::builder()
        .max_size(1)
        .build(r2d2_sqlite::SqliteConnectionManager::file(&legacy_db))
        .map_err(|e| mimotts_engine::EngineError::Internal(e.to_string()))?;
    let conn = legacy
        .get()
        .map_err(|e| mimotts_engine::EngineError::Internal(e.to_string()))?;
    // Attach legacy DB and import via SQL (best-effort).
    let _ = conn
        .execute_batch("PRAGMA journal_mode=WAL;")
        .map_err(|e| mimotts_engine::EngineError::Internal(e.to_string()))?;
    // Use the storage migration helper through the engine's pool instead.
    let imported = {
        let pool_conn = engine.storage.pool.get()?;
        conn.execute(
            "ATTACH DATABASE ?1 AS legacy",
            rusqlite::params![legacy_db.to_string_lossy()],
        )
        .map_err(|e| mimotts_engine::EngineError::Internal(e.to_string()))?;
        let rows: Vec<(String, String, String, String, String, String)> = {
            let mut stmt = conn
                .prepare("SELECT id, title, content, voice, model, status FROM legacy.tasks")
                .map_err(|e| mimotts_engine::EngineError::Internal(e.to_string()))?;
            let mapped = stmt
                .query_map([], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
                })
                .map_err(|e| mimotts_engine::EngineError::Internal(e.to_string()))?;
            mapped.filter_map(|r| r.ok()).collect()
        };
        let mut n = 0usize;
        let now = chrono::Utc::now().to_rfc3339();
        for (id, title, content, voice, model, raw_status) in rows {
            let status = normalize_v3_status(&raw_status);
            if status == "cancelled" {
                continue;
            }
            // Count only real inserts (INSERT OR IGNORE no-ops on duplicates).
            let inserted = pool_conn
                .execute(
                    "INSERT OR IGNORE INTO tasks (id, title, content, voice, model, status, total_chars, created_at, updated_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8)",
                    rusqlite::params![
                        id, title, content, voice, model, status,
                        content.chars().count() as i64, now
                    ],
                )
                .map_err(|e| mimotts_engine::EngineError::Internal(e.to_string()))?;
            if inserted > 0 {
                n += 1;
            }
        }
        n
    };
    println!("imported {imported} legacy tasks (status normalized, pending re-enqueue)");
    Ok(())
}

fn normalize_v3_status(raw: &str) -> String {
    let t = raw.trim().trim_matches('"').to_lowercase();
    match t.as_str() {
        "processing" | "chunking" => "synthesizing".into(),
        "mergingfailed" => "failed".into(),
        "done" | "completed" => "done".into(),
        other => other.into(),
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_batch(
    data_dir: PathBuf,
    db: PathBuf,
    txt: Vec<PathBuf>,
    session: Option<String>,
    voice: String,
    model: String,
    style: Option<String>,
    out: PathBuf,
    provider: Option<String>,
    workers: usize,
    json: bool,
) -> Result<(), mimotts_engine::EngineError> {
    if session.is_none() && txt.is_empty() {
        return Err(mimotts_engine::EngineError::InvalidInput(
            "need --txt (import) or --session (resume)".into(),
        ));
    }
    let out_display = out.display().to_string();
    let mut cfg = mimotts_engine::EngineConfig {
        data_dir: data_dir.clone(),
        db_path: db.to_string_lossy().to_string(),
        output_dir: out,
        workers,
        ..Default::default()
    };
    mimotts_engine::apply_env_overrides(&mut cfg);
    let engine = mimotts_engine::Engine::open(cfg)?;
    if engine.providers()?.iter().all(|p| !p.is_configured) {
        return Err(mimotts_engine::EngineError::NoProvider);
    }

    // ── import (streamed per file: constant memory, no all-files buffer) ──
    let sid = match session {
        Some(sid) => {
            if engine.session(&sid)?.is_none() {
                return Err(mimotts_engine::EngineError::NotFound(format!(
                    "session {sid}"
                )));
            }
            if json {
                println!("{}", serde_json::json!({"event":"resume","session_id": sid}));
            } else {
                println!("resuming session {sid} (no re-import)");
            }
            sid
        }
        None => {
            // Expand paths first (no file bytes held in memory).
            let mut paths: Vec<PathBuf> = Vec::new();
            for path in &txt {
                if path.is_dir() {
                    for entry in std::fs::read_dir(path)? {
                        let p = entry?.path();
                        if p.extension().map(|e| e == "txt").unwrap_or(false) {
                            paths.push(p);
                        }
                    }
                } else {
                    paths.push(path.clone());
                }
            }
            if paths.is_empty() {
                return Err(mimotts_engine::EngineError::InvalidInput(
                    "no txt files".into(),
                ));
            }
            let created = engine.create_session("CLI 批量合成")?;
            let sid = created.id.to_string();
            if json {
                println!("{}", serde_json::json!({"event":"import","files": paths.len()}));
            } else {
                println!("importing {} files…", paths.len());
            }
            let mut imported = 0usize;
            let mut rejected_total = 0usize;
            for path in paths {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                // One file at a time: a 1000-file import holds exactly one
                // file's bytes in memory (v4 fix — was O(all files)).
                let data = std::fs::read(&path)?;
                match engine.import_files(
                    Some(sid.clone()),
                    None,
                    &voice,
                    &model,
                    style.clone(),
                    provider.clone(),
                    vec![(name, data)],
                ) {
                    Ok(r) => {
                        imported += r.tasks_created;
                        rejected_total += r.rejected.len();
                        for x in &r.rejected {
                            if !json {
                                println!("  rejected: {x}");
                            }
                        }
                    }
                    Err(e) => {
                        rejected_total += 1;
                        if !json {
                            println!("  rejected (import error): {e}");
                        }
                    }
                }
            }
            if json {
                println!(
                    "{}",
                    serde_json::json!({"event":"imported","session_id": sid,
                        "tasks_created": imported, "rejected": rejected_total})
                );
            } else {
                println!(
                    "session {sid} created: {imported} tasks, {rejected_total} rejected"
                );
                println!("(resume later: mimotts run --session {sid} --out <dir>)");
            }
            sid
        }
    };

    // ── poll until terminal (shared by import & resume paths) ──
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let row = engine.session(&sid)?.ok_or_else(|| {
            mimotts_engine::EngineError::NotFound("session vanished".into())
        })?;
        if json {
            println!(
                "{}",
                serde_json::json!({"event":"progress","done": row.done_tasks, "failed": row.failed_tasks, "total": row.total_tasks})
            );
        }
        if matches!(row.status.as_str(), "completed" | "failed") {
            if json {
                println!(
                    "{}",
                    serde_json::json!({"event":"done","status": row.status, "done": row.done_tasks, "failed": row.failed_tasks, "out": out_display})
                );
            } else {
                println!(
                    "done: {} ok / {} failed (outputs in {})",
                    row.done_tasks, row.failed_tasks, out_display
                );
            }
            break;
        }
    }
    Ok(())
}
