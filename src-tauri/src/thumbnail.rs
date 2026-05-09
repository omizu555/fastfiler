// Phase 2B-3: 純粋ロジックは fastfiler-domain::thumbnail に移動済。
use crate::error::AppResult;
use fastfiler_domain::thumbnail as core;

pub use core::ThumbnailResult;

#[tauri::command]
pub fn get_thumbnail(path: String, size: u32) -> AppResult<ThumbnailResult> {
    core::get_thumbnail(path, size)
}
