//! `PaneState` のユーザーアクション群。
//!
//! state.rs にはストア定義 + コア (new / navigate / refresh_rows_only) のみ残し、
//! 履歴ナビゲーション・選択・モーダル・ファイル操作・ソート・クリップボードを
//! こちらに分離する。`impl PaneState` の追加 impl ブロックとして書ける。

use std::path::PathBuf;

use fastfiler_domain::file_ops as fops;
use fastfiler_domain::win_clipboard as wcb;
use floem::reactive::{SignalGet, SignalUpdate, SignalWith};

use crate::fs_model::{sort_rows, unique_dest, SortKey};
use crate::state::{ModalKind, PaneState};

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
    pub fn delete_selected(&self) {
        let paths: Vec<String> = self
            .selected_paths()
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        if paths.is_empty() {
            crate::flog!("[delete] no selection, skip");
            return;
        }
        let n = paths.len();
        crate::flog!("[delete] -> trash: {} files: {:?}", n, paths);
        // SHFileOperationW は通常 panic しないが、念のため catch_unwind で保護
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            fops::delete_to_trash(paths)
        }));
        // 削除後はインデックスがズレるので必ず選択をクリア
        self.selected.set(im::OrdSet::new());
        self.anchor.set(None);
        match result {
            Ok(Ok(())) => {
                self.status_msg
                    .set(format!("ごみ箱へ送りました ({} 件)", n));
                self.reload();
            }
            Ok(Err(e)) => {
                self.status_msg.set(format!("削除失敗: {}", e));
                self.reload();
            }
            Err(_) => {
                self.status_msg
                    .set(String::from("削除失敗: 内部例外 (詳細はログ参照)"));
                self.reload();
            }
        }
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

    pub fn confirm_modal(&self) {
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
    pub fn clipboard_paste(&self) {
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
                Ok(()) => ok += 1,
                Err(e) => {
                    crate::flog!("[paste] op error: {}", e);
                    err += 1;
                }
            }
        }
        let label = if is_move { "移動" } else { "コピー" };
        self.status_msg
            .set(format!("{} 完了 OK={} / NG={}", label, ok, err));
        self.reload();
    }
}
