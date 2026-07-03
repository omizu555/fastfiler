//! ペインの純 reducer (計画書 §5.3)。I/O 禁止 — 副作用は `Effect` で返す。

use crate::domain_event::DomainEvent;
use crate::effect::Effect;
use crate::model::{sort_entries, Column, Overlay, PaneState, COL_W_MAX, COL_W_MIN};
use crate::msg::PaneMsg;

/// watcher 変化 → reload のデバウンス間隔 (現行値を変えない — LESSONS/計画書)。
pub const RELOAD_DEBOUNCE_MS: u64 = 150;

/// ペインへの入力を状態遷移 + 副作用列に変換する。
pub fn update_pane(p: &mut PaneState, msg: PaneMsg) -> Vec<Effect> {
    // オーバーレイ表示中は、一覧向けのキー操作を横取りしない (§10-3 の 2 段ディスパッチ)
    if p.overlay.is_some() {
        if let Some(r) = update_overlay(p, &msg) {
            return r;
        }
    }
    match msg {
        PaneMsg::Loaded {
            generation,
            entries,
        } => {
            if generation != self_gen(p) {
                return vec![]; // 古い世代 (キャンセル済み) は捨てる
            }
            let (names, cursor_name) = p.selection_names();
            // 親へ戻った直後などは pending の名前を優先してカーソルを合わせる
            let cursor_name = p.pending_cursor_name.take().or(cursor_name);
            p.entries = entries;
            sort_entries(&mut p.entries, p.sort);
            p.loading = false;
            p.load_error = None;
            // watcher/reload でも選択は名前で維持 (USAGE.md §2)
            p.restore_selection(&names, cursor_name.as_deref());
            p.ensure_cursor_visible();
            p.scroll_offset = p.scroll_offset.clamp(0.0, p.max_scroll());
            vec![]
        }
        PaneMsg::LoadFailed { generation, error } => {
            if generation == self_gen(p) {
                p.loading = false;
                p.load_error = Some(error);
            }
            vec![]
        }
        PaneMsg::RowPressed { ix, ctrl, shift } => {
            p.click_row(ix, ctrl, shift);
            vec![]
        }
        PaneMsg::RowDoubleClicked { ix } => activate(p, ix),
        PaneMsg::ActivateCursor => match p.cursor {
            Some(ix) => activate(p, ix),
            None => vec![],
        },
        PaneMsg::GoParent => {
            let from = p.cur_path.clone();
            match p.cur_path.parent() {
                Some(parent) => {
                    let parent = parent.to_path_buf();
                    let effects = navigate(p, parent);
                    // 親へ戻ったら、元いたフォルダにカーソルを合わせたいが
                    // 一覧はまだ無い → Loaded 後の復元に載せる (名前だけ先に設定)。
                    if let Some(name) = from.file_name().map(|s| s.to_string_lossy().to_string()) {
                        p.pending_cursor_name = Some(name);
                    }
                    effects
                }
                None => vec![],
            }
        }
        PaneMsg::HeaderClicked(col) => {
            if p.sort.col == col {
                p.sort.asc = !p.sort.asc;
            } else {
                p.sort.col = col;
                p.sort.asc = true;
            }
            let (names, cursor_name) = p.selection_names();
            sort_entries(&mut p.entries, p.sort);
            p.restore_selection(&names, cursor_name.as_deref());
            p.ensure_cursor_visible();
            vec![]
        }
        PaneMsg::ColResized { col, width } => {
            let w = width.clamp(COL_W_MIN, COL_W_MAX);
            match col {
                Column::Modified => p.col_widths[0] = w,
                Column::Size => p.col_widths[1] = w,
                Column::Type => p.col_widths[2] = w,
                Column::Name => {} // 名前列は残り吸収なのでリサイズ対象外
            }
            vec![]
        }
        PaneMsg::Scrolled(offset) => {
            p.scroll_offset = offset.clamp(0.0, p.max_scroll());
            vec![]
        }
        PaneMsg::ViewportChanged { height } => {
            p.viewport_h = height;
            p.scroll_offset = p.scroll_offset.clamp(0.0, p.max_scroll());
            vec![]
        }
        PaneMsg::Nav(key, shift) => {
            p.key_nav(key, shift);
            vec![]
        }
        PaneMsg::SelectAll => {
            p.select_all();
            vec![]
        }
        PaneMsg::ClearSelection | PaneMsg::BlankPressed => {
            p.clear_selection();
            vec![]
        }
        PaneMsg::GoBack => {
            let Some(prev) = p.history_back.pop() else {
                return vec![];
            };
            p.history_fwd.push(p.cur_path.clone());
            set_path_and_load(p, prev)
        }
        PaneMsg::GoForward => {
            let Some(next) = p.history_fwd.pop() else {
                return vec![];
            };
            p.history_back.push(p.cur_path.clone());
            set_path_and_load(p, next)
        }
        PaneMsg::Reload => reload(p),
        PaneMsg::OpenPathEdit => {
            p.overlay = Some(Overlay::PathEdit {
                value: p.cur_path.to_string_lossy().to_string(),
            });
            vec![]
        }
        // overlay なし状態で届いた入力系は無視 (update_overlay が正規経路)
        PaneMsg::PathEditInput(_) | PaneMsg::PathEditCommit | PaneMsg::PathEditCancel => vec![],
        PaneMsg::Domain(ev) => match ev {
            DomainEvent::FsChange { path } => {
                // 監視は現在フォルダのみだが、遅れて届く旧パスのイベントは無視
                if path != p.cur_path.to_string_lossy() {
                    return vec![];
                }
                p.reload_seq += 1;
                vec![Effect::Debounce {
                    seq: p.reload_seq,
                    millis: RELOAD_DEBOUNCE_MS,
                }]
            }
            // ジョブ進捗/完了は Phase 2b で配線
            _ => vec![],
        },
        PaneMsg::ReloadTick(seq) => {
            if seq == p.reload_seq {
                reload(p)
            } else {
                vec![] // より新しい変化が控えている (デバウンス継続)
            }
        }
    }
}

/// オーバーレイ表示中のメッセージ処理。`None` を返すと通常処理へフォールスルー。
fn update_overlay(p: &mut PaneState, msg: &PaneMsg) -> Option<Vec<Effect>> {
    let Some(Overlay::PathEdit { value }) = &mut p.overlay else {
        return None;
    };
    match msg {
        PaneMsg::PathEditInput(v) => {
            *value = v.clone();
            Some(vec![])
        }
        PaneMsg::PathEditCommit => {
            let target = std::path::PathBuf::from(value.trim());
            p.overlay = None;
            if target.as_os_str().is_empty() || target == p.cur_path {
                return Some(vec![]);
            }
            Some(navigate(p, target))
        }
        PaneMsg::PathEditCancel | PaneMsg::ClearSelection => {
            p.overlay = None;
            Some(vec![])
        }
        // 入力中でも一覧の読み込み結果・スクロール・ビューポートは通す
        PaneMsg::Loaded { .. }
        | PaneMsg::LoadFailed { .. }
        | PaneMsg::Scrolled(_)
        | PaneMsg::ViewportChanged { .. }
        | PaneMsg::Domain(_)
        | PaneMsg::ReloadTick(_) => None,
        // それ以外の一覧操作 (キーナビ・クリック等) はオーバーレイ中は無効
        _ => Some(vec![]),
    }
}

fn self_gen(p: &PaneState) -> u64 {
    p.load_gen
}

/// 行の活性化: フォルダ = 移動 / ファイル = 既定アプリで開く (F-301)。
fn activate(p: &mut PaneState, ix: usize) -> Vec<Effect> {
    let Some(entry) = p.entries.get(ix) else {
        return vec![];
    };
    let target = p.cur_path.join(&entry.name);
    if entry.is_dir {
        navigate(p, target)
    } else {
        vec![Effect::OpenFile { path: target }]
    }
}

/// フォルダ移動: 履歴に現在地を積んでから移動する (F-303)。
pub fn navigate(p: &mut PaneState, path: std::path::PathBuf) -> Vec<Effect> {
    if path != p.cur_path {
        p.history_back.push(p.cur_path.clone());
        p.history_fwd.clear();
    }
    set_path_and_load(p, path)
}

/// 履歴操作用: 履歴を触らずにパスを差し替えて読み込む。
fn set_path_and_load(p: &mut PaneState, path: std::path::PathBuf) -> Vec<Effect> {
    p.cur_path = path.clone();
    p.cursor = None;
    p.anchor = None;
    p.selected.clear();
    p.pending_cursor_name = None;
    p.scroll_offset = 0.0;
    start_load(p, path)
}

/// F5 / watcher の再読み込み: 選択・カーソル・スクロールを維持したまま読み直す
/// (選択は Loaded 側で名前復元される — USAGE.md §2)。
pub fn reload(p: &mut PaneState) -> Vec<Effect> {
    start_load(p, p.cur_path.clone())
}

fn start_load(p: &mut PaneState, path: std::path::PathBuf) -> Vec<Effect> {
    p.load_gen += 1;
    p.loading = true;
    p.load_error = None;
    vec![Effect::LoadDir {
        generation: p.load_gen,
        path,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Entry, SortState};
    use std::path::PathBuf;

    fn entry(name: &str, is_dir: bool) -> Entry {
        let ext = (!is_dir)
            .then(|| name.rsplit_once('.').map(|(_, e)| e.to_string()))
            .flatten();
        Entry::new(name.to_string(), is_dir, 1, 100, ext, false)
    }

    fn pane_with(names: &[(&str, bool)]) -> PaneState {
        let mut p = PaneState::new(PathBuf::from("C:\\root"));
        p.entries = names.iter().map(|(n, d)| entry(n, *d)).collect();
        p.viewport_h = 240.0;
        p
    }

    #[test]
    fn double_click_dir_navigates_and_bumps_generation() {
        let mut p = pane_with(&[("sub", true), ("a.txt", false)]);
        let fx = update_pane(&mut p, PaneMsg::RowDoubleClicked { ix: 0 });
        assert_eq!(p.cur_path, PathBuf::from("C:\\root\\sub"));
        assert!(p.loading);
        assert_eq!(
            fx,
            vec![Effect::LoadDir {
                generation: 1,
                path: PathBuf::from("C:\\root\\sub")
            }]
        );
    }

    #[test]
    fn double_click_file_opens() {
        let mut p = pane_with(&[("a.txt", false)]);
        let fx = update_pane(&mut p, PaneMsg::RowDoubleClicked { ix: 0 });
        assert_eq!(
            fx,
            vec![Effect::OpenFile {
                path: PathBuf::from("C:\\root\\a.txt")
            }]
        );
        assert!(!p.loading);
    }

    #[test]
    fn stale_generation_result_is_dropped() {
        let mut p = pane_with(&[]);
        // 2 回ナビゲート → gen=2。gen=1 の結果は捨てる
        navigate(&mut p, PathBuf::from("C:\\a"));
        navigate(&mut p, PathBuf::from("C:\\b"));
        let fx = update_pane(
            &mut p,
            PaneMsg::Loaded {
                generation: 1,
                entries: vec![entry("stale.txt", false)],
            },
        );
        assert!(fx.is_empty());
        assert!(p.entries.is_empty());
        assert!(p.loading);
        // 現世代は反映される
        update_pane(
            &mut p,
            PaneMsg::Loaded {
                generation: 2,
                entries: vec![entry("fresh.txt", false)],
            },
        );
        assert_eq!(p.entries[0].name, "fresh.txt");
        assert!(!p.loading);
    }

    #[test]
    fn loaded_sorts_and_keeps_selection_by_name() {
        let mut p = pane_with(&[("b.txt", false), ("a.txt", false)]);
        p.click_row(0, false, false); // b.txt を選択
        let fx = update_pane(
            &mut p,
            PaneMsg::Loaded {
                generation: 0,
                entries: vec![
                    entry("b.txt", false),
                    entry("dir", true),
                    entry("a.txt", false),
                ],
            },
        );
        assert!(fx.is_empty());
        let names: Vec<_> = p.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["dir", "a.txt", "b.txt"]); // dir 先頭 + 名前順
        let sel: Vec<_> = p
            .selected
            .iter()
            .map(|&i| p.entries[i].name.as_str())
            .collect();
        assert_eq!(sel, ["b.txt"]); // 名前で維持
    }

    #[test]
    fn go_parent_sets_pending_cursor_to_source_folder() {
        let mut p = pane_with(&[]);
        let fx = update_pane(&mut p, PaneMsg::GoParent);
        assert_eq!(p.cur_path, PathBuf::from("C:\\"));
        assert_eq!(p.pending_cursor_name.as_deref(), Some("root"));
        assert_eq!(fx.len(), 1);
        // Loaded で root にカーソルが乗る
        let generation = p.load_gen;
        update_pane(
            &mut p,
            PaneMsg::Loaded {
                generation,
                entries: vec![entry("other", true), entry("root", true)],
            },
        );
        assert_eq!(p.cursor.map(|i| p.entries[i].name.as_str()), Some("root"));
    }

    #[test]
    fn header_click_toggles_and_switches_column() {
        let mut p = pane_with(&[("a.txt", false), ("b.txt", false)]);
        update_pane(&mut p, PaneMsg::HeaderClicked(Column::Name));
        assert_eq!(
            p.sort,
            SortState {
                col: Column::Name,
                asc: false
            }
        );
        assert_eq!(p.entries[0].name, "b.txt");
        update_pane(&mut p, PaneMsg::HeaderClicked(Column::Size));
        assert_eq!(
            p.sort,
            SortState {
                col: Column::Size,
                asc: true
            }
        );
    }

    #[test]
    fn col_resize_clamps_and_ignores_name() {
        let mut p = pane_with(&[]);
        update_pane(
            &mut p,
            PaneMsg::ColResized {
                col: Column::Size,
                width: 10.0,
            },
        );
        assert_eq!(p.col_widths[1], COL_W_MIN);
        update_pane(
            &mut p,
            PaneMsg::ColResized {
                col: Column::Modified,
                width: 9999.0,
            },
        );
        assert_eq!(p.col_widths[0], COL_W_MAX);
        let before = p.col_widths;
        update_pane(
            &mut p,
            PaneMsg::ColResized {
                col: Column::Name,
                width: 100.0,
            },
        );
        assert_eq!(p.col_widths, before);
    }

    #[test]
    fn history_back_and_forward_roundtrip() {
        let mut p = pane_with(&[("sub", true)]);
        // C:\root → C:\root\sub → 戻る → 進む
        update_pane(&mut p, PaneMsg::RowDoubleClicked { ix: 0 });
        assert_eq!(p.history_back, vec![PathBuf::from("C:\\root")]);
        let fx = update_pane(&mut p, PaneMsg::GoBack);
        assert_eq!(p.cur_path, PathBuf::from("C:\\root"));
        assert_eq!(p.history_fwd, vec![PathBuf::from("C:\\root\\sub")]);
        assert_eq!(fx.len(), 1);
        update_pane(&mut p, PaneMsg::GoForward);
        assert_eq!(p.cur_path, PathBuf::from("C:\\root\\sub"));
        assert!(p.history_fwd.is_empty());
        // 履歴が空なら何もしない
        let mut q = pane_with(&[]);
        assert!(update_pane(&mut q, PaneMsg::GoBack).is_empty());
        assert!(update_pane(&mut q, PaneMsg::GoForward).is_empty());
    }

    #[test]
    fn navigate_clears_forward_history() {
        let mut p = pane_with(&[("a", true), ("b", true)]);
        update_pane(&mut p, PaneMsg::RowDoubleClicked { ix: 0 }); // → a
        update_pane(&mut p, PaneMsg::GoBack); // → root (fwd=[a])
        assert_eq!(p.history_fwd.len(), 1);
        // ここで別フォルダへ移動すると進む履歴は消える
        p.entries = vec![entry("b", true)];
        update_pane(&mut p, PaneMsg::RowDoubleClicked { ix: 0 }); // → b
        assert!(p.history_fwd.is_empty());
    }

    #[test]
    fn reload_preserves_selection_and_scroll() {
        let mut p = pane_with(&[("a.txt", false), ("b.txt", false)]);
        p.click_row(1, false, false);
        p.scroll_offset = 12.0;
        p.viewport_h = 24.0;
        let fx = update_pane(&mut p, PaneMsg::Reload);
        assert!(matches!(fx[0], Effect::LoadDir { generation: 1, .. }));
        assert_eq!(p.scroll_offset, 12.0); // reload はスクロールを保つ
        let generation = p.load_gen;
        update_pane(
            &mut p,
            PaneMsg::Loaded {
                generation,
                entries: vec![entry("b.txt", false), entry("a.txt", false)],
            },
        );
        // 選択は名前で維持される
        let sel: Vec<_> = p
            .selected
            .iter()
            .map(|&i| p.entries[i].name.as_str())
            .collect();
        assert_eq!(sel, ["b.txt"]);
    }

    #[test]
    fn fs_change_debounces_and_only_latest_tick_reloads() {
        use crate::domain_event::DomainEvent;
        let mut p = pane_with(&[]);
        let cur = p.cur_path.to_string_lossy().to_string();
        let fx1 = update_pane(
            &mut p,
            PaneMsg::Domain(DomainEvent::FsChange { path: cur.clone() }),
        );
        assert_eq!(
            fx1,
            vec![Effect::Debounce {
                seq: 1,
                millis: 150
            }]
        );
        let fx2 = update_pane(&mut p, PaneMsg::Domain(DomainEvent::FsChange { path: cur }));
        assert_eq!(
            fx2,
            vec![Effect::Debounce {
                seq: 2,
                millis: 150
            }]
        );
        // 古い tick は無視、新しい tick で reload
        assert!(update_pane(&mut p, PaneMsg::ReloadTick(1)).is_empty());
        let fx3 = update_pane(&mut p, PaneMsg::ReloadTick(2));
        assert!(matches!(fx3[0], Effect::LoadDir { .. }));
        // 監視外パスの変化は無視
        let fx4 = update_pane(
            &mut p,
            PaneMsg::Domain(DomainEvent::FsChange {
                path: "D:\\other".into(),
            }),
        );
        assert!(fx4.is_empty());
    }

    #[test]
    fn path_edit_overlay_flow() {
        let mut p = pane_with(&[("x.txt", false)]);
        update_pane(&mut p, PaneMsg::OpenPathEdit);
        assert!(matches!(p.overlay, Some(Overlay::PathEdit { .. })));
        // オーバーレイ中は一覧キー操作が無効
        update_pane(&mut p, PaneMsg::Nav(crate::NavKey::Down, false));
        assert_eq!(p.cursor, None);
        // 入力 → 確定で navigate
        update_pane(&mut p, PaneMsg::PathEditInput("D:\\data".into()));
        let fx = update_pane(&mut p, PaneMsg::PathEditCommit);
        assert!(p.overlay.is_none());
        assert_eq!(p.cur_path, PathBuf::from("D:\\data"));
        assert_eq!(fx.len(), 1);
        assert_eq!(p.history_back.last(), Some(&PathBuf::from("C:\\root")));
        // Esc (ClearSelection) はオーバーレイを閉じるだけ
        update_pane(&mut p, PaneMsg::OpenPathEdit);
        update_pane(&mut p, PaneMsg::ClearSelection);
        assert!(p.overlay.is_none());
        assert_eq!(p.cur_path, PathBuf::from("D:\\data"));
        // 同一パスで確定 → 移動しない
        update_pane(&mut p, PaneMsg::OpenPathEdit);
        let fx = update_pane(&mut p, PaneMsg::PathEditCommit);
        assert!(fx.is_empty());
    }

    #[test]
    fn scroll_clamps_to_content() {
        let mut p = pane_with(&[("a.txt", false), ("b.txt", false)]);
        p.viewport_h = 24.0; // 1 行分
        update_pane(&mut p, PaneMsg::Scrolled(9999.0));
        assert_eq!(p.scroll_offset, 24.0); // 2 行 - 1 行分
        update_pane(&mut p, PaneMsg::Scrolled(-5.0));
        assert_eq!(p.scroll_offset, 0.0);
    }
}
