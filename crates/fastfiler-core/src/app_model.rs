//! アプリ全体の状態 (計画書 §5.2)。タブ + BSP + ペイン表の単一所有。
//!
//! メモリ健全性の構造保証: ペインを閉じる = `panes.remove(id)`。
//! 所有が 1 か所なのでリーク経路が存在しない。`PANES_ALIVE` で実測検証する
//! (ADR 0012 の教訓 — B-3)。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};

use slotmap::SlotMap;

use crate::bsp::{PaneNode, SplitDir};
use crate::model::{PaneId, PaneState};
use crate::tree::TreeState;

/// 生存ペイン数の計装 (GPUI 版 PANES_ALIVE の踏襲)。
/// AppModel::new_pane / remove_pane 経由でのみ増減する。
pub static PANES_ALIVE: AtomicI64 = AtomicI64::new(0);

pub fn panes_alive() -> i64 {
    PANES_ALIVE.load(Ordering::Relaxed)
}

/// 内部 D&D の進行状態 (F-601/F-605)。
#[derive(Debug, Clone, PartialEq)]
pub struct DragState {
    /// 運んでいるパス (選択全体 or 単一行 — USAGE.md §2)。
    pub paths: Vec<std::path::PathBuf>,
    pub from_pane: PaneId,
    /// 右ボタンドラッグ (ドロップ時にメニューで効果を選ぶ — F-605)。
    pub right_button: bool,
    /// 現在ホバー中の (ペイン, フォルダ行)。行ハイライト用。
    pub over: Option<(PaneId, Option<usize>)>,
}

/// 1 タブ = 独立した分割構成 + フォーカスペイン (CONTEXT.md の定義)。
#[derive(Debug)]
pub struct TabState {
    pub root: PaneNode,
    /// フォーカスペイン (操作の宛先。タブ切替で復元される)。
    pub focused: PaneId,
    /// ロック (F-104。中クリックで切替、移動系は新タブへ逃がす)。
    pub locked: bool,
    /// タブ名の元ペイン: 最初の分割元 (未分割ならフォーカスの移動に追従 — F-105)。
    pub name_src: PaneId,
}

/// タブバー幅の範囲 (USAGE.md §2: 100〜600px)。
pub const TAB_W_MIN: f32 = 100.0;
pub const TAB_W_MAX: f32 = 600.0;
pub const DEFAULT_TAB_W: f32 = 200.0;

#[derive(Debug)]
pub struct AppModel {
    pub panes: SlotMap<PaneId, PaneState>,
    pub tabs: Vec<TabState>,
    pub active: usize,
    pub tab_width: f32,
    /// Split id の採番 (タブをまたいで一意)。
    pub next_split_id: u64,
    /// ワークスペースツリー (F-801〜F-803)。
    pub tree: TreeState,
    /// 内部 D&D の状態機械 (計画書 §5.2。§10-7: グローバル static の代替)。
    pub drag: Option<DragState>,
}

impl AppModel {
    /// 1 タブ 1 ペインで開始する。
    pub fn new(start: PathBuf) -> Self {
        let mut m = Self {
            panes: SlotMap::with_key(),
            tabs: Vec::new(),
            active: 0,
            tab_width: DEFAULT_TAB_W,
            next_split_id: 0,
            tree: TreeState::default(),
            drag: None,
        };
        m.add_tab(start);
        m
    }

    pub fn new_pane(&mut self, path: PathBuf) -> PaneId {
        PANES_ALIVE.fetch_add(1, Ordering::Relaxed);
        self.panes.insert(PaneState::new(path))
    }

    /// ペインを表から除く (watcher 等の付随リソース解放は GUI 層が
    /// `removed` の返り値を見て行う)。
    pub fn remove_pane(&mut self, id: PaneId) {
        if self.panes.remove(id).is_some() {
            PANES_ALIVE.fetch_sub(1, Ordering::Relaxed);
        }
    }

    pub fn add_tab(&mut self, path: PathBuf) -> PaneId {
        let pane = self.new_pane(path);
        self.tabs.push(TabState {
            root: PaneNode::Leaf(pane),
            focused: pane,
            locked: false,
            name_src: pane,
        });
        self.active = self.tabs.len() - 1;
        pane
    }

    /// タブを閉じ、含まれる全ペインを解放する。最後の 1 枚は閉じない (F-101)。
    /// 戻り値: 解放したペイン (GUI 層が watcher を止める)。
    pub fn close_tab(&mut self, ix: usize) -> Vec<PaneId> {
        if self.tabs.len() <= 1 || ix >= self.tabs.len() {
            return vec![];
        }
        let tab = self.tabs.remove(ix);
        let leaves = tab.root.leaves();
        for id in &leaves {
            self.remove_pane(*id);
        }
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if ix < self.active {
            self.active -= 1;
        }
        leaves
    }

    pub fn active_tab(&self) -> &TabState {
        &self.tabs[self.active]
    }

    pub fn active_tab_mut(&mut self) -> &mut TabState {
        &mut self.tabs[self.active]
    }

    /// フォーカスペイン (常に有効な id — タブは 1 ペイン以上を持つ)。
    pub fn focused_pane(&self) -> PaneId {
        self.active_tab().focused
    }

    pub fn focused_state_mut(&mut self) -> &mut PaneState {
        let id = self.focused_pane();
        &mut self.panes[id]
    }

    /// フォーカスペインを dir 方向に分割する。ロックタブ内の新ペインはロック継承
    /// (タブ単位ロックなので自動)。戻り値: 新ペイン。
    pub fn split_focused(&mut self, dir: SplitDir) -> PaneId {
        let target = self.focused_pane();
        self.split_pane(target, dir)
            .expect("focused pane は必ず現在タブに含まれる")
    }

    /// 指定ペインを分割する (押されたボタンのペインが分裂する — 実機要望
    /// 2026-09-02。フォーカス側とは限らない)。新ペインは同フォルダで、
    /// フォーカスは新ペインへ移る (split_focused と同じ)。
    /// target が現在タブに無い (閉じた直後の古い ID 等) 場合は None。
    pub fn split_pane(&mut self, target: PaneId, dir: SplitDir) -> Option<PaneId> {
        if !self.tabs[self.active].root.contains(target) {
            return None;
        }
        let path = self.panes[target].cur_path.clone();
        let new = self.new_pane(path);
        let mut next = self.next_split_id;
        let tab = &mut self.tabs[self.active];
        tab.root.split(target, dir, new, &mut next);
        self.next_split_id = next;
        self.tabs[self.active].focused = new;
        Some(new)
    }

    /// ペインを閉じる (タブ内最後の 1 枚は残す — F-202)。
    /// 戻り値: 閉じたら Some(id) (GUI 層が watcher を止める)。
    pub fn close_pane(&mut self, id: PaneId) -> Option<PaneId> {
        let tab = &mut self.tabs[self.active];
        if !tab.root.contains(id) {
            return None;
        }
        if !tab.root.remove(id) {
            return None; // 最後の 1 枚
        }
        self.remove_pane(id);
        let tab = &mut self.tabs[self.active];
        if tab.focused == id {
            tab.focused = tab.root.first_leaf();
        }
        if tab.name_src == id {
            tab.name_src = tab.root.first_leaf();
        }
        Some(id)
    }

    /// F6: 表示順で次のペインへフォーカスを巡回。
    pub fn cycle_focus(&mut self) {
        let tab = self.active_tab();
        let leaves = tab.root.leaves();
        let cur = leaves.iter().position(|&p| p == tab.focused).unwrap_or(0);
        let next = leaves[(cur + 1) % leaves.len()];
        self.active_tab_mut().focused = next;
    }

    /// タブ表示名: name_src ペインのフォルダ名 + 場所プレフィックス
    /// (ローカル `C: 名前` / UNC `🌐 名前` — F-105)。
    pub fn tab_title(&self, ix: usize) -> String {
        let tab = &self.tabs[ix];
        let path = &self.panes[tab.name_src].cur_path;
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        let prefix = location_prefix(path);
        let lock = if tab.locked { "🔒 " } else { "" };
        format!("{lock}{prefix}{name}")
    }
}

/// パスの場所プレフィックス: `C: ` (ドライブ) / `🌐 ` (UNC)。
fn location_prefix(path: &Path) -> String {
    let s = path.to_string_lossy();
    if s.starts_with("\\\\") {
        "🌐 ".to_string()
    } else if let Some(drive) = s.chars().next().filter(|c| c.is_ascii_alphabetic()) {
        if s.get(1..2) == Some(":") {
            return format!("{}: ", drive.to_ascii_uppercase());
        }
        format!("{drive}: ")
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// PANES_ALIVE はプロセス全体の static のため、カウンタを検証するテストは
    /// 直列化する (並列実行だと他テストの生成分で基準がずれる)。
    static COUNTER_LOCK: Mutex<()> = Mutex::new(());

    fn model() -> AppModel {
        AppModel::new(PathBuf::from("C:\\root"))
    }

    #[test]
    fn panes_alive_returns_to_baseline_after_tab_churn() {
        let _g = COUNTER_LOCK.lock().unwrap();
        let base = panes_alive();
        let mut m = model();
        assert_eq!(panes_alive(), base + 1);
        // 50 回 タブ追加 + 分割 + 分割 + タブ閉じ
        for _ in 0..50 {
            m.add_tab(PathBuf::from("C:\\a"));
            m.split_focused(SplitDir::Row);
            m.split_focused(SplitDir::Column);
            let closed = m.close_tab(m.active);
            assert_eq!(closed.len(), 3);
        }
        assert_eq!(panes_alive(), base + 1); // ベースライン復帰 (B-3 の core 版)
        assert_eq!(m.panes.len(), 1);
        drop(m);
    }

    #[test]
    fn close_tab_keeps_last_and_fixes_active() {
        let _g = COUNTER_LOCK.lock().unwrap();
        let mut m = model();
        assert!(m.close_tab(0).is_empty()); // 最後の 1 枚は閉じない
        m.add_tab(PathBuf::from("C:\\b"));
        m.add_tab(PathBuf::from("C:\\c"));
        m.active = 2;
        m.close_tab(2);
        assert_eq!(m.active, 1);
        m.active = 0;
        m.close_tab(1);
        assert_eq!(m.active, 0);
        assert_eq!(m.tabs.len(), 1);
    }

    #[test]
    fn split_and_close_pane_moves_focus_and_name_src() {
        let _g = COUNTER_LOCK.lock().unwrap();
        let mut m = model();
        let first = m.focused_pane();
        let second = m.split_focused(SplitDir::Row);
        assert_eq!(m.focused_pane(), second);
        assert_eq!(m.active_tab().name_src, first); // タブ名は最初の分割元固定
                                                    // フォーカス中の second を閉じる → first へ戻る
        assert_eq!(m.close_pane(second), Some(second));
        assert_eq!(m.focused_pane(), first);
        // 最後の 1 枚は閉じない
        assert_eq!(m.close_pane(first), None);
        assert_eq!(m.panes.len(), 1);
    }

    #[test]
    fn cycle_focus_wraps_in_display_order() {
        let _g = COUNTER_LOCK.lock().unwrap();
        let mut m = model();
        let p0 = m.focused_pane();
        let p1 = m.split_focused(SplitDir::Row);
        let p2 = m.split_focused(SplitDir::Column);
        assert_eq!(m.focused_pane(), p2);
        m.cycle_focus();
        // 表示順: p0, p1, p2 → p2 の次は p0
        assert_eq!(m.focused_pane(), p0);
        m.cycle_focus();
        assert_eq!(m.focused_pane(), p1);
    }

    #[test]
    fn tab_title_has_location_prefix_and_lock() {
        let _g = COUNTER_LOCK.lock().unwrap();
        let mut m = model();
        assert_eq!(m.tab_title(0), "C: root");
        m.tabs[0].locked = true;
        assert_eq!(m.tab_title(0), "🔒 C: root");
        let m2 = AppModel::new(PathBuf::from("\\\\server\\share\\docs"));
        assert_eq!(m2.tab_title(0), "🌐 docs");
    }
}
