// Phase 2B: 純粋ロジックは fastfiler-domain::fs に移動済。
// このファイルは `#[tauri::command]` シムだけを保持する。
//
// 型 (FileEntry / DriveInfo / DiskInfo) は domain 側のものを re-export。

use crate::error::AppResult;
use fastfiler_domain::fs as core;

pub use core::{DiskInfo, DriveInfo, FileEntry};

#[tauri::command]
pub fn list_dir(path: String) -> AppResult<Vec<FileEntry>> {
    core::list_dir(path)
}

#[tauri::command]
pub fn stat_path(path: String) -> AppResult<FileEntry> {
    core::stat_path(path)
}

#[tauri::command]
pub fn list_dirs(path: String, include_hidden: Option<bool>) -> AppResult<Vec<FileEntry>> {
    core::list_dirs(path, include_hidden)
}

#[tauri::command]
pub fn home_dir() -> AppResult<String> {
    core::home_dir()
}

#[tauri::command]
pub fn list_drives() -> AppResult<Vec<DriveInfo>> {
    core::list_drives()
}

#[tauri::command]
pub fn disk_free(path: String) -> AppResult<DiskInfo> {
    core::disk_free(path)
}
