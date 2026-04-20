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

/// 検証付きで一覧 (manifest 不正もエラー入りで返す)
#[derive(Serialize, Clone, Debug)]
pub struct PluginStatus {
    pub dir: String,
    pub id: Option<String>,
    pub manifest: Option<PluginManifest>,
    pub error: Option<String>,
}

const ALLOWED_CAPS: &[&str] = &[
    "fs.read.dir","fs.read.text","fs.stat","fs.write.text","fs.mkdir","fs.rename",
    "fs.copy","fs.move","fs.delete","shell.open","storage.get","storage.set",
    "ui.notify","pane.getActive","pane.setPath","ui.contextMenu.register",
];

#[tauri::command]
pub fn list_plugins_with_status() -> AppResult<Vec<PluginStatus>> {
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
        let dir_str = p.to_string_lossy().to_string();
        let manifest_path = p.join("manifest.json");
        if !manifest_path.exists() {
            out.push(PluginStatus { dir: dir_str, id: None, manifest: None, error: Some("manifest.json が見つかりません".into()) });
            continue;
        }
        let raw = match std::fs::read_to_string(&manifest_path) {
            Ok(s) => s,
            Err(e) => { out.push(PluginStatus { dir: dir_str, id: None, manifest: None, error: Some(format!("読み込み失敗: {e}")) }); continue; }
        };
        let manifest: PluginManifest = match serde_json::from_str(&raw) {
            Ok(m) => m,
            Err(e) => { out.push(PluginStatus { dir: dir_str, id: None, manifest: None, error: Some(format!("manifest 解析失敗: {e}")) }); continue; }
        };
        let mut errs: Vec<String> = Vec::new();
        if manifest.id.trim().is_empty() { errs.push("id が空".into()); }
        if manifest.name.trim().is_empty() { errs.push("name が空".into()); }
        let entry_path = p.join(&manifest.entry);
        if !entry_path.exists() { errs.push(format!("entry '{}' が存在しません", manifest.entry)); }
        for c in &manifest.capabilities {
            if !ALLOWED_CAPS.iter().any(|a| a == c) {
                errs.push(format!("未知の capability: {c}"));
            }
        }
        let id = Some(manifest.id.clone());
        let error = if errs.is_empty() { None } else { Some(errs.join(" / ")) };
        out.push(PluginStatus { dir: dir_str, id, manifest: Some(manifest), error });
    }
    Ok(out)
}

/// ZIP インポート: 1階層目に manifest.json があれば <id>/ に、無ければ ZIP 内最上位フォルダを <id>/ にリネーム
#[tauri::command]
pub fn import_plugin_zip(zip_path: String) -> AppResult<String> {
    let dir = plugins_dir().ok_or_else(|| AppError::Other("no plugin dir".into()))?;
    let f = std::fs::File::open(&zip_path).map_err(|e| AppError::Other(format!("zip open: {e}")))?;
    let mut archive = zip::ZipArchive::new(f).map_err(|e| AppError::Other(format!("zip read: {e}")))?;

    // 1) manifest.json を探す → そこから id を決定
    let mut manifest_inner: Option<String> = None;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| AppError::Other(format!("zip entry: {e}")))?;
        let name = entry.name().replace('\\', "/");
        if name.ends_with("manifest.json") && !name.contains("__MACOSX") {
            manifest_inner = Some(name);
            break;
        }
    }
    let manifest_inner = manifest_inner.ok_or_else(|| AppError::Other("manifest.json が ZIP 内にありません".into()))?;
    let mut mf = archive.by_name(&manifest_inner).map_err(|e| AppError::Other(format!("zip manifest: {e}")))?;
    let mut raw = String::new();
    use std::io::Read;
    mf.read_to_string(&mut raw).map_err(|e| AppError::Other(format!("manifest read: {e}")))?;
    drop(mf);
    let manifest: PluginManifest = serde_json::from_str(&raw)
        .map_err(|e| AppError::Other(format!("manifest 解析: {e}")))?;
    if manifest.id.trim().is_empty() {
        return Err(AppError::Other("manifest.id が空です".into()));
    }

    // 2) 展開先
    let target = dir.join(&manifest.id);
    if target.exists() {
        std::fs::remove_dir_all(&target).map_err(|e| AppError::Other(format!("既存削除: {e}")))?;
    }
    std::fs::create_dir_all(&target).map_err(|e| AppError::Other(format!("mkdir: {e}")))?;

    // 3) 共通プレフィックス算出 (manifest.json があった階層)
    let prefix = {
        let mut p = manifest_inner.clone();
        if let Some(pos) = p.rfind('/') { p.truncate(pos + 1); } else { p.clear(); }
        p
    };

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| AppError::Other(format!("zip entry: {e}")))?;
        let name = entry.name().replace('\\', "/");
        if name.contains("__MACOSX") { continue; }
        let rel = if prefix.is_empty() { name.clone() } else if let Some(s) = name.strip_prefix(&prefix) { s.to_string() } else { continue };
        if rel.is_empty() { continue; }
        let out_path = target.join(&rel);
        if entry.is_dir() || rel.ends_with('/') {
            std::fs::create_dir_all(&out_path).ok();
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::Other(format!("mkdir: {e}")))?;
        }
        let mut out_f = std::fs::File::create(&out_path).map_err(|e| AppError::Other(format!("create {}: {e}", out_path.display())))?;
        std::io::copy(&mut entry, &mut out_f).map_err(|e| AppError::Other(format!("write: {e}")))?;
    }
    Ok(manifest.id)
}

#[tauri::command]
pub fn delete_plugin(id: String) -> AppResult<()> {
    let dir = plugins_dir().ok_or_else(|| AppError::Other("no plugin dir".into()))?;
    let target = dir.join(&id);
    if !target.exists() { return Ok(()); }
    // 安全策: plugins_dir 配下であることを再確認
    let canon_dir = std::fs::canonicalize(&dir).unwrap_or(dir.clone());
    let canon_target = std::fs::canonicalize(&target).unwrap_or(target.clone());
    if !canon_target.starts_with(&canon_dir) {
        return Err(AppError::Other("invalid plugin path".into()));
    }
    std::fs::remove_dir_all(&canon_target).map_err(|e| AppError::Other(format!("削除失敗: {e}")))?;
    Ok(())
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
    let arg_str = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let arg_bool = |k: &str, d: bool| args.get(k).and_then(|v| v.as_bool()).unwrap_or(d);
    match capability.as_str() {
        // ---- read ----
        "fs.read.dir" => {
            let v = crate::fs_service::list_dir(arg_str("path"))?;
            Ok(serde_json::to_value(v).unwrap_or(serde_json::Value::Null))
        }
        "fs.read.text" => {
            let v = crate::preview::read_text_preview(arg_str("path"), Some(64 * 1024))?;
            Ok(serde_json::to_value(v).unwrap_or(serde_json::Value::Null))
        }
        "fs.stat" => {
            let v = crate::fs_service::stat_path(arg_str("path"))?;
            Ok(serde_json::to_value(v).unwrap_or(serde_json::Value::Null))
        }
        // ---- write / mutating ----
        "fs.write.text" => {
            let path = arg_str("path");
            let content = arg_str("content");
            std::fs::write(&path, content.as_bytes())
                .map_err(|e| AppError::Other(format!("write failed: {e}")))?;
            Ok(serde_json::Value::Null)
        }
        "fs.mkdir" => {
            let path = arg_str("path");
            let recursive = arg_bool("recursive", true);
            if recursive {
                std::fs::create_dir_all(&path)
            } else {
                std::fs::create_dir(&path)
            }
            .map_err(|e| AppError::Other(format!("mkdir failed: {e}")))?;
            Ok(serde_json::Value::Null)
        }
        "fs.rename" => {
            crate::file_ops::rename_path(arg_str("from"), arg_str("to"))?;
            Ok(serde_json::Value::Null)
        }
        "fs.copy" => {
            crate::file_ops::copy_path(arg_str("from"), arg_str("to"))?;
            Ok(serde_json::Value::Null)
        }
        "fs.move" => {
            crate::file_ops::move_path(arg_str("from"), arg_str("to"))?;
            Ok(serde_json::Value::Null)
        }
        "fs.delete" => {
            let paths: Vec<String> = args
                .get("paths")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            let permanent = arg_bool("permanent", false);
            if permanent {
                for p in &paths {
                    crate::file_ops::delete_path(p.clone(), true)?;
                }
            } else {
                crate::file_ops::delete_to_trash(paths)?;
            }
            Ok(serde_json::Value::Null)
        }
        // ---- shell ----
        "shell.open" => {
            crate::shell::open_with_shell(arg_str("path"))?;
            Ok(serde_json::Value::Null)
        }
        // ---- storage (per-plugin KV in plugins/<id>/storage.json) ----
        "storage.get" => {
            let key = arg_str("key");
            let st = read_plugin_storage(&dir, &plugin_id)?;
            Ok(st.get(&key).cloned().unwrap_or(serde_json::Value::Null))
        }
        "storage.set" => {
            let key = arg_str("key");
            let value = args.get("value").cloned().unwrap_or(serde_json::Value::Null);
            let mut st = read_plugin_storage(&dir, &plugin_id)?;
            if value.is_null() {
                st.remove(&key);
            } else {
                st.insert(key, value);
            }
            write_plugin_storage(&dir, &plugin_id, &st)?;
            Ok(serde_json::Value::Null)
        }
        // ---- pane / ui (フロント側で処理。capability チェックだけ通す) ----
        "ui.notify" | "pane.getActive" | "pane.setPath" | "ui.contextMenu.register" => {
            Err(AppError::Other(format!(
                "capability '{capability}' is handled by frontend host (do not call via plugin_invoke)"
            )))
        }
        _ => Err(AppError::Other(format!("unknown capability: {capability}"))),
    }
}

// ---- storage helpers ----
fn storage_path(dir: &std::path::Path, plugin_id: &str) -> PathBuf {
    dir.join(plugin_id).join("storage.json")
}

fn read_plugin_storage(
    dir: &std::path::Path,
    plugin_id: &str,
) -> AppResult<serde_json::Map<String, serde_json::Value>> {
    let p = storage_path(dir, plugin_id);
    if !p.exists() {
        return Ok(serde_json::Map::new());
    }
    let raw = std::fs::read_to_string(&p)
        .map_err(|e| AppError::Other(format!("storage read: {e}")))?;
    let v: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| AppError::Other(format!("storage parse: {e}")))?;
    Ok(v.as_object().cloned().unwrap_or_default())
}

fn write_plugin_storage(
    dir: &std::path::Path,
    plugin_id: &str,
    st: &serde_json::Map<String, serde_json::Value>,
) -> AppResult<()> {
    let p = storage_path(dir, plugin_id);
    let raw = serde_json::to_string_pretty(&serde_json::Value::Object(st.clone()))
        .map_err(|e| AppError::Other(format!("storage serialize: {e}")))?;
    std::fs::write(&p, raw).map_err(|e| AppError::Other(format!("storage write: {e}")))?;
    Ok(())
}
