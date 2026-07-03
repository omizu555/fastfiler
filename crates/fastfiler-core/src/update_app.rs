//! アプリ全体の reducer (計画書 §5.3)。タブ操作 + ペインへのルーティング。

use std::path::PathBuf;

use crate::app_model::{AppModel, TabState, TAB_W_MAX, TAB_W_MIN};
use crate::bsp::SplitDir;
use crate::effect::Effect;
use crate::model::PaneId;
use crate::msg::PaneMsg;
use crate::tree::TreeMsg;
use crate::update::{navigate, update_pane};

#[derive(Debug, Clone)]
pub enum AppMsg {
    /// 特定ペイン宛 (FileList イベント / domain イベントの振り分け)。
    Pane(PaneId, PaneMsg),
    /// フォーカスペイン宛 (キーボード操作)。
    Focused(PaneMsg),
    Tab(TabMsg),
    /// ワークスペースツリー (F-801〜F-803)。
    Tree(TreeMsg),
}

#[derive(Debug, Clone)]
pub enum TabMsg {
    /// ＋ボタン: アクティブペインと同じフォルダで開く (F-101)。
    Add,
    Select(usize),
    /// Ctrl+Tab / Ctrl+Shift+Tab (F-102)。
    Next,
    Prev,
    Close(usize),
    /// 中クリック: ロック切替 (F-104)。
    ToggleLock(usize),
    /// タブ D&D 並べ替え (F-103)。
    Reorder {
        from: usize,
        to: usize,
    },
    /// ペイン右上 ↔ ↕ (F-201)。
    SplitFocused(SplitDir),
    /// ペイン右上 × (F-202)。
    ClosePane(PaneId),
    /// F6 (F-204)。
    CycleFocus,
    /// 分割境界ドラッグ (F-203)。delta_ratio は比率差分 (px→比率変換は view 側)。
    Resize {
        split_id: u64,
        handle_ix: usize,
        delta_ratio: f32,
        min_ratio: f32,
    },
    /// タブバー幅リサイズ (F-106)。
    SetTabWidth(f32),
}

/// アプリ全体の入力を処理する。ペイン宛はロック状態を添えて `update_pane` へ委譲し、
/// ロックタブからの `OpenTabFor` はここで新タブに展開する (F-104)。
pub fn update_app(m: &mut AppModel, msg: AppMsg) -> Vec<Effect> {
    match msg {
        AppMsg::Focused(pmsg) => {
            let id = m.focused_pane();
            update_app(m, AppMsg::Pane(id, pmsg))
        }
        AppMsg::Pane(id, pmsg) => {
            // 閉じたペインへの遅延メッセージ (Task/Subscription 残り) は無害に捨てる
            if !m.panes.contains_key(id) {
                return vec![];
            }
            // クリック系はそのペインへフォーカスを移す (F-204)
            if matches!(
                pmsg,
                PaneMsg::RowPressed { .. }
                    | PaneMsg::RowDoubleClicked { .. }
                    | PaneMsg::BlankPressed
                    | PaneMsg::HeaderClicked(_)
                    | PaneMsg::OpenPathEdit
            ) {
                if let Some(tix) = tab_of(m, id) {
                    m.active = tix;
                    m.tabs[tix].focused = id;
                }
            }
            let locked = tab_of(m, id).map(|t| m.tabs[t].locked).unwrap_or(false);
            let structural = matches!(pmsg, PaneMsg::ColResized { .. });
            let effects = update_pane(&mut m.panes[id], id, locked, pmsg);
            let mut out = expand_open_tab(m, effects);
            // パス変更 (LoadDir) と列幅はセッション保存対象 (800ms デバウンス)。
            // フォーカスペインの移動はツリーを追従させ、UNC は自動登録する (F-802/F-803)
            if structural || out.iter().any(|e| matches!(e, Effect::LoadDir { .. })) {
                if out.iter().any(|e| matches!(e, Effect::LoadDir { .. })) {
                    let path = m.panes[m.focused_pane()].cur_path.clone();
                    m.tree.register_unc(&path);
                    let tree_fx = m.tree.reveal(&path);
                    out.extend(tree_fx);
                }
                out.push(Effect::ScheduleSessionSave);
            }
            out
        }
        AppMsg::Tab(tmsg) => {
            let mut out = update_tab(m, tmsg);
            out.push(Effect::ScheduleSessionSave);
            out
        }
        AppMsg::Tree(tmsg) => m.tree.update(tmsg),
    }
}

fn update_tab(m: &mut AppModel, msg: TabMsg) -> Vec<Effect> {
    match msg {
        TabMsg::Add => {
            let path = m.panes[m.focused_pane()].cur_path.clone();
            open_new_tab(m, path)
        }
        TabMsg::Select(ix) => {
            if ix < m.tabs.len() {
                m.active = ix;
            }
            vec![]
        }
        TabMsg::Next => {
            m.active = (m.active + 1) % m.tabs.len();
            vec![]
        }
        TabMsg::Prev => {
            m.active = (m.active + m.tabs.len() - 1) % m.tabs.len();
            vec![]
        }
        TabMsg::Close(ix) => m
            .close_tab(ix)
            .into_iter()
            .map(Effect::PaneClosed)
            .collect(),
        TabMsg::ToggleLock(ix) => {
            if let Some(t) = m.tabs.get_mut(ix) {
                t.locked = !t.locked;
            }
            vec![]
        }
        TabMsg::Reorder { from, to } => {
            if from < m.tabs.len() && to < m.tabs.len() && from != to {
                let t = m.tabs.remove(from);
                m.tabs.insert(to, t);
                // アクティブタブの位置を追従
                m.active = match m.active {
                    a if a == from => to,
                    a if from < a && a <= to => a - 1,
                    a if to <= a && a < from => a + 1,
                    a => a,
                };
            }
            vec![]
        }
        TabMsg::SplitFocused(dir) => {
            let new = m.split_focused(dir);
            let path = m.panes[new].cur_path.clone();
            // 新ペインは同じフォルダを読み込む (ロックはタブ単位なので自動継承)
            navigate(&mut m.panes[new], new, path)
        }
        TabMsg::ClosePane(id) => match m.close_pane(id) {
            Some(closed) => vec![Effect::PaneClosed(closed)],
            None => vec![],
        },
        TabMsg::CycleFocus => {
            m.cycle_focus();
            vec![]
        }
        TabMsg::Resize {
            split_id,
            handle_ix,
            delta_ratio,
            min_ratio,
        } => {
            m.active_tab_mut()
                .root
                .resize(split_id, handle_ix, delta_ratio, min_ratio);
            vec![]
        }
        TabMsg::SetTabWidth(w) => {
            m.tab_width = w.clamp(TAB_W_MIN, TAB_W_MAX);
            vec![]
        }
    }
}

/// ロックタブの `OpenTabFor` を新タブ + 読み込みに展開する。
fn expand_open_tab(m: &mut AppModel, effects: Vec<Effect>) -> Vec<Effect> {
    let mut out = Vec::with_capacity(effects.len());
    for e in effects {
        match e {
            Effect::OpenTabFor { path } => out.extend(open_new_tab(m, path)),
            other => out.push(other),
        }
    }
    out
}

fn open_new_tab(m: &mut AppModel, path: PathBuf) -> Vec<Effect> {
    let pane = m.add_tab(path.clone());
    navigate(&mut m.panes[pane], pane, path)
}

fn tab_of(m: &AppModel, id: PaneId) -> Option<usize> {
    m.tabs.iter().position(|t| t.root.contains(id))
}

/// タブ切替時の表示用: アクティブタブのフォーカスペイン (青枠の宛先)。
pub fn is_focused_pane(m: &AppModel, tab: &TabState, id: PaneId) -> bool {
    let _ = m;
    tab.focused == id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msg::PaneMsg;

    fn model() -> AppModel {
        let mut m = AppModel::new(PathBuf::from("C:\\root"));
        // テスト用にダミー entries を持たせる
        let id = m.focused_pane();
        m.panes[id].entries = vec![crate::model::Entry::new(
            "sub".into(),
            true,
            0,
            0,
            None,
            false,
        )];
        m
    }

    #[test]
    fn locked_tab_opens_navigation_in_new_tab() {
        let mut m = model();
        m.tabs[0].locked = true;
        let fx = update_app(&mut m, AppMsg::Focused(PaneMsg::RowDoubleClicked { ix: 0 }));
        // 新タブが増え、元タブのペインは動いていない
        assert_eq!(m.tabs.len(), 2);
        assert_eq!(m.active, 1);
        let old = m.tabs[0].root.first_leaf();
        assert_eq!(m.panes[old].cur_path, PathBuf::from("C:\\root"));
        let new = m.tabs[1].root.first_leaf();
        assert_eq!(m.panes[new].cur_path, PathBuf::from("C:\\root\\sub"));
        assert!(fx.iter().any(|e| matches!(e, Effect::LoadDir { .. })));
        // 新タブはロックされていない
        assert!(!m.tabs[1].locked);
    }

    #[test]
    fn locked_tab_history_shows_message() {
        let mut m = model();
        m.tabs[0].locked = true;
        let id = m.focused_pane();
        m.panes[id].history_back.push(PathBuf::from("C:\\old"));
        let fx = update_app(&mut m, AppMsg::Focused(PaneMsg::GoBack));
        assert!(fx.is_empty()); // 不動作 (表示のみ)
        assert_eq!(
            m.panes[id].status_msg.as_deref(),
            Some("タブはロックされています")
        );
        assert_eq!(m.panes[id].cur_path, PathBuf::from("C:\\root"));
    }

    #[test]
    fn tab_close_emits_pane_closed_for_all_leaves() {
        let mut m = model();
        update_app(&mut m, AppMsg::Tab(TabMsg::Add));
        update_app(&mut m, AppMsg::Tab(TabMsg::SplitFocused(SplitDir::Row)));
        assert_eq!(m.panes.len(), 3);
        let fx = update_app(&mut m, AppMsg::Tab(TabMsg::Close(1)));
        let closed = fx
            .iter()
            .filter(|e| matches!(e, Effect::PaneClosed(_)))
            .count();
        assert_eq!(closed, 2);
        assert_eq!(m.panes.len(), 1);
    }

    #[test]
    fn reorder_tracks_active_tab() {
        let mut m = model();
        update_app(&mut m, AppMsg::Tab(TabMsg::Add)); // tab1
        update_app(&mut m, AppMsg::Tab(TabMsg::Add)); // tab2 (active=2)
        update_app(&mut m, AppMsg::Tab(TabMsg::Reorder { from: 2, to: 0 }));
        assert_eq!(m.active, 0);
        update_app(&mut m, AppMsg::Tab(TabMsg::Reorder { from: 1, to: 2 }));
        assert_eq!(m.active, 0);
        update_app(&mut m, AppMsg::Tab(TabMsg::Reorder { from: 0, to: 2 }));
        assert_eq!(m.active, 2);
    }

    #[test]
    fn click_on_other_pane_moves_focus() {
        let mut m = model();
        let first = m.focused_pane();
        update_app(&mut m, AppMsg::Tab(TabMsg::SplitFocused(SplitDir::Row)));
        let second = m.focused_pane();
        assert_ne!(first, second);
        update_app(&mut m, AppMsg::Pane(first, PaneMsg::BlankPressed));
        assert_eq!(m.focused_pane(), first);
        // F6 巡回
        update_app(&mut m, AppMsg::Tab(TabMsg::CycleFocus));
        assert_eq!(m.focused_pane(), second);
    }

    #[test]
    fn stale_pane_messages_are_dropped() {
        let mut m = model();
        update_app(&mut m, AppMsg::Tab(TabMsg::Add));
        let doomed = m.focused_pane();
        update_app(&mut m, AppMsg::Tab(TabMsg::Close(1)));
        // 閉じたペイン宛の遅延メッセージは無害
        let fx = update_app(&mut m, AppMsg::Pane(doomed, PaneMsg::Reload));
        assert!(fx.is_empty());
    }
}
