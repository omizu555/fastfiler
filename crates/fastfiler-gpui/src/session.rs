//! セッション永続化: タブ / 分割構成・各ペインのフォルダを JSON で保存し、
//! 次回起動時に復元する。
//!
//! 保存先: `%APPDATA%\FastFiler\gpui_session.json`
//! 保存タイミング: 構成変更後 800ms デバウンス + アプリ終了時 (on_app_quit)。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
pub struct SessionData {
    pub active: usize,
    /// ワークスペースツリーパネルの表示状態。
    #[serde(default = "default_true")]
    pub show_tree: bool,
    /// ツリーパネルの幅 (px)。
    #[serde(default = "default_tree_width")]
    pub tree_width: f32,
    /// タブバーの幅 (px)。
    #[serde(default = "default_tab_width")]
    pub tab_width: f32,
    /// ウィンドウの位置とサイズ [x, y, w, h] (px)。
    #[serde(default)]
    pub window: Option<[f32; 4]>,
    /// ワークスペースツリーに登録済みの UNC share (`\\server\share`)。
    #[serde(default)]
    pub unc_shares: Vec<String>,
    /// テーマ名 (プリセット)。
    #[serde(default)]
    pub theme: Option<String>,
    pub tabs: Vec<NodeData>,
}

fn default_true() -> bool {
    true
}

fn default_tree_width() -> f32 {
    220.0
}

fn default_tab_width() -> f32 {
    200.0
}

/// ペインツリーのシリアライズ表現。
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NodeData {
    Leaf {
        path: String,
        #[serde(default)]
        focused: bool,
        /// 列幅 [更新日時, サイズ, 種類] (px)。ペイン個別。
        #[serde(default)]
        cols: Option<[f32; 3]>,
    },
    Split {
        /// "row" | "column"
        dir: String,
        ratios: Vec<f32>,
        children: Vec<NodeData>,
    },
}

fn session_path() -> Option<PathBuf> {
    let base = std::env::var("APPDATA").ok()?;
    Some(
        PathBuf::from(base)
            .join("FastFiler")
            .join("gpui_session.json"),
    )
}

pub fn load() -> Option<SessionData> {
    let p = session_path()?;
    let s = std::fs::read_to_string(p).ok()?;
    serde_json::from_str(&s).ok()
}

pub fn save(data: &SessionData) {
    let Some(p) = session_path() else {
        return;
    };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(s) = serde_json::to_string_pretty(data) {
        let _ = std::fs::write(p, s);
    }
}
