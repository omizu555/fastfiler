// Phase 2B: 純粋ロジックは fastfiler-domain::templates に移動済。
use crate::error::AppResult;
use fastfiler_domain::templates as core;

pub use core::TemplateInfo;

#[tauri::command]
pub fn templates_dir() -> AppResult<String> { core::templates_dir() }

#[tauri::command]
pub fn list_templates() -> AppResult<Vec<TemplateInfo>> { core::list_templates() }

#[tauri::command]
pub fn create_empty_file(dest_dir: String, file_name: String, body: Option<String>) -> AppResult<String> {
    core::create_empty_file(dest_dir, file_name, body)
}

#[tauri::command]
pub fn create_file_from_template(
    template_path: String,
    dest_dir: String,
    file_name: Option<String>,
) -> AppResult<String> {
    core::create_file_from_template(template_path, dest_dir, file_name)
}
