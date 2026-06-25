//! セッション永続化: タブ / 分割構成・各ペインのフォルダを JSON で保存し、
//! 次回起動時に復元する。
//!
//! 保存先: `%APPDATA%\FastFiler\gpui_session.json`
//! 保存タイミング: 構成変更後 800ms デバウンス + アプリ終了時 (on_app_quit)。
//!
//! クラッシュ・電源断では on_app_quit が呼ばれないため、デバウンス保存だけが
//! 頼り。その保存が壊れてもタブが消えないよう、書き込みは [`crate::persist`] の
//! アトミック書き込み + `.bak` フォールバックを使う (素の `fs::write` は途中切れ /
//! 0 バイト化の危険がある)。

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
    /// 最大化中は通常表示時 (restore) の位置を保持する。
    #[serde(default)]
    pub window: Option<[f32; 4]>,
    /// 最大化状態で終了したか (次回起動時に最大化で復元)。
    #[serde(default)]
    pub maximized: bool,
    /// ワークスペースツリーに登録済みの UNC share (`\\server\share`)。
    #[serde(default)]
    pub unc_shares: Vec<String>,
    /// テーマ名 (プリセット)。
    #[serde(default)]
    pub theme: Option<String>,
    /// 各タブのロック状態 (tabs と同じ並び)。
    #[serde(default)]
    pub locked: Vec<bool>,
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
    // 本体が壊れていれば .bak から復元する。
    crate::persist::load_with_backup(&p, |s| serde_json::from_str(s).ok())
}

pub fn save(data: &SessionData) {
    let Some(p) = session_path() else {
        return;
    };
    if let Ok(s) = serde_json::to_string_pretty(data) {
        // アトミック書き込み + .bak 退避 (電源断でもタブ情報が消えないように)。
        let _ = crate::persist::write_atomic(&p, &s);
    }
}
