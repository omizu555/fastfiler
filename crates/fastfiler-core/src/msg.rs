//! メッセージ定義 (計画書 §5.3)。Phase 1 は単一ペイン分のみ。
//! Phase 3 で `Msg::Pane(PaneId, PaneMsg)` の階層に載る。

use crate::domain_event::DomainEvent;
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
    // ---- ナビゲーション (Phase 2a) ----
    /// 履歴を戻る (Alt+← / マウス第4ボタン)。
    GoBack,
    /// 履歴を進む (Alt+→ / マウス第5ボタン)。
    GoForward,
    /// F5 再読み込み (選択・スクロールは維持)。
    Reload,
    /// パスバークリック → 直接入力を開く。
    OpenPathEdit,
    PathEditInput(String),
    PathEditCommit,
    PathEditCancel,
    // ---- domain イベント (Phase 2a) ----
    Domain(DomainEvent),
    /// デバウンス満了 (Effect::Debounce の返し)。seq が現在値なら reload する。
    ReloadTick(u64),
    // ---- ファイル操作 (Phase 2b) ----
    /// F2 (カーソル行のリネーム)。
    OpenRename,
    /// F7 / F8。
    OpenNewFolder,
    OpenNewFile,
    ModalInput(String),
    ModalCommit,
    ModalCancel,
    /// Ctrl+C / Ctrl+X / Ctrl+V。
    RequestCopy,
    RequestCut,
    RequestPaste,
    /// クリップボード読み取り結果 (Effect::ClipboardRead の返し)。op: "copy"|"cut"。
    PasteRead {
        paths: Vec<String>,
        op: String,
    },
    /// 衝突ダイアログの選択。
    Conflict(crate::transfer::ConflictChoice),
    /// Delete (選択をごみ箱へ)。
    RequestDelete,
    /// Ctrl+Z。
    Undo,
    /// フッタへの一時メッセージ (操作結果の通知)。
    StatusMsg(String),
    /// フッタのキャンセルボタン (モーダル表示中でも実行中ジョブを止められる専用経路。
    /// Esc は「モーダルを閉じる > ジョブキャンセル」の階層だが、こちらは直接止める)。
    CancelJobRequest,
}
