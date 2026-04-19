// Phase 6: プラグイン基盤
//
// 仕様（最小実装）:
// - %APPDATA%\fastfiler\plugins\<id>\manifest.json と index.html を配置
// - manifest: { id, name, version, entry: "index.html", capabilities: ["fs.read", ...] }
// - フロントは list_plugins で一覧取得 → サイドパネルの iframe で読み込み
// - プラグインは window.parent.postMessage で API 呼び出し
// - 公開 API は capabilities でホワイトリスト制御
//
// 本フェーズではディスカバリと manifest 読み込みのみ Rust 側で実装。
// API ブリッジはフロント (src/plugin-host.ts) で実装する。

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_entry")]
    pub entry: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

fn default_version() -> String { "0.0.0".into() }
fn default_entry() -> String { "index.html".into() }

#[derive(Serialize, Clone, Debug)]
pub struct PluginInfo {
    pub manifest: PluginManifest,
    pub dir: String,
    pub entry_path: String,
}

fn plugins_dir() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    let mut p = PathBuf::from(appdata);
    p.push("fastfiler");
    p.push("plugins");
    if !p.exists() {
        let _ = std::fs::create_dir_all(&p);
    }
    Some(p)
}

#[tauri::command]
pub fn list_plugins() -> AppResult<Vec<PluginInfo>> {
    let dir = match plugins_dir() {
        Some(d) => d,
        None => return Ok(vec![]),
    };
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(vec![]),
    };
    for ent in entries.flatten() {
        let p = ent.path();
        if !p.is_dir() { continue; }
        let manifest_path = p.join("manifest.json");
        if !manifest_path.exists() { continue; }
        let raw = match std::fs::read_to_string(&manifest_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let manifest: PluginManifest = match serde_json::from_str(&raw) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let entry_path = p.join(&manifest.entry);
        out.push(PluginInfo {
            manifest,
            dir: p.to_string_lossy().to_string(),
            entry_path: entry_path.to_string_lossy().to_string(),
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn plugins_dir_path() -> AppResult<String> {
    let p = plugins_dir().ok_or_else(|| AppError::Other("no APPDATA".into()))?;
    Ok(p.to_string_lossy().to_string())
}

/// プラグイン用の中継 IPC（capability チェック付き）
#[tauri::command]
pub fn plugin_invoke(
    plugin_id: String,
    capability: String,
    args: serde_json::Value,
) -> AppResult<serde_json::Value> {
    let dir = plugins_dir().ok_or_else(|| AppError::Other("no plugin dir".into()))?;
    let manifest_path = dir.join(&plugin_id).join("manifest.json");
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|_| AppError::Other(format!("plugin not found: {plugin_id}")))?;
    let manifest: PluginManifest = serde_json::from_str(&raw)
        .map_err(|e| AppError::Other(format!("manifest parse: {e}")))?;
    if !manifest.capabilities.iter().any(|c| c == &capability) {
        return Err(AppError::Other(format!(
            "capability '{capability}' not granted to plugin '{plugin_id}'"
        )));
    }
    match capability.as_str() {
        "fs.read.dir" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let v = crate::fs_service::list_dir(path.to_string())?;
            Ok(serde_json::to_value(v).unwrap_or(serde_json::Value::Null))
        }
        "fs.read.text" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let v = crate::preview::read_text_preview(path.to_string(), Some(64 * 1024))?;
            Ok(serde_json::to_value(v).unwrap_or(serde_json::Value::Null))
        }
        "shell.open" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            crate::shell::open_with_shell(path.to_string())?;
            Ok(serde_json::Value::Null)
        }
        _ => Err(AppError::Other(format!("unknown capability: {capability}"))),
    }
}
