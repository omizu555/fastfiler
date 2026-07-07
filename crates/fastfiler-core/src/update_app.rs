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
    /// 内部 D&D (F-601/F-604/F-605)。
    Drag(DragMsg),
}

#[derive(Debug, Clone)]
pub enum DragMsg {
    /// ドラッグ開始 (閾値超)。選択に含まれる行なら選択全体を運ぶ。
    Started {
        pane: PaneId,
        row: usize,
        right_button: bool,
    },
    /// ホバー更新 (行 = Some(フォルダ行) or None = ペイン背景)。
    Hover { pane: PaneId, row: Option<usize> },
    /// ドロップ。修飾キーは GUI 層が焼き込む (F-604)。
    Dropped {
        pane: PaneId,
        row: Option<usize>,
        ctrl: bool,
        shift: bool,
        at: (f32, f32),
        /// ドロップメニュー用コマンド (右ボタン時に GUI 層が採取)。
        commands: Vec<crate::menu::CommandInfo>,
    },
    /// ウィンドウ外リリース等でのキャンセル。
    Cancel,
    /// 外部 (OLE) からのドロップ確定 (F-602)。ペイン解決は GUI 層が行う。
    External {
        pane: PaneId,
        paths: Vec<std::path::PathBuf>,
        /// DROPEFFECT: 1=コピー / 2=移動
        effect: u32,
        /// 右ボタンドロップ (メニューで効果を選ぶ)。
        right_button: bool,
        at: (f32, f32),
        commands: Vec<crate::menu::CommandInfo>,
    },
    /// 外部への OLE ドラッグが MOVE で完了した (F-606: 安全側削除)。
    ExternalMoveDone { paths: Vec<std::path::PathBuf> },
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
            // クリック系はそのペインへフォーカスを移す (F-204)。
            // 右クリック系 (メニュー) も移す — overlay はフォーカスペインのものしか
            // 描画されないため、移さないと非フォーカスペインのフッタ/行/背景の
            // 右クリックメニューが「開いたのに見えない」= 無反応になる (実機報告。
            // 右ドラッグの DropMenu は Dropped 側で focused を移す先例あり)
            if matches!(
                pmsg,
                PaneMsg::RowPressed { .. }
                    | PaneMsg::RowDoubleClicked { .. }
                    | PaneMsg::BlankPressed
                    | PaneMsg::HeaderClicked(_)
                    | PaneMsg::OpenPathEdit
                    | PaneMsg::RightPressed { .. }
                    | PaneMsg::OpenMenu { .. }
                    | PaneMsg::ShellMenuRequest { .. }
            ) {
                if let Some(tix) = tab_of(m, id) {
                    m.active = tix;
                    m.tabs[tix].focused = id;
                }
            }
            let locked = tab_of(m, id).map(|t| m.tabs[t].locked).unwrap_or(false);
            let structural = matches!(pmsg, PaneMsg::ColResized { .. });
            let effects = update_pane(&mut m.panes[id], id, locked, pmsg);
            // DropTransfer は衝突検出込みの転送へ展開 (DropMenu 確定経路)
            let mut expanded = Vec::with_capacity(effects.len());
            for e in effects {
                match e {
                    Effect::DropTransfer {
                        pane,
                        op,
                        paths,
                        dest,
                    } => expanded.extend(start_drop_transfer(m, pane, op, paths, dest)),
                    other => expanded.push(other),
                }
            }
            let mut out = expand_open_tab(m, expanded);
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
        AppMsg::Drag(dmsg) => update_drag(m, dmsg),
    }
}

/// 内部 D&D の状態遷移 (F-601/F-604/F-605)。
fn update_drag(m: &mut AppModel, msg: DragMsg) -> Vec<Effect> {
    use crate::app_model::DragState;
    use crate::transfer::{self, TransferOp};
    match msg {
        DragMsg::Started {
            pane,
            row,
            right_button,
        } => {
            let Some(p) = m.panes.get(pane) else {
                return vec![];
            };
            if p.showing_search() {
                return vec![]; // 検索結果からのドラッグは対象外 (安全側)
            }
            // 選択中の行をドラッグすると選択全体を運ぶ (USAGE.md §2)
            let rows: Vec<usize> = if p.selected.contains(&row) {
                p.selected.iter().copied().collect()
            } else {
                vec![row]
            };
            let paths: Vec<std::path::PathBuf> = rows
                .iter()
                .filter_map(|&i| p.entries.get(i))
                .map(|e| p.cur_path.join(&e.name))
                .collect();
            if paths.is_empty() {
                return vec![];
            }
            m.drag = Some(DragState {
                paths,
                from_pane: pane,
                right_button,
                over: None,
            });
            vec![]
        }
        DragMsg::Hover { pane, row } => {
            // フォルダ行のみハイライト対象 (ファイル行はペイン背景扱い)
            let dir_row = row.filter(|&ix| {
                m.panes
                    .get(pane)
                    .and_then(|p| p.entries.get(ix))
                    .is_some_and(|e| e.is_dir)
            });
            if let Some(d) = &mut m.drag {
                d.over = Some((pane, dir_row));
            }
            vec![]
        }
        DragMsg::Cancel => {
            m.drag = None;
            vec![]
        }
        DragMsg::External {
            pane,
            paths,
            effect,
            right_button,
            at,
            commands,
        } => {
            let Some(p) = m.panes.get_mut(pane) else {
                return vec![];
            };
            let dest = p.cur_path.clone();
            // 自分のフォルダへのドロップは無視 (エクスプローラ同様)
            if paths.iter().all(|src| src.parent() == Some(dest.as_path())) {
                return vec![];
            }
            if right_button {
                let items = crate::menu::build_drop_menu(&commands);
                p.overlay = Some(crate::model::Overlay::DropMenu {
                    items,
                    at,
                    paths,
                    dest,
                });
                m.tabs[m.active].focused = pane;
                return vec![];
            }
            let op = if effect == 2 {
                TransferOp::Move
            } else {
                TransferOp::Copy
            };
            start_drop_transfer_ext(m, pane, op, paths, dest, true)
        }
        DragMsg::ExternalMoveDone { paths } => {
            // 外部への MOVE 完了: PerformedDropEffect==MOVE のときだけ呼ばれる
            // (判定は domain ole_dnd — F-606 の安全側削除)。元をゴミ箱へ送る
            // (完全削除より安全側に倒す。エクスプローラの挙動とも一致)
            vec![Effect::DeleteToTrash { paths }]
        }
        DragMsg::Dropped {
            pane,
            row,
            ctrl,
            shift,
            at,
            commands,
        } => {
            let Some(drag) = m.drag.take() else {
                return vec![];
            };
            let Some(p) = m.panes.get_mut(pane) else {
                return vec![];
            };
            // ドロップ先: フォルダ行ならそのフォルダ、それ以外は表示中フォルダ (F-601)
            let dest = match row.and_then(|ix| p.entries.get(ix)) {
                Some(e) if e.is_dir => p.cur_path.join(&e.name),
                _ => p.cur_path.clone(),
            };
            // 右ボタン: ドロップメニューで効果を選ぶ (F-605)
            if drag.right_button {
                let items = crate::menu::build_drop_menu(&commands);
                p.overlay = Some(crate::model::Overlay::DropMenu {
                    items,
                    at,
                    paths: drag.paths,
                    dest,
                });
                m.tabs[m.active].focused = pane;
                return vec![];
            }
            // 修飾キー規則 (F-604): Ctrl=コピー / Shift=移動 /
            // なし = 同一ドライブなら移動、別ドライブならコピー
            let same_vol = drag
                .paths
                .first()
                .is_some_and(|src| transfer::same_volume(src, &dest));
            let op = if ctrl {
                TransferOp::Copy
            } else if shift || same_vol {
                // Shift 明示 or 同一ドライブの既定 = 移動 (F-604)
                TransferOp::Move
            } else {
                TransferOp::Copy
            };
            start_drop_transfer(m, pane, op, drag.paths, dest)
        }
    }
}

/// ドロップの転送開始 (衝突検出は貼り付けと同じ plan_transfer 経路 — F-503 全経路共通)。
pub(crate) fn start_drop_transfer(
    m: &mut AppModel,
    pane: PaneId,
    op: crate::transfer::TransferOp,
    paths: Vec<std::path::PathBuf>,
    dest: std::path::PathBuf,
) -> Vec<Effect> {
    start_drop_transfer_ext(m, pane, op, paths, dest, false)
}

/// `external_move` = 外部 (OLE) 発の移動: Copy + ソース削除に分解する
/// (受信側 rename はエクスプローラのソースロックと共有違反になる)。
pub(crate) fn start_drop_transfer_ext(
    m: &mut AppModel,
    pane: PaneId,
    op: crate::transfer::TransferOp,
    paths: Vec<std::path::PathBuf>,
    dest: std::path::PathBuf,
    external_move: bool,
) -> Vec<Effect> {
    use crate::transfer::{self, ConflictChoice};
    let Some(p) = m.panes.get_mut(pane) else {
        return vec![];
    };
    // 自己ネストのガード: ドラッグ項目自身/その配下へのドロップは不正 (フォルダを
    // 自分の子孫へ移動/コピーしてしまう)。エクスプローラ同様 no-op にする
    if paths.iter().any(|src| dest.starts_with(src)) {
        return vec![];
    }
    // dest がペインの表示フォルダと違う (フォルダ行ドロップ) 場合、既存名は不明 →
    // 空集合で計画し衝突は上書き確認なしになるため、表示フォルダのときだけ検出する。
    // フォルダ行ドロップの衝突は domain のジョブが上書きで処理 (GPUI 版と同等)。
    let existing: std::collections::BTreeSet<String> = if dest == p.cur_path {
        p.entries.iter().map(|e| e.name.clone()).collect()
    } else {
        Default::default()
    };
    let plan = transfer::plan_transfer(op, &paths, &dest, &existing);
    if plan.items.is_empty() {
        return vec![];
    }
    if plan.conflicts.is_empty() {
        let items = transfer::resolve_conflicts(&plan, ConflictChoice::Overwrite, &existing);
        if external_move && op == transfer::TransferOp::Move {
            return vec![Effect::SpawnExternalMove { pane, items }];
        }
        vec![Effect::SpawnJob { pane, op, items }]
    } else {
        p.overlay = Some(crate::model::Overlay::Conflict { plan });
        m.tabs[m.active].focused = pane;
        vec![]
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
    fn open_menu_on_unfocused_pane_moves_focus() {
        // overlay はフォーカスペインのものしか描画されない — 右クリックメニューを
        // 開くときにフォーカスが移らないと「開いたのに見えない」(実機報告:
        // フッタ右クリックの反応が悪い)
        let mut m = model();
        let first = m.focused_pane();
        update_app(&mut m, AppMsg::Tab(TabMsg::SplitFocused(SplitDir::Row)));
        assert_ne!(m.focused_pane(), first);
        update_app(
            &mut m,
            AppMsg::Pane(
                first,
                PaneMsg::OpenMenu {
                    at: (10.0, 10.0),
                    row: None,
                    templates: vec![],
                    commands: vec![],
                    templates_dir: String::new(),
                    can_paste: false,
                },
            ),
        );
        assert_eq!(m.focused_pane(), first);
        assert!(matches!(
            m.panes[first].overlay,
            Some(crate::model::Overlay::ContextMenu { .. })
        ));
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

    #[test]
    fn drop_into_own_subtree_is_rejected() {
        use crate::transfer::TransferOp;
        let mut m = AppModel::new(std::path::PathBuf::from(r"C:\a"));
        let pane = m.focused_pane();
        // C:\a\sub をドラッグして dest = 自分自身 / 自分の配下
        let src = vec![std::path::PathBuf::from(r"C:\a\sub")];
        let fx = start_drop_transfer(
            &mut m,
            pane,
            TransferOp::Move,
            src.clone(),
            std::path::PathBuf::from(r"C:\a\sub"),
        );
        assert!(fx.is_empty());
        let fx = start_drop_transfer(
            &mut m,
            pane,
            TransferOp::Copy,
            src,
            std::path::PathBuf::from(r"C:\a\sub\inner"),
        );
        assert!(fx.is_empty());
    }

    #[test]
    fn external_right_drop_menu_executes_copy_and_move() {
        use crate::transfer::TransferOp;
        // 再現: エクスプローラからの右ボタンドロップ → チューザー → 転送実行
        let mut m = AppModel::new(std::path::PathBuf::from(r"C:\dest"));
        let pane = m.focused_pane();
        let fx = update_app(
            &mut m,
            AppMsg::Drag(DragMsg::External {
                pane,
                paths: vec![std::path::PathBuf::from(r"E:\src.txt")],
                effect: 1,
                right_button: true,
                at: (10.0, 10.0),
                commands: vec![],
            }),
        );
        assert!(fx.is_empty(), "右ドロップはメニュー表示のみのはず: {fx:?}");
        let Some(crate::model::Overlay::DropMenu { items, .. }) = &m.panes[pane].overlay else {
            panic!("DropMenu が開いていない: {:?}", m.panes[pane].overlay);
        };
        assert_eq!(items[0].label, "ここにコピー");
        assert_eq!(items[1].label, "ここに移動");
        // 「ここにコピー」をクリック → SpawnJob(Copy)
        let fx = update_app(&mut m, AppMsg::Pane(pane, PaneMsg::MenuClicked(vec![0])));
        assert!(
            fx.iter().any(|e| matches!(
                e,
                Effect::SpawnJob {
                    op: TransferOp::Copy,
                    ..
                }
            )),
            "コピーの SpawnJob が出ない: {fx:?}"
        );
        assert!(m.panes[pane].overlay.is_none(), "メニューが閉じていない");
        // 「ここに移動」も同様
        let _ = update_app(
            &mut m,
            AppMsg::Drag(DragMsg::External {
                pane,
                paths: vec![std::path::PathBuf::from(r"E:\src.txt")],
                effect: 1,
                right_button: true,
                at: (10.0, 10.0),
                commands: vec![],
            }),
        );
        let fx = update_app(&mut m, AppMsg::Pane(pane, PaneMsg::MenuClicked(vec![1])));
        assert!(
            fx.iter().any(|e| matches!(
                e,
                Effect::SpawnJob {
                    op: TransferOp::Move,
                    ..
                }
            )),
            "移動の SpawnJob が出ない: {fx:?}"
        );
    }
}
