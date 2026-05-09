// Phase 2B: 純粋ロジックは fastfiler-domain::shell に移動済。
use crate::error::AppResult;
use fastfiler_domain::shell as core;

#[tauri::command]
pub fn open_with_shell(path: String) -> AppResult<()> { core::open_with_shell(path) }

#[tauri::command]
pub fn reveal_in_explorer(path: String) -> AppResult<()> { core::reveal_in_explorer(path) }

#[tauri::command]
pub fn show_properties(path: String) -> AppResult<()> { core::show_properties(path) }
