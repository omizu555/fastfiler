//! 選択モデル (F-403〜F-405。GPUI 版 doc/spec 08 章の cursor/selected/anchor と同一挙動)。
//!
//! - クリック: 単一選択 (cursor=anchor=ix)
//! - Ctrl+クリック: トグル追加/除外
//! - Shift+クリック / Shift+移動: anchor から範囲選択 (置換)
//! - 矢印等の移動: 単一選択に戻る
//! - Ctrl+A: 全選択 / Esc・空白クリック: 選択解除 (カーソルは残す)
//! - reload 後は名前で選択・カーソルを復元 (watcher 自動更新でも選択維持 — USAGE.md §2)

use std::collections::BTreeSet;

use crate::model::PaneState;

/// キーボードナビゲーションの種別。PageUp/Down の移動量は可視行数から計算する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavKey {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

impl PaneState {
    /// 行クリック (左ボタン)。
    pub fn click_row(&mut self, ix: usize, ctrl: bool, shift: bool) {
        if ix >= self.visible_len() {
            return;
        }
        if shift {
            let a = self.anchor.or(self.cursor).unwrap_or(ix);
            self.select_range(a, ix);
            self.cursor = Some(ix);
            self.anchor = Some(a);
        } else if ctrl {
            if self.selected.contains(&ix) {
                self.selected.remove(&ix);
            } else {
                self.selected.insert(ix);
            }
            self.cursor = Some(ix);
            self.anchor = Some(ix);
        } else {
            self.selected = BTreeSet::from([ix]);
            self.cursor = Some(ix);
            self.anchor = Some(ix);
        }
    }

    /// PageUp/PageDown の固定移動量 (GPUI 版 pane.rs:978 と同じ ±10 行)。
    const PAGE: isize = 10;

    /// キーボード移動。shift 押下で anchor からの範囲選択。
    /// カーソル未設定は「-1 行目にいる」扱い (GPUI 版 move_cursor と同じ:
    /// 未設定から Down/PageDown/End はそれぞれ 0 行目 / 9 行目 / 最終行へ)。
    pub fn key_nav(&mut self, key: NavKey, shift: bool) {
        if self.visible_len() == 0 {
            return;
        }
        let last = (self.visible_len() - 1) as isize;
        let base = self.cursor.map(|c| c as isize).unwrap_or(-1);
        let next = match key {
            NavKey::Up => base - 1,
            NavKey::Down => base + 1,
            NavKey::PageUp => base - Self::PAGE,
            NavKey::PageDown => base + Self::PAGE,
            NavKey::Home => 0,
            NavKey::End => last,
        };
        let next = next.clamp(0, last) as usize;
        if shift {
            let a = self.anchor.or(self.cursor).unwrap_or(next);
            self.select_range(a, next);
            self.anchor = Some(a);
        } else {
            self.selected = BTreeSet::from([next]);
            self.anchor = Some(next);
        }
        self.cursor = Some(next);
        self.ensure_cursor_visible();
    }

    /// 全選択 (Ctrl+A)。GPUI 版 pane.rs:672 と同じく anchor=0、
    /// カーソル未設定なら 0 行目に置く (直後の Shift+移動が先頭起点になる)。
    pub fn select_all(&mut self) {
        if self.visible_len() == 0 {
            return;
        }
        self.selected = (0..self.visible_len()).collect();
        self.anchor = Some(0);
        if self.cursor.is_none() {
            self.cursor = Some(0);
        }
    }

    /// 選択解除 (空白クリック / Esc)。GPUI 版 (pane.rs:932, 2094) と同じく
    /// selected / cursor / anchor をすべて解除する。
    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.cursor = None;
        self.anchor = None;
    }

    fn select_range(&mut self, a: usize, b: usize) {
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        self.selected = (lo..=hi.min(self.visible_len().saturating_sub(1))).collect();
    }

    /// 現在の選択・カーソルの名前スナップショット (reload 前に取る)。
    pub fn selection_names(&self) -> (Vec<String>, Option<String>) {
        let names = self
            .selected
            .iter()
            .filter_map(|&i| self.entries.get(i).map(|e| e.name.clone()))
            .collect();
        let cursor_name = self
            .cursor
            .and_then(|i| self.entries.get(i))
            .map(|e| e.name.clone());
        (names, cursor_name)
    }

    /// reload / 再ソート後に名前で選択・カーソルを復元する。
    pub fn restore_selection(&mut self, names: &[String], cursor_name: Option<&str>) {
        let index_of = |name: &str| self.entries.iter().position(|e| e.name == name);
        self.selected = names.iter().filter_map(|n| index_of(n)).collect();
        self.cursor = cursor_name.and_then(index_of);
        self.anchor = self.cursor;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Entry, PaneState};
    use std::path::PathBuf;

    fn pane(n: usize) -> PaneState {
        let mut p = PaneState::new(PathBuf::from("C:\\"));
        p.entries = (0..n)
            .map(|i| {
                Entry::new(
                    format!("f{i:02}.txt"),
                    false,
                    1,
                    0,
                    Some("txt".into()),
                    false,
                )
            })
            .collect();
        p.viewport_h = 240.0; // 10 行
        p
    }

    #[test]
    fn plain_click_selects_single() {
        let mut p = pane(10);
        p.click_row(3, false, false);
        assert_eq!(p.cursor, Some(3));
        assert!(p.selected.iter().eq(&[3]));
        p.click_row(5, false, false);
        assert!(p.selected.iter().eq(&[5]));
    }

    #[test]
    fn ctrl_click_toggles() {
        let mut p = pane(10);
        p.click_row(3, false, false);
        p.click_row(5, true, false);
        assert!(p.selected.iter().eq(&[3, 5]));
        p.click_row(3, true, false);
        assert!(p.selected.iter().eq(&[5]));
        assert_eq!(p.cursor, Some(3));
    }

    #[test]
    fn shift_click_selects_range_from_anchor() {
        let mut p = pane(10);
        p.click_row(2, false, false);
        p.click_row(6, false, true);
        assert!(p.selected.iter().eq(&[2, 3, 4, 5, 6]));
        assert_eq!(p.cursor, Some(6));
        // anchor は 2 のまま → 逆方向へ縮む
        p.click_row(0, false, true);
        assert!(p.selected.iter().eq(&[0, 1, 2]));
    }

    #[test]
    fn arrow_moves_and_resets_selection() {
        let mut p = pane(10);
        p.click_row(4, false, false);
        p.key_nav(NavKey::Down, false);
        assert_eq!(p.cursor, Some(5));
        assert!(p.selected.iter().eq(&[5]));
        p.key_nav(NavKey::Up, true);
        assert!(p.selected.iter().eq(&[4, 5])); // anchor=5 から 4
        assert_eq!(p.cursor, Some(4));
    }

    #[test]
    fn nav_clamps_at_edges_and_pages() {
        let mut p = pane(30);
        p.key_nav(NavKey::Up, false); // カーソル未設定 (-1) → 先頭
        assert_eq!(p.cursor, Some(0));
        p.key_nav(NavKey::Up, false);
        assert_eq!(p.cursor, Some(0));
        p.key_nav(NavKey::PageDown, false); // 固定 10 行 (GPUI パリティ)
        assert_eq!(p.cursor, Some(10));
        p.key_nav(NavKey::End, false);
        assert_eq!(p.cursor, Some(29));
        p.key_nav(NavKey::Down, false);
        assert_eq!(p.cursor, Some(29));
        p.key_nav(NavKey::Home, false);
        assert_eq!(p.cursor, Some(0));
    }

    #[test]
    fn nav_from_no_cursor_matches_gpui() {
        // GPUI 版: 未設定 (-1) から End=最終行 / PageDown=9 行目 / Down=0 行目
        let mut p = pane(30);
        p.key_nav(NavKey::End, false);
        assert_eq!(p.cursor, Some(29));
        let mut p = pane(30);
        p.key_nav(NavKey::PageDown, false);
        assert_eq!(p.cursor, Some(9));
        let mut p = pane(30);
        p.key_nav(NavKey::Down, false);
        assert_eq!(p.cursor, Some(0));
    }

    #[test]
    fn select_all_and_clear() {
        let mut p = pane(5);
        p.click_row(1, false, false);
        p.select_all();
        assert_eq!(p.selected.len(), 5);
        assert_eq!(p.cursor, Some(1)); // 既存カーソルは不変
        assert_eq!(p.anchor, Some(0)); // anchor は先頭 (GPUI パリティ)
        p.clear_selection();
        assert!(p.selected.is_empty());
        assert_eq!(p.cursor, None); // GPUI 版はカーソルも解除
        assert_eq!(p.anchor, None);
        // カーソル未設定での Ctrl+A は 0 行目にカーソルを置く
        p.select_all();
        assert_eq!(p.cursor, Some(0));
    }

    #[test]
    fn restore_selection_by_names_after_reorder() {
        let mut p = pane(5);
        p.click_row(1, false, false);
        p.click_row(3, true, false);
        let (names, cursor) = p.selection_names();
        assert_eq!(names, ["f01.txt", "f03.txt"]);
        // 逆順に並べ替え (reload 相当)
        p.entries.reverse();
        p.restore_selection(&names, cursor.as_deref());
        let sel: Vec<_> = p
            .selected
            .iter()
            .map(|&i| p.entries[i].name.as_str())
            .collect();
        assert_eq!(sel, ["f03.txt", "f01.txt"]);
        assert_eq!(
            p.cursor.map(|i| p.entries[i].name.as_str()),
            Some("f03.txt")
        );
        // 消えた名前は選択から落ちる (f03.txt を削除)
        let ix = p.entries.iter().position(|e| e.name == "f03.txt").unwrap();
        p.entries.remove(ix);
        p.restore_selection(&names, Some("f03.txt"));
        let sel: Vec<_> = p
            .selected
            .iter()
            .map(|&i| p.entries[i].name.as_str())
            .collect();
        assert_eq!(sel, ["f01.txt"]);
        assert_eq!(p.cursor, None);
    }
}
