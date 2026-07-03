//! セッション永続化 (F-1001/F-1002)。GPUI 版 session.rs のスキーマ互換。
//!
//! - 保存先: `%APPDATA%\FastFiler\iced_session.json` (開発期は GPUI 版と別ファイル —
//!   凍結中の GPUI 版のセッションを壊さないため。Phase 7 の切替時に統合を判断)
//! - 初回起動で iced_session.json が無ければ **gpui_session.json から自動移行** (N-05)
//! - 書き込みは persist のアトミック書き込み + .bak (F-1002)

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::app_model::{AppModel, TabState};
use crate::bsp::{PaneNode, SplitDir};
use crate::model::{PaneId, PaneState, DEFAULT_ROW_H};

#[derive(Serialize, Deserialize)]
pub struct SessionData {
    pub active: usize,
    /// ワークスペースツリーパネルの表示状態 (Phase 4 で使用)。
    #[serde(default = "default_true")]
    pub show_tree: bool,
    #[serde(default = "default_tree_width")]
    pub tree_width: f32,
    #[serde(default = "default_tab_width")]
    pub tab_width: f32,
    /// ウィンドウの位置とサイズ [x, y, w, h]。最大化中は restore 時の値。
    #[serde(default)]
    pub window: Option<[f32; 4]>,
    #[serde(default)]
    pub maximized: bool,
    /// ワークスペースツリーの UNC share (Phase 4 で使用)。
    #[serde(default)]
    pub unc_shares: Vec<String>,
    /// テーマ名 (Phase 6 で使用。GPUI 版旧互換フィールド)。
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

/// ペインツリーのシリアライズ表現 (GPUI 版と同一のタグ付き JSON)。
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NodeData {
    Leaf {
        path: String,
        #[serde(default)]
        focused: bool,
        /// 列幅 [更新日時, サイズ, 種類] (px)。ペイン個別 (F-402)。
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

/// AppModel → セッションデータ (保存用スナップショット)。
pub fn snapshot(m: &AppModel, window: Option<[f32; 4]>, maximized: bool) -> SessionData {
    SessionData {
        active: m.active,
        show_tree: true,
        tree_width: 220.0,
        tab_width: m.tab_width,
        window,
        maximized,
        unc_shares: Vec::new(),
        theme: None,
        locked: m.tabs.iter().map(|t| t.locked).collect(),
        tabs: m.tabs.iter().map(|t| node_to_data(m, t, &t.root)).collect(),
    }
}

fn node_to_data(m: &AppModel, tab: &TabState, node: &PaneNode) -> NodeData {
    match node {
        PaneNode::Leaf(id) => {
            let p = &m.panes[*id];
            NodeData::Leaf {
                path: p.cur_path.to_string_lossy().to_string(),
                focused: tab.focused == *id,
                cols: Some(p.col_widths),
            }
        }
        PaneNode::Split {
            dir,
            ratios,
            children,
            ..
        } => NodeData::Split {
            dir: match dir {
                SplitDir::Row => "row".to_string(),
                SplitDir::Column => "column".to_string(),
            },
            ratios: ratios.clone(),
            children: children.iter().map(|c| node_to_data(m, tab, c)).collect(),
        },
    }
}

/// セッションデータ → AppModel (起動時の復元)。
/// 壊れた形 (空タブ等) は安全側に落とし、必ず 1 タブ以上を保証する。
/// 戻り値: 復元した全ペイン (呼び出し側が読み込みを開始する)。
pub fn restore(data: &SessionData, fallback: PathBuf) -> (AppModel, Vec<PaneId>) {
    let mut m = AppModel::new(fallback);
    // AppModel::new が作った初期タブは復元後に除去する
    let placeholder = m.tabs[0].root.first_leaf();

    for (ix, node) in data.tabs.iter().enumerate() {
        let locked = data.locked.get(ix).copied().unwrap_or(false);
        if let Some((root, focused)) = data_to_node(&mut m, node) {
            let name_src = root.first_leaf();
            m.tabs.push(TabState {
                focused: focused.unwrap_or_else(|| root.first_leaf()),
                root,
                locked,
                name_src,
            });
        }
    }
    if m.tabs.len() > 1 {
        // プレースホルダタブを除去
        m.tabs.remove(0);
        m.remove_pane(placeholder);
        m.active = data.active.min(m.tabs.len() - 1);
    }
    m.tab_width = data
        .tab_width
        .clamp(crate::app_model::TAB_W_MIN, crate::app_model::TAB_W_MAX);
    let panes: Vec<PaneId> = m.panes.keys().collect();
    (m, panes)
}

/// NodeData → PaneNode。パス不明の葉も一旦作る (読み込み失敗はエラー表示で残す)。
fn data_to_node(m: &mut AppModel, node: &NodeData) -> Option<(PaneNode, Option<PaneId>)> {
    match node {
        NodeData::Leaf {
            path,
            focused,
            cols,
        } => {
            let id = m.new_pane(PathBuf::from(path));
            let p: &mut PaneState = &mut m.panes[id];
            p.row_h = DEFAULT_ROW_H;
            if let Some(c) = cols {
                p.col_widths = *c;
            }
            Some((PaneNode::Leaf(id), focused.then_some(id)))
        }
        NodeData::Split {
            dir,
            ratios,
            children,
        } => {
            let dir = match dir.as_str() {
                "column" => SplitDir::Column,
                _ => SplitDir::Row,
            };
            let mut built = Vec::new();
            let mut focused = None;
            for c in children {
                if let Some((n, f)) = data_to_node(m, c) {
                    built.push(n);
                    focused = focused.or(f);
                }
            }
            match built.len() {
                0 => None,
                1 => Some((built.remove(0), focused)),
                n => {
                    m.next_split_id += 1;
                    let ratios = if ratios.len() == n {
                        ratios.clone()
                    } else {
                        vec![1.0 / n as f32; n]
                    };
                    Some((
                        PaneNode::Split {
                            id: m.next_split_id,
                            dir,
                            ratios,
                            children: built,
                        },
                        focused,
                    ))
                }
            }
        }
    }
}

fn session_dir() -> Option<PathBuf> {
    let base = std::env::var("APPDATA").ok()?;
    Some(PathBuf::from(base).join("FastFiler"))
}

fn session_path() -> Option<PathBuf> {
    Some(session_dir()?.join("iced_session.json"))
}

pub fn load() -> Option<SessionData> {
    let dir = session_dir()?;
    let own = dir.join("iced_session.json");
    if let Some(d) = crate::persist::load_with_backup(&own, |s| serde_json::from_str(s).ok()) {
        return Some(d);
    }
    // 初回: GPUI 版セッションから自動移行 (読むだけ — 元ファイルには触れない)
    let gpui = dir.join("gpui_session.json");
    crate::persist::load_with_backup(&gpui, |s| serde_json::from_str(s).ok())
}

pub fn save(data: &SessionData) {
    let Some(p) = session_path() else {
        return;
    };
    if let Ok(s) = serde_json::to_string_pretty(data) {
        let _ = crate::persist::write_atomic(&p, &s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update_app::{update_app, AppMsg, TabMsg};

    #[test]
    fn snapshot_restore_roundtrip_preserves_structure() {
        let mut m = AppModel::new(PathBuf::from("C:\\a"));
        update_app(&mut m, AppMsg::Tab(TabMsg::Add));
        update_app(&mut m, AppMsg::Tab(TabMsg::SplitFocused(SplitDir::Row)));
        update_app(&mut m, AppMsg::Tab(TabMsg::SplitFocused(SplitDir::Column)));
        update_app(&mut m, AppMsg::Tab(TabMsg::ToggleLock(0)));
        {
            let id = m.focused_pane();
            m.panes[id].col_widths = [100.0, 50.0, 60.0];
        }
        m.tab_width = 320.0;
        m.active = 1;

        let data = snapshot(&m, Some([10.0, 20.0, 800.0, 600.0]), true);
        let json = serde_json::to_string(&data).unwrap();
        let parsed: SessionData = serde_json::from_str(&json).unwrap();
        let (r, panes) = restore(&parsed, PathBuf::from("C:\\fallback"));

        assert_eq!(r.tabs.len(), 2);
        assert_eq!(r.active, 1);
        assert_eq!(r.tab_width, 320.0);
        assert!(r.tabs[0].locked);
        assert!(!r.tabs[1].locked);
        // タブ 1 は Row(a, Column(a, a)) → 葉 3 枚
        assert_eq!(r.tabs[1].root.leaves().len(), 3);
        assert_eq!(panes.len(), 4);
        // フォーカスペインの列幅が戻る
        let f = r.tabs[1].focused;
        assert_eq!(r.panes[f].col_widths, [100.0, 50.0, 60.0]);
        // カウンタを汚さないよう掃除
        let ids: Vec<_> = {
            let mut m2 = r;
            let ids: Vec<_> = m2.panes.keys().collect();
            for id in &ids {
                m2.remove_pane(*id);
            }
            ids
        };
        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn restore_broken_session_falls_back_to_single_tab() {
        let data = SessionData {
            active: 5,
            show_tree: true,
            tree_width: 220.0,
            tab_width: 200.0,
            window: None,
            maximized: false,
            unc_shares: vec![],
            theme: None,
            locked: vec![],
            tabs: vec![], // タブなし (壊れたファイル)
        };
        let (m, panes) = restore(&data, PathBuf::from("C:\\fallback"));
        assert_eq!(m.tabs.len(), 1);
        assert_eq!(m.active, 0);
        assert_eq!(panes.len(), 1);
        assert_eq!(
            m.panes[m.focused_pane()].cur_path,
            PathBuf::from("C:\\fallback")
        );
    }

    #[test]
    fn gpui_session_json_shape_is_readable() {
        // GPUI 版が書く形をそのまま読めること (自動移行 — N-05)
        let json = r#"{
            "active": 0,
            "tab_width": 250.0,
            "locked": [true],
            "tabs": [{
                "type": "Split", "dir": "row", "ratios": [0.6, 0.4],
                "children": [
                    {"type": "Leaf", "path": "C:\\Windows", "focused": true, "cols": [140.0, 90.0, 90.0]},
                    {"type": "Leaf", "path": "D:\\data"}
                ]
            }]
        }"#;
        let parsed: SessionData = serde_json::from_str(json).unwrap();
        let (m, _) = restore(&parsed, PathBuf::from("C:\\"));
        assert_eq!(m.tabs.len(), 1);
        assert!(m.tabs[0].locked);
        assert_eq!(m.tabs[0].root.leaves().len(), 2);
        assert_eq!(
            m.panes[m.tabs[0].focused].cur_path,
            PathBuf::from("C:\\Windows")
        );
    }
}
