// Phase 2B: 純粋ロジックは fastfiler-domain::preview に移動済。
use crate::error::AppResult;
use fastfiler_domain::preview as core;

pub use core::PreviewData;

#[tauri::command]
pub fn read_text_preview(path: String, max_bytes: Option<u64>) -> AppResult<PreviewData> {
    core::read_text_preview(path, max_bytes)
}
