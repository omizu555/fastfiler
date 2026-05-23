// drop_exec.rs — D&D ドロップ実行の共通ヘルパ。
//
// 内部 D&D (左/右ボタン) と外部受信 D&D (右ボタン経由のメニュー選択時) で
// 同じ「items 組立 → 閾値判定 → JobsState or 同期 + Undo」フローを共有する。
//
// 設計:
// - `execute_drop`: 実体。is_move フラグで Move/Copy を切替、source_pane_id が
//   Some なら同一フォルダ判定で skip + 終了時に元ペイン reload も行う。
// - `show_right_drop_menu`: 右ボタンドラッグのドロップ時に context menu を出して、
//   選択結果に応じて `execute_drop` を呼ぶ。
//
// pane.rs 内の左ボタン drop ロジック (元 L1380-1497) もこのヘルパに集約済。

use std::path::PathBuf;

use floem::action::show_context_menu;
use floem::kurbo::Point;
use floem::menu::{Menu, MenuItem};
use floem::reactive::{SignalGet, SignalUpdate};

use fastfiler_domain::file_ops as fops;
use fastfiler_domain::undo::{MoveItem, UndoOp};

use crate::fs_model::unique_dest;
use crate::state::{AppState, PaneState};

/// D&D ドロップを実行する。`source_pane_id` は内部 D&D 元ペインの ID
/// (外部受信時は None)。`reason` はログ用。
pub fn execute_drop(
    app: &AppState,
    target_pane: &PaneState,
    source_pane_id: Option<u64>,
    src_paths: &[PathBuf],
    is_move: bool,
    reason: &str,
) {
    let dest_dir = target_pane.cur_path.get_untracked();
    let op_label = if is_move { "移動" } else { "コピー" };

    // 衝突回避済みの (src, dst) を組み立てる。Move 時は同一ディレクトリは skip。
    let mut items: Vec<(PathBuf, PathBuf)> = Vec::with_capacity(src_paths.len());
    let mut skipped_same_dir = 0u32;
    for src in src_paths {
        if is_move {
            if let Some(parent) = src.parent() {
                if parent == dest_dir.as_path() {
                    skipped_same_dir += 1;
                    crate::flog!("[drop] skip move src={} (same dir as dest)", src.display());
                    continue;
                }
            }
        }
        let Some(name) = src.file_name().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        let dst = unique_dest(&dest_dir, &name);
        items.push((src.clone(), dst));
    }
    if items.is_empty() {
        if skipped_same_dir > 0 {
            target_pane.status_msg.set(format!(
                "D&D {} スキップ ({} 件は同一フォルダ)",
                op_label, skipped_same_dir
            ));
        }
        return;
    }

    // 大量時 (件数 ≥ 100 or 合計 ≥ 50MB) は JobsState 経由 (Undo 不可)
    let (total_files, total_bytes) =
        crate::core::jobs::scan_total_for_threshold(items.iter().map(|(f, _)| f.as_path()));
    let big = total_files >= crate::core::jobs::THRESHOLD_FILES
        || total_bytes >= crate::core::jobs::THRESHOLD_BYTES;
    crate::flog!(
        "[drop] dest_dir={} op={} reason={} files={} big={}",
        dest_dir.display(),
        if is_move { "move" } else { "copy" },
        reason,
        items.len(),
        big,
    );
    if big {
        target_pane.status_msg.set(format!(
            "D&D {} 開始 ({} 件、進捗表示中 / Undo 不可)",
            op_label,
            items.len()
        ));
        if is_move {
            app.jobs.spawn_move(items, |_ok| {});
        } else {
            app.jobs.spawn_copy(items, |_ok| {});
        }
        return;
    }

    // 閾値未満は同期パス (Move は UndoOp::Move へ push)
    let _g_drop = crate::core::perf::scope(
        if is_move {
            crate::core::perf::MetricKind::Move
        } else {
            crate::core::perf::MetricKind::Copy
        },
        format!("drop sync files={} reason={}", items.len(), reason),
    );
    let mut ok = 0u32;
    let mut err = 0u32;
    let mut moved: Vec<MoveItem> = Vec::new();
    for (from, dst) in &items {
        crate::flog!(
            "[drop] {} src={} dst={}",
            if is_move { "move" } else { "copy" },
            from.display(),
            dst.display()
        );
        let res = if is_move {
            fops::move_path(
                from.to_string_lossy().into_owned(),
                dst.to_string_lossy().into_owned(),
            )
        } else {
            fops::copy_path(
                from.to_string_lossy().into_owned(),
                dst.to_string_lossy().into_owned(),
            )
        };
        match res {
            Ok(()) => {
                ok += 1;
                if is_move {
                    moved.push(MoveItem {
                        from: from.clone(),
                        to: dst.clone(),
                    });
                }
            }
            Err(e) => {
                crate::flog!("[drop] op error: {}", e);
                err += 1;
            }
        }
    }
    if !moved.is_empty() {
        app.undo_manager.lock().push(UndoOp::Move { items: moved });
    }
    let suffix = if skipped_same_dir > 0 {
        format!(" / スキップ={}", skipped_same_dir)
    } else {
        String::new()
    };
    target_pane
        .status_msg
        .set(format!("D&D {} OK={} / NG={}{}", op_label, ok, err, suffix));
    target_pane.reload();
    if let Some(spid) = source_pane_id {
        if let Some(sp) = app.find_pane(spid) {
            sp.reload();
        }
    }
}

/// 右ボタン D&D のドロップ時に「ここにコピー / ここに移動 / キャンセル」
/// メニューを出す。同一フォルダのみの drop はメニューを出さず無視する。
pub fn show_right_drop_menu(
    app: AppState,
    target_pane: PaneState,
    source_pane_id: Option<u64>,
    src_paths: Vec<PathBuf>,
    viewport_pos: Point,
) {
    if src_paths.is_empty() {
        return;
    }
    // 同一フォルダのみが対象なら何もしない (Move/Copy いずれも無意味)。
    let dest_dir = target_pane.cur_path.get_untracked();
    let all_same_dir = src_paths
        .iter()
        .all(|p| p.parent() == Some(dest_dir.as_path()));
    if all_same_dir {
        crate::flog!(
            "[drop:right] skip menu (all src in dest_dir={})",
            dest_dir.display()
        );
        return;
    }

    let app_for_copy = app.clone();
    let pane_for_copy = target_pane.clone();
    let paths_for_copy = src_paths.clone();
    let app_for_move = app.clone();
    let pane_for_move = target_pane.clone();
    let paths_for_move = src_paths.clone();

    let menu = Menu::new("")
        .entry(MenuItem::new("ここにコピー(&C)").action(move || {
            crate::flog!("[drop:right] menu -> copy");
            execute_drop(
                &app_for_copy,
                &pane_for_copy,
                source_pane_id,
                &paths_for_copy,
                false,
                "right-menu",
            );
        }))
        .entry(MenuItem::new("ここに移動(&M)").action(move || {
            crate::flog!("[drop:right] menu -> move");
            execute_drop(
                &app_for_move,
                &pane_for_move,
                source_pane_id,
                &paths_for_move,
                true,
                "right-menu",
            );
        }))
        .separator()
        .entry(MenuItem::new("キャンセル").action(|| {
            crate::flog!("[drop:right] menu -> cancel");
        }));

    show_context_menu(menu, Some(viewport_pos));
}
