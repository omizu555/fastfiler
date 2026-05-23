//! `PaneState` のユーザーアクション群。
//!
//! state.rs にはストア定義 + コア (new / navigate / refresh_rows_only) のみ残し、
//! 履歴ナビゲーション・選択・モーダル・ファイル操作・ソート・クリップボードを
//! こちらに分離する。`impl PaneState` の追加 impl ブロックとして書ける。

use std::path::PathBuf;
use std::sync::Arc;

use fastfiler_domain::file_ops as fops;
use fastfiler_domain::undo::{MoveItem, TrashedItem, UndoManager, UndoOp};
use fastfiler_domain::win_clipboard as wcb;
use floem::reactive::{SignalGet, SignalUpdate, SignalWith};
use parking_lot::Mutex;

use crate::fs_model::{sort_rows, unique_dest, SortKey};
use crate::state::{AppState, ModalKind, PaneState};

impl PaneState {
    // ───── 履歴ナビゲーション ─────

    pub fn back(&self) {
        let mut h = self.history.get();
        if let Some(prev) = h.back.pop_back() {
            let cur = self.cur_path.get_untracked();
            h.forward.push_back(cur);
            self.history.set(h);
            self.navigate(prev, false);
        }
    }

    pub fn forward(&self) {
        let mut h = self.history.get();
        if let Some(next) = h.forward.pop_back() {
            let cur = self.cur_path.get_untracked();
            h.back.push_back(cur);
            self.history.set(h);
            self.navigate(next, false);
        }
    }

    pub fn up(&self) {
        let cur = self.cur_path.get_untracked();
        if let Some(parent) = cur.parent() {
            self.navigate(parent.to_path_buf(), true);
        }
    }

    pub fn reload(&self) {
        let cur = self.cur_path.get_untracked();
        self.navigate(cur, false);
    }

    // ───── 選択 ─────

    /// 選択行のフルパスを返す
    pub fn selected_paths(&self) -> Vec<PathBuf> {
        let rows = self.rows.get_untracked();
        let cur = self.cur_path.get_untracked();
        self.selected
            .get_untracked()
            .iter()
            .filter_map(|i| rows.get(*i).map(|r| cur.join(&r.name)))
            .collect()
    }

    /// 選択行が 1 件のときのみインデックスを返す
    pub fn single_selected(&self) -> Option<usize> {
        let s = self.selected.get_untracked();
        if s.len() == 1 {
            s.iter().next().copied()
        } else {
            None
        }
    }

    /// 行 idx をクリック (修飾キー対応)
    pub fn click_row(&self, idx: usize, ctrl: bool, shift: bool) {
        // 範囲外を即弾く (sort/reload 直後の古いインデックスでクラッシュさせない)
        let len = self.rows.with_untracked(|v| v.len());
        if idx >= len {
            return;
        }
        if shift {
            let anchor = self.anchor.get_untracked().unwrap_or(idx);
            let (lo, hi) = if anchor <= idx {
                (anchor, idx)
            } else {
                (idx, anchor)
            };
            let mut set = if ctrl {
                self.selected.get_untracked()
            } else {
                im::OrdSet::new()
            };
            for i in lo..=hi.min(len.saturating_sub(1)) {
                set.insert(i);
            }
            self.selected.set(set);
        } else if ctrl {
            self.selected.update(|s| {
                if s.contains(&idx) {
                    s.remove(&idx);
                } else {
                    s.insert(idx);
                }
            });
            self.anchor.set(Some(idx));
        } else {
            let mut set = im::OrdSet::new();
            set.insert(idx);
            self.selected.set(set);
            self.anchor.set(Some(idx));
        }
    }

    pub fn select_all(&self) {
        let len = self.rows.with_untracked(|v| v.len());
        let mut set = im::OrdSet::new();
        for i in 0..len {
            set.insert(i);
        }
        self.selected.set(set);
    }

    // ───── ファイル操作 (削除 / モーダル経由の作成・リネーム) ─────

    /// 選択をゴミ箱へ送る
    ///
    /// ADR 0008 S4: Undo 対応のため 1 件ずつ trash 処理して成否を取得し、
    /// 成功分だけを `TrashedItem` 化して `undo_manager` に push する。
    pub fn delete_selected(&self, undo_manager: &Arc<Mutex<UndoManager>>) {
        let paths: Vec<PathBuf> = self.selected_paths();
        if paths.is_empty() {
            crate::flog!("[delete] no selection, skip");
            return;
        }
        let n = paths.len();
        crate::flog!("[delete] -> trash: {} files", n);

        let mut ok_items: Vec<TrashedItem> = Vec::with_capacity(n);
        let mut ng = 0usize;
        for p in &paths {
            // 削除前にメタ情報を採取 (Undo 復元時の識別キーに使う)
            let meta = std::fs::metadata(p).ok();
            let snapshot = meta.map(|m| TrashedItem {
                original_path: p.clone(),
                file_name: p.file_name().map(|s| s.to_os_string()).unwrap_or_default(),
                size: if m.is_dir() { 0 } else { m.len() },
                modified: m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                is_dir: m.is_dir(),
                deleted_at: std::time::SystemTime::now(),
            });
            let path_str = p.to_string_lossy().into_owned();
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                fops::delete_to_trash(vec![path_str])
            }));
            match (res, snapshot) {
                (Ok(Ok(())), Some(item)) => ok_items.push(item),
                (Ok(Ok(())), None) => {} // メタが取れず Undo 不可だが削除自体は成功
                (Ok(Err(e)), _) => {
                    crate::flog!("[delete] op error on {}: {}", p.display(), e);
                    ng += 1;
                }
                (Err(_), _) => {
                    crate::flog!("[delete] panic on {}", p.display());
                    ng += 1;
                }
            }
        }
        let ok_n = ok_items.len();
        if !ok_items.is_empty() {
            undo_manager.lock().push(UndoOp::Trash { items: ok_items });
        }
        self.selected.set(im::OrdSet::new());
        self.anchor.set(None);
        if ng == 0 {
            self.status_msg
                .set(format!("ごみ箱へ送りました ({} 件)", ok_n));
        } else {
            self.status_msg.set(format!("削除 OK={} / NG={}", ok_n, ng));
        }
        self.reload();
    }

    pub fn open_new_folder_modal(&self) {
        self.modal_input.set(String::from("New Folder"));
        self.modal_kind.set(ModalKind::NewFolder);
    }

    #[allow(dead_code)]
    pub fn open_new_file_modal(&self) {
        self.modal_input.set(String::from("new.txt"));
        self.modal_kind.set(ModalKind::NewFile);
    }

    pub fn open_rename_modal(&self) {
        let Some(idx) = self.single_selected() else {
            self.status_msg
                .set(String::from("リネームは 1 件のみ選択時"));
            return;
        };
        let name = self
            .rows
            .with_untracked(|v| v.get(idx).map(|r| r.name.clone()));
        if let Some(name) = name {
            self.modal_input.set(name.clone());
            self.modal_kind.set(ModalKind::Rename(name));
        }
    }

    pub fn close_modal(&self) {
        self.modal_kind.set(ModalKind::None);
        self.modal_input.set(String::new());
    }

    pub fn confirm_modal(&self, undo_manager: &Arc<Mutex<UndoManager>>) {
        let kind = self.modal_kind.get_untracked();
        let input = self.modal_input.get_untracked().trim().to_string();
        if input.is_empty() {
            self.close_modal();
            return;
        }
        let cur = self.cur_path.get_untracked();
        match kind {
            ModalKind::None => {}
            ModalKind::NewFolder => {
                let target = cur.join(&input);
                match fops::create_dir(target.to_string_lossy().into_owned()) {
                    Ok(()) => {
                        self.status_msg.set(format!("作成: {}", input));
                        self.close_modal();
                        self.reload();
                    }
                    Err(e) => self.status_msg.set(format!("作成失敗: {}", e)),
                }
            }
            ModalKind::NewFile => {
                match fastfiler_domain::templates::create_empty_file(
                    cur.to_string_lossy().into_owned(),
                    input.clone(),
                    None,
                ) {
                    Ok(p) => {
                        self.status_msg.set(format!("作成: {}", p));
                        self.close_modal();
                        self.reload();
                    }
                    Err(e) => self.status_msg.set(format!("作成失敗: {}", e)),
                }
            }
            ModalKind::Rename(orig) => {
                if input == orig {
                    self.close_modal();
                    return;
                }
                let from = cur.join(&orig);
                let to = cur.join(&input);
                match fops::rename_path(
                    from.to_string_lossy().into_owned(),
                    to.to_string_lossy().into_owned(),
                ) {
                    Ok(()) => {
                        undo_manager.lock().push(UndoOp::Rename {
                            from: from.clone(),
                            to: to.clone(),
                        });
                        self.status_msg
                            .set(format!("リネーム: {} → {}", orig, input));
                        self.close_modal();
                        self.reload();
                    }
                    Err(e) => self.status_msg.set(format!("リネーム失敗: {}", e)),
                }
            }
        }
    }

    // ───── ソート ─────

    /// ソート列をクリック (同じ列なら方向トグル / 別列なら昇順)
    pub fn click_sort(&self, key: SortKey) {
        if self.sort_key.get_untracked() == key {
            self.sort_desc.update(|d| *d = !*d);
        } else {
            self.sort_key.set(key);
            self.sort_desc.set(false);
        }
        let k = self.sort_key.get_untracked();
        let d = self.sort_desc.get_untracked();
        self.rows.update(|v| sort_rows(v, k, d));
        self.selected.set(im::OrdSet::new());
        self.anchor.set(None);
    }

    // ───── クリップボード ─────

    /// 選択行をクリップボードへ書き込み (op = "copy" or "move")
    pub fn clipboard_write(&self, op: &str) {
        let paths: Vec<String> = self
            .selected_paths()
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        if paths.is_empty() {
            self.status_msg.set(String::from("選択がありません"));
            return;
        }
        let n = paths.len();
        match wcb::clipboard_write_paths(paths, op.to_string()) {
            Ok(()) => self
                .status_msg
                .set(format!("{} ({} 件) をクリップボードへ", op, n)),
            Err(e) => self.status_msg.set(format!("クリップボード失敗: {}", e)),
        }
    }

    /// クリップボードから貼り付け (Copy / Move を自動判別)
    ///
    /// move 経由で成功した項目は 1 ユーザーアクション = 1 UndoOp::Move として push する (ADR 0008 D3)。
    pub fn clipboard_paste(&self, undo_manager: &Arc<Mutex<UndoManager>>) {
        let cb = match wcb::clipboard_read_paths() {
            Ok(Some(c)) => c,
            Ok(None) => {
                self.status_msg
                    .set(String::from("クリップボードに項目がありません"));
                return;
            }
            Err(e) => {
                self.status_msg
                    .set(format!("クリップボード読込失敗: {}", e));
                return;
            }
        };
        let dst_dir = self.cur_path.get_untracked();
        let is_move = cb.op.eq_ignore_ascii_case("move");
        crate::flog!(
            "[paste] dst={} is_move={} count={}",
            dst_dir.display(),
            is_move,
            cb.paths.len()
        );
        let mut ok = 0usize;
        let mut err = 0usize;
        let mut moved: Vec<MoveItem> = Vec::new();
        for src in &cb.paths {
            let from = PathBuf::from(src);
            let name = from.file_name().map(|s| s.to_string_lossy().into_owned());
            let Some(name) = name else {
                err += 1;
                continue;
            };
            let dst = unique_dest(&dst_dir, &name);
            crate::flog!(
                "[paste] {} src={} dst={}",
                if is_move { "move" } else { "copy" },
                src,
                dst.display()
            );
            let res = if is_move {
                fops::move_path(src.clone(), dst.to_string_lossy().into_owned())
            } else {
                fops::copy_path(src.clone(), dst.to_string_lossy().into_owned())
            };
            match res {
                Ok(()) => {
                    ok += 1;
                    if is_move {
                        // ADR 0008 D4: 同名衝突回避でリネームされる可能性があるため
                        // Undo は「実際の配置先」を記録する。
                        moved.push(MoveItem {
                            from: from.clone(),
                            to: dst.clone(),
                        });
                    }
                }
                Err(e) => {
                    crate::flog!("[paste] op error: {}", e);
                    err += 1;
                }
            }
        }
        if !moved.is_empty() {
            undo_manager.lock().push(UndoOp::Move { items: moved });
        }
        let label = if is_move { "移動" } else { "コピー" };
        self.status_msg
            .set(format!("{} 完了 OK={} / NG={}", label, ok, err));
        self.reload();
    }
}

// ─────────────────────────────────────────────────────────────
// AppState::undo (ADR 0006/0008)
// ─────────────────────────────────────────────────────────────

impl AppState {
    /// 直近の Undo 操作を実行する。
    ///
    /// ADR 0008 S5: ロックは pop のみで取り、I/O はロック外で実行する。
    /// 完全失敗時は元の op を stack 末尾へ戻し、部分失敗時は失敗分のみを
    /// 新しい UndoOp として末尾へ push する (Ctrl+Z 連打で再試行可能)。
    pub fn undo(&self) {
        let op = { self.undo_manager.lock().pop() };
        let Some(op) = op else {
            if let Some(p) = self.active_pane() {
                p.status_msg.set(String::from("Undo 履歴なし"));
            }
            return;
        };
        crate::flog!("[undo] start: {} ({} 件)", op.label(), op.count());

        let (msg, remainder) = match op {
            UndoOp::Rename { from, to } => {
                match fops::rename_path_no_overwrite(
                    to.to_string_lossy().into_owned(),
                    from.to_string_lossy().into_owned(),
                ) {
                    Ok(()) => (format!("Undo: リネーム取消 ({})", from.display()), None),
                    Err(e) => {
                        crate::flog!("[undo] rename failed: {}", e);
                        (
                            format!("Undo 失敗 (リネーム): {}", e),
                            Some(UndoOp::Rename { from, to }),
                        )
                    }
                }
            }
            UndoOp::Move { items } => {
                let mut failed: Vec<MoveItem> = Vec::new();
                let mut ok = 0usize;
                for it in &items {
                    match fops::move_path_no_overwrite(
                        it.to.to_string_lossy().into_owned(),
                        it.from.to_string_lossy().into_owned(),
                    ) {
                        Ok(()) => ok += 1,
                        Err(e) => {
                            crate::flog!(
                                "[undo] move-back failed {} -> {}: {}",
                                it.to.display(),
                                it.from.display(),
                                e
                            );
                            failed.push(it.clone());
                        }
                    }
                }
                let total = items.len();
                let remainder = if failed.is_empty() {
                    None
                } else if failed.len() == total {
                    Some(UndoOp::Move { items: failed })
                } else {
                    Some(UndoOp::Move { items: failed })
                };
                (
                    format!("Undo: 移動取消 OK={} / NG={}", ok, total - ok),
                    remainder,
                )
            }
            UndoOp::Trash { items } => {
                let mut failed: Vec<TrashedItem> = Vec::new();
                let mut ok = 0usize;
                for it in &items {
                    match fops::restore_from_trash(it) {
                        Ok(()) => ok += 1,
                        Err(e) => {
                            crate::flog!(
                                "[undo] restore failed {}: {}",
                                it.original_path.display(),
                                e
                            );
                            failed.push(it.clone());
                        }
                    }
                }
                let total = items.len();
                let extra_hint = if !failed.is_empty() {
                    " (ゴミ箱から自動復元できなかった項目があります)"
                } else {
                    ""
                };
                let remainder = if failed.is_empty() {
                    None
                } else {
                    Some(UndoOp::Trash { items: failed })
                };
                (
                    format!(
                        "Undo: ゴミ箱送り取消 OK={} / NG={}{}",
                        ok,
                        total - ok,
                        extra_hint
                    ),
                    remainder,
                )
            }
        };

        if let Some(rem) = remainder {
            self.undo_manager.lock().push(rem);
        }

        if let Some(p) = self.active_pane() {
            p.status_msg.set(msg);
            p.reload();
        }
        // FS 監視通知 (ツリー等の更新)
        self.tree_tick.update(|t| *t = t.wrapping_add(1));
    }
}
