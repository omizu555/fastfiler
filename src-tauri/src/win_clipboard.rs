// Phase 2B: 純粋ロジックは fastfiler-domain::win_clipboard に移動済。
use crate::error::AppResult;
use fastfiler_domain::win_clipboard as core;

pub use core::ClipboardPaths;

#[tauri::command]
pub fn clipboard_write_paths(paths: Vec<String>, op: String) -> AppResult<()> {
    core::clipboard_write_paths(paths, op)
}

#[tauri::command]
pub fn clipboard_read_paths() -> AppResult<Option<ClipboardPaths>> {
    core::clipboard_read_paths()
}
