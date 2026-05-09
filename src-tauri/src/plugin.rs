// Phase 2B: 純粋ロジックは fastfiler-domain::plugin に移動済。
use crate::error::AppResult;
use fastfiler_domain::plugin as core;

#[allow(unused_imports)]
pub use core::{PluginInfo, PluginManifest, PluginStatus};

#[tauri::command]
pub fn list_plugins() -> AppResult<Vec<PluginInfo>> { core::list_plugins() }

#[tauri::command]
pub fn plugins_dir_path() -> AppResult<String> { core::plugins_dir_path() }

#[tauri::command]
pub fn list_plugins_with_status() -> AppResult<Vec<PluginStatus>> { core::list_plugins_with_status() }

#[tauri::command]
pub fn import_plugin_zip(zip_path: String) -> AppResult<String> { core::import_plugin_zip(zip_path) }

#[tauri::command]
pub fn delete_plugin(id: String) -> AppResult<()> { core::delete_plugin(id) }

#[tauri::command]
pub fn plugin_invoke(
    plugin_id: String,
    capability: String,
    args: serde_json::Value,
) -> AppResult<serde_json::Value> {
    core::plugin_invoke(plugin_id, capability, args)
}
