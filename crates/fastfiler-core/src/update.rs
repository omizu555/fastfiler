//! ペインの純 reducer (計画書 §5.3)。I/O 禁止 — 副作用は `Effect` で返す。

use crate::domain_event::DomainEvent;
use crate::effect::Effect;
use crate::model::{
    sort_entries, Column, JobStatus, ModalKind, Overlay, PaneState, COL_W_MAX, COL_W_MIN,
};
use crate::msg::PaneMsg;
use crate::transfer::{self, ConflictChoice, TransferOp};

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
        PaneMsg::ClearSelection => {
            // Esc の優先順位: 実行中ジョブのキャンセル > 選択解除 (USAGE.md 固定キー表)
            if let Some(job) = &p.job {
                return vec![Effect::CancelJob { id: job.id }];
            }
            p.status_msg = None;
            p.clear_selection();
            vec![]
        }
        PaneMsg::BlankPressed => {
            p.status_msg = None;
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
            DomainEvent::JobProgress {
                job_id,
                kind,
                done_files,
                total_files,
                done_bytes,
                total_bytes,
                current,
            } => {
                p.job = Some(JobStatus {
                    id: job_id,
                    kind,
                    done_files,
                    total_files,
                    done_bytes,
                    total_bytes,
                    current,
                });
                vec![]
            }
            DomainEvent::JobDone {
                ok,
                canceled,
                error,
                ..
            } => {
                p.job = None;
                p.status_msg = if canceled {
                    Some("キャンセルしました".into())
                } else if let Some(e) = error {
                    Some(format!("エラー: {e}"))
                } else if !ok {
                    Some("一部の項目を処理できませんでした".into())
                } else {
                    None // 成功は無言
                };
                // watcher が効かないフォルダ (ネットワークドライブ等) でも結果を
                // 反映させるため明示 reload (GPUI 版パリティ。watcher と二重に
                // なっても世代キャンセルで最後の 1 回だけが反映される)
                reload(p)
            }
            DomainEvent::Unknown { .. } => vec![],
        },
        PaneMsg::ReloadTick(seq) => {
            if seq == p.reload_seq {
                reload(p)
            } else {
                vec![] // より新しい変化が控えている (デバウンス継続)
            }
        }
        PaneMsg::OpenRename => {
            let Some(name) = p
                .cursor
                .and_then(|i| p.entries.get(i))
                .map(|e| e.name.clone())
            else {
                return vec![];
            };
            p.overlay = Some(Overlay::Modal {
                kind: ModalKind::Rename {
                    original: name.clone(),
                },
                value: name,
            });
            vec![]
        }
        PaneMsg::OpenNewFolder => {
            p.overlay = Some(Overlay::Modal {
                kind: ModalKind::NewFolder,
                value: String::new(),
            });
            vec![]
        }
        PaneMsg::OpenNewFile => {
            p.overlay = Some(Overlay::Modal {
                kind: ModalKind::NewFile,
                value: String::new(),
            });
            vec![]
        }
        // overlay なし状態で届いたモーダル系は無視
        PaneMsg::ModalInput(_) | PaneMsg::ModalCommit | PaneMsg::ModalCancel => vec![],
        PaneMsg::Conflict(_) => vec![],
        PaneMsg::RequestCopy => clipboard_write(p, "copy"),
        PaneMsg::RequestCut => clipboard_write(p, "cut"),
        PaneMsg::RequestPaste => vec![Effect::ClipboardRead],
        PaneMsg::PasteRead { paths, op } => {
            let op = if op == "cut" {
                TransferOp::Move
            } else {
                TransferOp::Copy
            };
            let sources: Vec<std::path::PathBuf> =
                paths.into_iter().map(std::path::PathBuf::from).collect();
            let existing = existing_names(p);
            let plan = transfer::plan_transfer(op, &sources, &p.cur_path, &existing);
            if plan.items.is_empty() {
                return vec![];
            }
            if plan.conflicts.is_empty() {
                let items =
                    transfer::resolve_conflicts(&plan, ConflictChoice::Overwrite, &existing);
                vec![Effect::SpawnJob { op, items }]
            } else {
                p.overlay = Some(Overlay::Conflict { plan });
                vec![]
            }
        }
        PaneMsg::RequestDelete => {
            let paths: Vec<std::path::PathBuf> = p
                .selected
                .iter()
                .filter_map(|&i| p.entries.get(i))
                .map(|e| p.cur_path.join(&e.name))
                .collect();
            if paths.is_empty() {
                return vec![];
            }
            vec![Effect::DeleteToTrash { paths }]
        }
        PaneMsg::Undo => vec![Effect::PerformUndo],
        PaneMsg::StatusMsg(s) => {
            p.status_msg = Some(s);
            vec![]
        }
        PaneMsg::CancelJobRequest => match &p.job {
            Some(job) => vec![Effect::CancelJob { id: job.id }],
            None => vec![],
        },
    }
}

/// 選択パスをクリップボードへ (選択が空なら何もしない)。
fn clipboard_write(p: &PaneState, op: &str) -> Vec<Effect> {
    let paths: Vec<std::path::PathBuf> = p
        .selected
        .iter()
        .filter_map(|&i| p.entries.get(i))
        .map(|e| p.cur_path.join(&e.name))
        .collect();
    if paths.is_empty() {
        return vec![];
    }
    vec![Effect::ClipboardWrite {
        paths,
        op: op.to_string(),
    }]
}

fn existing_names(p: &PaneState) -> std::collections::BTreeSet<String> {
    p.entries.iter().map(|e| e.name.clone()).collect()
}

/// オーバーレイ表示中のメッセージ処理。`None` を返すと通常処理へフォールスルー。
fn update_overlay(p: &mut PaneState, msg: &PaneMsg) -> Option<Vec<Effect>> {
    // どのオーバーレイでも通すもの (読み込み結果・幾何・domain イベント)
    if matches!(
        msg,
        PaneMsg::Loaded { .. }
            | PaneMsg::LoadFailed { .. }
            | PaneMsg::Scrolled(_)
            | PaneMsg::ViewportChanged { .. }
            | PaneMsg::Domain(_)
            | PaneMsg::ReloadTick(_)
            | PaneMsg::StatusMsg(_)
            | PaneMsg::CancelJobRequest
    ) {
        return None;
    }
    match p.overlay.as_mut()? {
        Overlay::PathEdit { value } => match msg {
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
            _ => Some(vec![]),
        },
        Overlay::Modal { kind, value } => match msg {
            PaneMsg::ModalInput(v) => {
                *value = v.clone();
                Some(vec![])
            }
            PaneMsg::ModalCommit => {
                let name = value.trim().to_string();
                // 空・パス区切りを含む名前は確定させない (モーダルは開いたまま)
                if name.is_empty() || name.contains(['\\', '/']) {
                    return Some(vec![]);
                }
                // 新規ファイル/リネームで既存名は確定拒否 (GPUI 版は create_new /
                // no_overwrite でエラーにする — 事前検査で同じ結果に。F7 は冪等なので許容)
                let clashes = p.entries.iter().any(|e| e.name == name);
                match kind {
                    ModalKind::NewFile if clashes => {
                        p.status_msg = Some(format!("「{name}」は既に存在します"));
                        return Some(vec![]);
                    }
                    ModalKind::Rename { original } if clashes && name != *original => {
                        p.status_msg = Some(format!("「{name}」は既に存在します"));
                        return Some(vec![]);
                    }
                    _ => {}
                }
                let kind = kind.clone();
                p.overlay = None;
                Some(commit_modal(p, kind, name))
            }
            PaneMsg::ModalCancel | PaneMsg::ClearSelection => {
                p.overlay = None;
                Some(vec![])
            }
            _ => Some(vec![]),
        },
        Overlay::Conflict { plan } => match msg {
            PaneMsg::Conflict(choice) => {
                let plan = plan.clone();
                p.overlay = None;
                if *choice == ConflictChoice::Cancel {
                    return Some(vec![]);
                }
                let existing = existing_names(p);
                let items = transfer::resolve_conflicts(&plan, *choice, &existing);
                if items.is_empty() {
                    return Some(vec![]);
                }
                Some(vec![Effect::SpawnJob { op: plan.op, items }])
            }
            PaneMsg::ClearSelection => {
                p.overlay = None;
                Some(vec![])
            }
            _ => Some(vec![]),
        },
    }
}

/// 入力モーダルの確定 (F2/F7/F8)。作成/リネーム後は watcher reload で
/// その名前へカーソルが乗るよう pending_cursor_name を仕込む。
fn commit_modal(p: &mut PaneState, kind: ModalKind, name: String) -> Vec<Effect> {
    match kind {
        ModalKind::Rename { original } => {
            if name == original {
                return vec![];
            }
            p.pending_cursor_name = Some(name.clone());
            vec![Effect::Rename {
                from: p.cur_path.join(original),
                to: p.cur_path.join(name),
            }]
        }
        ModalKind::NewFolder => {
            p.pending_cursor_name = Some(name.clone());
            vec![Effect::CreateDir {
                path: p.cur_path.join(name),
            }]
        }
        ModalKind::NewFile => {
            p.pending_cursor_name = Some(name.clone());
            vec![Effect::CreateFile {
                dir: p.cur_path.clone(),
                name,
            }]
        }
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
    fn rename_modal_commit_emits_effect_and_pending_cursor() {
        let mut p = pane_with(&[("old.txt", false)]);
        p.click_row(0, false, false);
        update_pane(&mut p, PaneMsg::OpenRename);
        assert!(matches!(p.overlay, Some(Overlay::Modal { .. })));
        update_pane(&mut p, PaneMsg::ModalInput("new.txt".into()));
        let fx = update_pane(&mut p, PaneMsg::ModalCommit);
        assert_eq!(
            fx,
            vec![Effect::Rename {
                from: PathBuf::from("C:\\root\\old.txt"),
                to: PathBuf::from("C:\\root\\new.txt"),
            }]
        );
        assert_eq!(p.pending_cursor_name.as_deref(), Some("new.txt"));
        assert!(p.overlay.is_none());
        // 不正な名前 (区切り文字) は確定されずモーダルが残る
        update_pane(&mut p, PaneMsg::OpenNewFolder);
        update_pane(&mut p, PaneMsg::ModalInput("a\\b".into()));
        assert!(update_pane(&mut p, PaneMsg::ModalCommit).is_empty());
        assert!(p.overlay.is_some());
    }

    #[test]
    fn paste_without_conflict_spawns_job_directly() {
        use crate::transfer::TransferOp;
        let mut p = pane_with(&[("existing.txt", false)]);
        let fx = update_pane(
            &mut p,
            PaneMsg::PasteRead {
                paths: vec!["D:\\src\\new.txt".into()],
                op: "copy".into(),
            },
        );
        assert_eq!(
            fx,
            vec![Effect::SpawnJob {
                op: TransferOp::Copy,
                items: vec![(
                    PathBuf::from("D:\\src\\new.txt"),
                    PathBuf::from("C:\\root\\new.txt")
                )],
            }]
        );
        assert!(p.overlay.is_none());
    }

    #[test]
    fn paste_with_conflict_opens_dialog_then_rename_both() {
        use crate::transfer::{ConflictChoice, TransferOp};
        let mut p = pane_with(&[("a.txt", false)]);
        let fx = update_pane(
            &mut p,
            PaneMsg::PasteRead {
                paths: vec!["D:\\src\\a.txt".into()],
                op: "cut".into(),
            },
        );
        assert!(fx.is_empty());
        assert!(matches!(p.overlay, Some(Overlay::Conflict { .. })));
        let fx = update_pane(&mut p, PaneMsg::Conflict(ConflictChoice::RenameBoth));
        assert_eq!(
            fx,
            vec![Effect::SpawnJob {
                op: TransferOp::Move,
                items: vec![(
                    PathBuf::from("D:\\src\\a.txt"),
                    PathBuf::from("C:\\root\\a (2).txt")
                )],
            }]
        );
        assert!(p.overlay.is_none());
    }

    #[test]
    fn esc_cancels_running_job_before_clearing_selection() {
        use crate::domain_event::DomainEvent;
        let mut p = pane_with(&[("a.txt", false)]);
        p.click_row(0, false, false);
        update_pane(
            &mut p,
            PaneMsg::Domain(DomainEvent::JobProgress {
                job_id: 9,
                kind: "copy".into(),
                done_files: 1,
                total_files: 5,
                done_bytes: 0,
                total_bytes: 0,
                current: "x".into(),
            }),
        );
        assert!(p.job.is_some());
        // 1 回目の Esc = ジョブキャンセル (選択は残る)
        let fx = update_pane(&mut p, PaneMsg::ClearSelection);
        assert_eq!(fx, vec![Effect::CancelJob { id: 9 }]);
        assert!(!p.selected.is_empty());
        // JobDone (キャンセル) で job が消え、次の Esc は選択解除
        update_pane(
            &mut p,
            PaneMsg::Domain(DomainEvent::JobDone {
                job_id: 9,
                kind: "copy".into(),
                ok: false,
                canceled: true,
                error: None,
                done_files: 1,
                total_files: 5,
            }),
        );
        assert!(p.job.is_none());
        assert_eq!(p.status_msg.as_deref(), Some("キャンセルしました"));
        assert!(update_pane(&mut p, PaneMsg::ClearSelection).is_empty());
        assert!(p.selected.is_empty());
    }

    #[test]
    fn copy_cut_delete_use_selection() {
        let mut p = pane_with(&[("a.txt", false), ("b.txt", false)]);
        p.click_row(0, false, false);
        p.click_row(1, true, false);
        let fx = update_pane(&mut p, PaneMsg::RequestCopy);
        assert_eq!(
            fx,
            vec![Effect::ClipboardWrite {
                paths: vec![
                    PathBuf::from("C:\\root\\a.txt"),
                    PathBuf::from("C:\\root\\b.txt")
                ],
                op: "copy".into(),
            }]
        );
        let fx = update_pane(&mut p, PaneMsg::RequestDelete);
        assert_eq!(
            fx,
            vec![Effect::DeleteToTrash {
                paths: vec![
                    PathBuf::from("C:\\root\\a.txt"),
                    PathBuf::from("C:\\root\\b.txt")
                ],
            }]
        );
        // 選択なしなら何も出ない
        p.clear_selection();
        assert!(update_pane(&mut p, PaneMsg::RequestCut).is_empty());
        assert!(update_pane(&mut p, PaneMsg::RequestDelete).is_empty());
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
