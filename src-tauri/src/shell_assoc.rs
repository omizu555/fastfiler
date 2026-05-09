// Phase 2B: 純粋ロジックは fastfiler-domain::shell_assoc に移動済。
use fastfiler_domain::shell_assoc as core;

#[tauri::command]
pub fn shell_assoc_enable() -> Result<(), String> { core::shell_assoc_enable() }

#[tauri::command]
pub fn shell_assoc_status() -> Result<bool, String> { core::shell_assoc_status() }

#[tauri::command]
pub fn shell_assoc_disable() -> Result<(), String> { core::shell_assoc_disable() }

#[tauri::command]
pub fn shell_assoc_diagnose() -> Result<String, String> { core::shell_assoc_diagnose() }
