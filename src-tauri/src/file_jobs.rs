// Phase 2B-4: 純粋ロジックは fastfiler-domain::file_jobs に移動済。
use crate::error::AppResult;
use crate::events::{self};
use fastfiler_domain::file_jobs as core;
use tauri::{AppHandle, State};

#[allow(unused_imports)]
pub use core::{JobDone, JobItem, JobProgress, JobRegistry};

#[tauri::command]
pub fn cancel_job(job_id: u64, state: State<'_, JobRegistry>) -> bool {
    state.cancel(job_id)
}

#[tauri::command]
pub async fn job_copy(
    job_id: u64,
    items: Vec<JobItem>,
    app: AppHandle,
    state: State<'_, JobRegistry>,
) -> AppResult<()> {
    let sink = events::tauri_sink(app);
    state.run_copy(&sink, job_id, items)
}

#[tauri::command]
pub async fn job_move(
    job_id: u64,
    items: Vec<JobItem>,
    app: AppHandle,
    state: State<'_, JobRegistry>,
) -> AppResult<()> {
    let sink = events::tauri_sink(app);
    state.run_move(&sink, job_id, items)
}

#[tauri::command]
pub async fn job_delete(
    job_id: u64,
    paths: Vec<String>,
    app: AppHandle,
    state: State<'_, JobRegistry>,
) -> AppResult<()> {
    let sink = events::tauri_sink(app);
    state.run_delete(&sink, job_id, paths)
}
