// Phase 2B: 純粋ロジックは fastfiler-domain::file_ops に移動済。
use crate::error::AppResult;
use fastfiler_domain::file_ops as core;

#[tauri::command]
pub fn create_dir(path: String) -> AppResult<()> { core::create_dir(path) }

#[tauri::command]
pub fn rename_path(from: String, to: String) -> AppResult<()> { core::rename_path(from, to) }

#[tauri::command]
pub fn delete_path(path: String, recursive: bool) -> AppResult<()> { core::delete_path(path, recursive) }

#[tauri::command]
pub fn copy_path(from: String, to: String) -> AppResult<()> { core::copy_path(from, to) }

#[tauri::command]
pub fn move_path(from: String, to: String) -> AppResult<()> { core::move_path(from, to) }

#[tauri::command]
pub fn delete_to_trash(paths: Vec<String>) -> AppResult<()> { core::delete_to_trash(paths) }
