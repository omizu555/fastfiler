//! Undo マネージャ。
//!
//! ADR 0006: 対象は **リネーム / 移動 / ゴミ箱送り** の 3 種、in-memory N=20、起動間で保持しない。
//! ADR 0008:
//!  - 履歴はアプリ全体で 1 本 (グローバル)
//!  - バルクは 1 ユーザーアクション = 1 アンドゥ枠
//!  - 失敗分は新しい UndoOp として stack に push し直す
//!  - 上書きは絶対にしない (実行側で no-overwrite な逆操作 API を使う)
//!  - ゴミ箱復元は (元パス + ファイル名 + size + modified + is_dir + deleted_at) で識別
//!
//! このモジュールは **データ構造と stack 操作のみ** を提供する。
//! 逆操作の実行 (rename/move/restore_from_trash) は呼び出し側 (native crate) が
//! 取り出した `UndoOp` を解釈して行う。

use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::SystemTime;

/// Undo スタックの最大長 (ADR 0006)。
pub const MAX_HISTORY: usize = 20;

/// ゴミ箱に送られた 1 項目の情報。
/// 復元時の識別キーとして使用する (ADR 0008 S3)。
#[derive(Clone, Debug)]
pub struct TrashedItem {
    pub original_path: PathBuf,
    pub file_name: OsString,
    pub size: u64,
    pub modified: SystemTime,
    pub is_dir: bool,
    pub deleted_at: SystemTime,
}

/// 1 ユーザーアクション分の Undo 情報 (バルク含む)。
#[derive(Clone, Debug)]
pub enum UndoOp {
    /// `from` (元の名前) ←→ `to` (変更後)。Undo は `to` → `from` への rename。
    /// 親フォルダは同じ前提。
    Rename { from: PathBuf, to: PathBuf },
    /// 1 ペースト/1 D&D 等の 1 ユーザーアクションで移動した複数項目。
    /// Undo は各要素を `to` → `from` へ move back する。
    /// 同名衝突回避でリネームされた場合も `to` は実際の配置先。
    Move { items: Vec<MoveItem> },
    /// ゴミ箱へ送った複数項目。Undo は各要素を `IFileOperation::MoveItems` で元の場所へ戻す。
    Trash { items: Vec<TrashedItem> },
}

#[derive(Clone, Debug)]
pub struct MoveItem {
    pub from: PathBuf,
    pub to: PathBuf,
}

impl UndoOp {
    /// 人間に表示する種別ラベル。
    pub fn label(&self) -> &'static str {
        match self {
            UndoOp::Rename { .. } => "リネーム",
            UndoOp::Move { .. } => "移動",
            UndoOp::Trash { .. } => "ゴミ箱送り",
        }
    }

    /// 影響件数 (1 アクションあたり)。
    pub fn count(&self) -> usize {
        match self {
            UndoOp::Rename { .. } => 1,
            UndoOp::Move { items } => items.len(),
            UndoOp::Trash { items } => items.len(),
        }
    }

    /// 代表パス (ステータスバー表示用)。Move/Trash の場合は先頭要素の戻し先。
    pub fn representative_path(&self) -> Option<&std::path::Path> {
        match self {
            UndoOp::Rename { from, .. } => Some(from.as_path()),
            UndoOp::Move { items } => items.first().map(|i| i.from.as_path()),
            UndoOp::Trash { items } => items.first().map(|i| i.original_path.as_path()),
        }
    }
}

/// in-memory Undo スタック (容量 [`MAX_HISTORY`])。
#[derive(Default, Debug)]
pub struct UndoManager {
    stack: VecDeque<UndoOp>,
}

impl UndoManager {
    pub fn new() -> Self {
        Self {
            stack: VecDeque::with_capacity(MAX_HISTORY),
        }
    }

    /// 履歴を末尾に積む。容量を超えたら最古を捨てる。
    pub fn push(&mut self, op: UndoOp) {
        // 空 items は積まない (push しても Undo 時にすることが無い)
        match &op {
            UndoOp::Move { items } if items.is_empty() => return,
            UndoOp::Trash { items } if items.is_empty() => return,
            _ => {}
        }
        if self.stack.len() == MAX_HISTORY {
            self.stack.pop_front();
        }
        self.stack.push_back(op);
    }

    /// 直近の履歴を取り出す (Undo 実行用)。
    pub fn pop(&mut self) -> Option<UndoOp> {
        self.stack.pop_back()
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_rename(n: u32) -> UndoOp {
        UndoOp::Rename {
            from: PathBuf::from(format!("C:/a{n}.txt")),
            to: PathBuf::from(format!("C:/b{n}.txt")),
        }
    }

    #[test]
    fn push_and_pop_lifo() {
        let mut m = UndoManager::new();
        m.push(mk_rename(1));
        m.push(mk_rename(2));
        m.push(mk_rename(3));
        let op = m.pop().unwrap();
        match op {
            UndoOp::Rename { from, .. } => assert_eq!(from, PathBuf::from("C:/a3.txt")),
            _ => panic!("expected rename"),
        }
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn capacity_capped_at_max_history() {
        let mut m = UndoManager::new();
        for i in 0..(MAX_HISTORY as u32 + 5) {
            m.push(mk_rename(i));
        }
        assert_eq!(m.len(), MAX_HISTORY);
        // 最古 5 件 (0..5) は押し出され、末尾は最新
        let op = m.pop().unwrap();
        match op {
            UndoOp::Rename { from, .. } => {
                assert_eq!(from, PathBuf::from(format!("C:/a{}.txt", MAX_HISTORY + 4)));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn empty_bulk_is_not_pushed() {
        let mut m = UndoManager::new();
        m.push(UndoOp::Move { items: vec![] });
        m.push(UndoOp::Trash { items: vec![] });
        assert!(m.is_empty());
    }

    #[test]
    fn pop_on_empty_returns_none() {
        let mut m = UndoManager::new();
        assert!(m.pop().is_none());
    }

    #[test]
    fn label_and_count() {
        let r = mk_rename(1);
        assert_eq!(r.label(), "リネーム");
        assert_eq!(r.count(), 1);
        let m = UndoOp::Move {
            items: vec![
                MoveItem {
                    from: "a".into(),
                    to: "b".into(),
                },
                MoveItem {
                    from: "c".into(),
                    to: "d".into(),
                },
            ],
        };
        assert_eq!(m.label(), "移動");
        assert_eq!(m.count(), 2);
    }
}
