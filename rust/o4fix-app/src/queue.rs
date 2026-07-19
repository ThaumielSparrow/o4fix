use crate::settings::GuiSettings;
use o4core::pipeline::{self, Outcome, Progress, Stage};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

#[derive(Default)]
pub struct AppState {
    next_id: AtomicU64,
    jobs: Mutex<HashMap<u64, Arc<AtomicBool>>>, // id -> cancel flag
}

#[derive(Clone, Serialize)]
pub struct JobProgress {
    pub id: u64,
    pub file: String,
    pub stage: &'static str,
    pub pct: f64,
    pub detail: String,
}

#[derive(Clone, Serialize)]
pub struct JobDone {
    pub id: u64,
    pub status: &'static str,
    pub message: String,
}

pub fn stage_name(s: Stage) -> &'static str {
    match s {
        Stage::Extract | Stage::Analyze => "analyzing",
        Stage::Optical => "measuring motion",
        Stage::Splice => "patching",
        Stage::Write => "verifying",
    }
}

struct Job {
    id: u64,
    file: PathBuf,
    cancel: Arc<AtomicBool>,
}

#[tauri::command]
pub fn start_queue(
    app: AppHandle,
    state: State<'_, AppState>,
    files: Vec<String>,
    settings: GuiSettings,
) -> Vec<u64> {
    let cfg = settings.config.to_config();
    let out_dir = settings.output_dir.clone().map(PathBuf::from);
    let queue: Arc<Mutex<VecDeque<Job>>> = Arc::default();
    let mut ids = Vec::new();
    {
        let mut q = queue.lock().unwrap();
        let mut jobs = state.jobs.lock().unwrap();
        for f in files {
            let id = state.next_id.fetch_add(1, Ordering::Relaxed);
            let cancel = Arc::new(AtomicBool::new(false));
            jobs.insert(id, cancel.clone());
            q.push_back(Job {
                id,
                file: PathBuf::from(f),
                cancel,
            });
            ids.push(id);
        }
    }
    let workers = settings.concurrent_files.clamp(1, 3).min(ids.len());
    for _ in 0..workers {
        let app = app.clone();
        let queue = queue.clone();
        let cfg = cfg.clone();
        let out_dir = out_dir.clone();
        std::thread::spawn(move || loop {
            let job = queue.lock().unwrap().pop_front();
            let Some(job) = job else { break };
            run_job(&app, job, &cfg, out_dir.as_deref());
        });
    }
    ids
}

fn run_job(app: &AppHandle, job: Job, cfg: &o4core::config::Config, out_dir: Option<&Path>) {
    let fname = job
        .file
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let finish = |status: &'static str, message: String| {
        let _ = app.emit(
            "job_done",
            JobDone {
                id: job.id,
                status,
                message,
            },
        );
    };
    if !fname.to_ascii_lowercase().ends_with(".mp4") {
        finish("error", "not an .MP4 file".into());
        return;
    }
    // output-folder override reuses pipeline's default naming scheme
    let out = out_dir.map(|d| {
        let stem = job.file.file_stem().unwrap_or_default().to_string_lossy();
        let ext = job
            .file
            .extension()
            .map(|e| e.to_string_lossy())
            .unwrap_or_default();
        if ext.is_empty() {
            d.join(format!("{stem}_fixed"))
        } else {
            d.join(format!("{stem}_fixed.{ext}"))
        }
    });
    let emit_progress = |p: Progress| {
        if !p.message.is_empty() {
            let _ = app.emit(
                "job_log",
                serde_json::json!({ "id": job.id, "line": p.message }),
            );
        }
        let _ = app.emit(
            "job_progress",
            JobProgress {
                id: job.id,
                file: fname.clone(),
                stage: stage_name(p.stage),
                pct: p.pct,
                detail: p.message,
            },
        );
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pipeline::process(&job.file, out.as_deref(), cfg, &emit_progress, &job.cancel)
    }));
    match result {
        Ok(Ok(Outcome::Repaired { out, .. })) => finish("done", out.display().to_string()),
        Ok(Ok(Outcome::Healthy)) => finish(
            "healthy",
            "telemetry looks healthy, nothing to repair".into(),
        ),
        Ok(Err(o4core::error::O4Error::Cancelled)) => finish("cancelled", String::new()),
        Ok(Err(e)) => finish("error", e.to_string()),
        Err(panic) => {
            let msg = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".into());
            finish("error", format!("internal error: {msg}"));
        }
    }
}

#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>, id: u64) {
    if let Some(c) = state.jobs.lock().unwrap().get(&id) {
        c.store(true, Ordering::Relaxed);
    }
}

#[tauri::command]
pub async fn pick_files(app: AppHandle) -> Vec<String> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog()
        .file()
        .add_filter("MP4 video", &["mp4", "MP4"])
        .blocking_pick_files()
        .map(|files| files.into_iter().map(|f| f.to_string()).collect())
        .unwrap_or_default()
}

#[tauri::command]
pub fn load_settings(app: AppHandle) -> GuiSettings {
    crate::settings::load_from(&crate::settings::settings_path(&app))
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: GuiSettings) -> Result<(), String> {
    crate::settings::save_to(&crate::settings::settings_path(&app), &settings)
}
