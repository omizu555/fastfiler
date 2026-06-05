//! アプリ設定 (`%APPDATA%\FastFiler\gpui_settings.json`)。
//!
//! セッション (レイアウト = gpui_session.json) とは別管理。設定画面 (⚙) から
//! 変更し、即保存する。`get()` は static 経由でどこからでも参照できる
//! (例: 検索の Everything ポート)。

use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct AppSettings {
    /// テーマ名 (プリセット)。None なら既定 (ダーク)。
    #[serde(default)]
    pub theme: Option<String>,
    /// Everything HTTP サーバのポート (検索連携)。
    #[serde(default = "default_port")]
    pub everything_port: u16,
}

fn default_port() -> u16 {
    80
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: None,
            everything_port: default_port(),
        }
    }
}

fn config_path() -> Option<PathBuf> {
    let base = std::env::var("APPDATA").ok()?;
    Some(
        PathBuf::from(base)
            .join("FastFiler")
            .join("gpui_settings.json"),
    )
}

fn store() -> &'static RwLock<AppSettings> {
    static STORE: OnceLock<RwLock<AppSettings>> = OnceLock::new();
    STORE.get_or_init(|| RwLock::new(AppSettings::default()))
}

/// 起動時に 1 回呼ぶ: ファイルから読み込んで static に格納し、その値を返す。
pub fn load() -> AppSettings {
    let s = config_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<AppSettings>(&t).ok())
        .unwrap_or_default();
    *store().write().unwrap() = s.clone();
    s
}

/// 現在の設定 (コピー)。
pub fn get() -> AppSettings {
    store().read().unwrap().clone()
}

/// 設定を変更して即保存する。
pub fn update(f: impl FnOnce(&mut AppSettings)) {
    let snapshot = {
        let mut s = store().write().unwrap();
        f(&mut s);
        s.clone()
    };
    if let Some(p) = config_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(&snapshot) {
            let _ = std::fs::write(p, text);
        }
    }
}
