//! 副作用の記述 (計画書 §5.3)。`update` は I/O をせず Effect を返し、
//! GUI 層 (fastfiler-iced/effects.rs) が実行する。

use std::path::PathBuf;

use crate::model::PaneId;

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// フォルダ一覧の読み込み (domain fs::list_dir + アイコン取得)。
    /// gen が古くなった結果は捨てる (連続ナビゲーションのキャンセル)。
    /// 実行側は watcher の監視先もこのパスへ付け替える。
    LoadDir {
        pane: PaneId,
        generation: u64,
        path: PathBuf,
    },
    /// ファイルを既定アプリで開く (domain shell)。
    OpenFile {
        path: PathBuf,
    },
    /// millis 後に該当ペインへ `PaneMsg::ReloadTick(seq)` を返す (150ms デバウンス)。
    Debounce {
        pane: PaneId,
        seq: u64,
        millis: u64,
    },
    // ---- ファイル操作 (Phase 2b) ----
    /// クリップボードへ書き込む (CF_HDROP + Preferred DropEffect)。op: "copy" | "cut"
    ClipboardWrite {
        paths: Vec<PathBuf>,
        op: String,
    },
    /// クリップボードを読む → 該当ペインへ `PaneMsg::PasteRead` が返る。
    ClipboardRead {
        pane: PaneId,
    },
    /// コピー/移動ジョブを起動する (進捗は DomainEvent 経由で pane へ戻る)。
    SpawnJob {
        pane: PaneId,
        op: crate::transfer::TransferOp,
        items: Vec<(PathBuf, PathBuf)>,
    },
    CancelJob {
        id: u64,
    },
    /// リネーム (上書きなし)。成功時は Undo 記録 (実行側)。
    Rename {
        from: PathBuf,
        to: PathBuf,
    },
    CreateDir {
        path: PathBuf,
    },
    /// 空ファイル作成 (同名があれば domain 側が連番を振る)。
    CreateFile {
        dir: PathBuf,
        name: String,
    },
    /// ごみ箱へ送る。成功時は Undo 記録 (実行側)。
    DeleteToTrash {
        paths: Vec<PathBuf>,
    },
    /// Undo スタックの先頭を取り消す (実行側が逆操作)。
    PerformUndo,
    // ---- タブ / ペイン (Phase 3) ----
    /// ロックタブの移動を新タブへ逃がす (F-104)。update_app が展開する。
    OpenTabFor {
        path: PathBuf,
    },
    /// ペインが閉じた (GUI 層は watcher 等の付随リソースを解放する)。
    PaneClosed(PaneId),
    /// セッション保存の予約 (800ms デバウンス — 実行側でタイマー管理)。
    ScheduleSessionSave,
}
