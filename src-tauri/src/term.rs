// Phase 2B-4: 純粋ロジックは fastfiler-domain::term に移動済。
use crate::events::{self, EventSink};
use fastfiler_domain::term as core;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

#[allow(unused_imports)]
pub use core::{TermOpenOptions, TermRegistry, TermSession};

#[tauri::command]
pub fn term_open(
    cwd: Option<String>,
    shell: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
    app: AppHandle,
    reg: State<'_, TermRegistry>,
) -> Result<u64, String> {
    let sink: Arc<dyn EventSink> = Arc::new(events::tauri_sink(app));
    reg.open_with_sink(sink, TermOpenOptions { cwd, shell, cols, rows })
}

#[tauri::command]
pub fn term_write(id: u64, data: String, reg: State<'_, TermRegistry>) -> Result<(), String> {
    reg.write(id, data.as_bytes())
}

#[tauri::command]
pub fn term_resize(
    id: u64,
    cols: u16,
    rows: u16,
    reg: State<'_, TermRegistry>,
) -> Result<(), String> {
    reg.resize(id, cols, rows)
}

#[tauri::command]
pub fn term_close(id: u64, reg: State<'_, TermRegistry>) -> Result<(), String> {
    reg.close(id)
}

pub fn register(app: &AppHandle) {
    app.manage(TermRegistry::default());
}
