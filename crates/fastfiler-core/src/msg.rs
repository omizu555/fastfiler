//! メッセージ定義 (計画書 §5.3)。Phase 1 は単一ペイン分のみ。
//! Phase 3 で `Msg::Pane(PaneId, PaneMsg)` の階層に載る。

use crate::model::{Column, Entry};
use crate::selection::NavKey;

/// ペインへの入力。GUI 層 (FileList / キーボード購読) が発行し、
/// `update::update_pane` が解釈する。
#[derive(Debug, Clone)]
pub enum PaneMsg {
    /// 読み込み完了 (世代が現在と一致するときだけ反映)。
    Loaded {
        generation: u64,
        entries: Vec<Entry>,
    },
    LoadFailed {
        generation: u64,
        error: String,
    },
    RowPressed {
        ix: usize,
        ctrl: bool,
        shift: bool,
    },
    RowDoubleClicked {
        ix: usize,
    },
    /// Enter (カーソル行を開く)。
    ActivateCursor,
    /// Backspace / ↑ ボタン (親フォルダへ)。
    GoParent,
    HeaderClicked(Column),
    /// 列幅ドラッグ (Modified/Size/Type のみ)。値は clamp 済み前提としない。
    ColResized {
        col: Column,
        width: f32,
    },
    Scrolled(f32),
    ViewportChanged {
        height: f32,
    },
    Nav(NavKey, bool),
    SelectAll,
    ClearSelection,
    /// 一覧の空白部分をクリック (選択解除)。
    BlankPressed,
}
