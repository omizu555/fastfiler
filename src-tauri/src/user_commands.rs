// Phase 2B-3: 純粋ロジックは fastfiler-domain::user_commands に移動済。
use crate::error::AppResult;
use fastfiler_domain::user_commands as core;

pub use core::{RunCtx, UserCommand};

#[tauri::command]
pub fn user_commands_dir() -> AppResult<String> { core::user_commands_dir() }

#[tauri::command]
pub fn list_user_commands() -> AppResult<Vec<UserCommand>> { core::list_user_commands() }

#[tauri::command]
pub fn run_user_command(id: String, ctx: RunCtx) -> AppResult<()> {
    core::run_user_command(id, ctx)
}
