// 新規ファイル テンプレート機能
//
// - templates_dir: %APPDATA%\fastfiler\templates (なければ作成)
// - list_templates: テンプレートフォルダ内のファイルを列挙
// - create_file_from_template: テンプレートを指定先にコピーして新規作成
// - create_empty_file: 空ファイルを作成 (テンプレート未指定時)
//
// 同名ファイルがあれば " (2)", " (3)"... を付与。

use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Clone)]
pub struct TemplateInfo {
    pub name: String,        // ファイル名 (拡張子含む)
    pub path: String,        // フルパス
    pub ext: Option<String>, // 拡張子 (lowercase, ドットなし)
}

fn templates_dir_inner() -> AppResult<PathBuf> {
    let appdata = std::env::var("APPDATA")
        .map_err(|_| AppError::Other("APPDATA not set".into()))?;
    let dir = PathBuf::from(appdata).join("fastfiler").join("templates");
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

#[tauri::command]
pub fn templates_dir() -> AppResult<String> {
    let p = templates_dir_inner()?;
    Ok(p.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn list_templates() -> AppResult<Vec<TemplateInfo>> {
    let dir = templates_dir_inner()?;
    let mut items = Vec::new();
    let rd = match fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(_) => return Ok(items),
    };
    for ent in rd.flatten() {
        let meta = match ent.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !meta.is_file() {
            continue;
        }
        let path = ent.path();
        let name = ent.file_name().to_string_lossy().into_owned();
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase());
        items.push(TemplateInfo {
            name,
            path: path.to_string_lossy().into_owned(),
            ext,
        });
    }
    items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(items)
}

fn unique_path(dir: &std::path::Path, base: &str, ext: &str) -> PathBuf {
    let mk = |n: u32| -> PathBuf {
        let name = if n == 0 {
            if ext.is_empty() {
                base.to_string()
            } else {
                format!("{}.{}", base, ext)
            }
        } else if ext.is_empty() {
            format!("{} ({})", base, n + 1)
        } else {
            format!("{} ({}).{}", base, n + 1, ext)
        };
        dir.join(name)
    };
    let mut n = 0u32;
    loop {
        let p = mk(n);
        if !p.exists() {
            return p;
        }
        n += 1;
        if n > 9999 {
            return p;
        }
    }
}

fn split_base_ext(name: &str) -> (String, String) {
    if let Some(idx) = name.rfind('.') {
        if idx > 0 {
            return (name[..idx].to_string(), name[idx + 1..].to_string());
        }
    }
    (name.to_string(), String::new())
}

#[tauri::command]
pub fn create_empty_file(dest_dir: String, file_name: String, body: Option<String>) -> AppResult<String> {
    let dir = PathBuf::from(&dest_dir);
    if !dir.is_dir() {
        return Err(AppError::Other(format!("destination is not a directory: {}", dest_dir)));
    }
    let (base, ext) = split_base_ext(&file_name);
    let p = unique_path(&dir, &base, &ext);
    match body {
        Some(text) if !text.is_empty() => fs::write(&p, text)?,
        _ => {
            fs::File::create(&p)?;
        }
    }
    Ok(p.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn create_file_from_template(
    template_path: String,
    dest_dir: String,
    file_name: Option<String>,
) -> AppResult<String> {
    let src = PathBuf::from(&template_path);
    let dir = PathBuf::from(&dest_dir);
    if !src.is_file() {
        return Err(AppError::Other(format!("template not found: {}", template_path)));
    }
    if !dir.is_dir() {
        return Err(AppError::Other(format!("destination is not a directory: {}", dest_dir)));
    }
    let name = file_name.unwrap_or_else(|| {
        src.file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "新規ファイル".into())
    });
    let (base, ext) = split_base_ext(&name);
    let dst = unique_path(&dir, &base, &ext);
    fs::copy(&src, &dst)?;
    Ok(dst.to_string_lossy().into_owned())
}
