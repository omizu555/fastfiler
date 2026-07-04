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
    // ---- ツリー / 検索 (Phase 4) ----
    /// ドライブ一覧の読み込み (起動時)。
    LoadDrives,
    /// ツリーの子フォルダ読み込み (遅延展開)。
    LoadTreeChildren {
        path: PathBuf,
    },
    /// 検索開始 (Everything → 内蔵フォールバックは domain 側)。
    StartSearch {
        pane: PaneId,
        root: PathBuf,
        query: String,
    },
    // ---- メニュー / シェル統合 (Phase 5a) ----
    /// テンプレートから新規ファイル (同名は domain が連番)。
    CreateFromTemplate {
        dir: PathBuf,
        template: String,
    },
    /// ユーザーコマンド実行 (commands.json — F-904)。
    RunUserCommand {
        id: String,
        paths: Vec<PathBuf>,
        cwd: PathBuf,
    },
    /// Windows シェルコンテキストメニュー (F-905。UI スレッドで同期実行)。
    ShowShellMenu {
        paths: Vec<PathBuf>,
    },
    /// 外部 (OLE) からの移動: Copy ジョブ + 完了後にソースをゴミ箱 (実行側が分解)。
    /// 受信側の rename はドラッグ直後のソースロックと衝突するため使わない。
    SpawnExternalMove {
        pane: PaneId,
        items: Vec<(PathBuf, PathBuf)>,
    },
    /// ドロップメニュー確定後の転送 (update_app が衝突検出込みで展開する)。
    DropTransfer {
        pane: PaneId,
        op: crate::transfer::TransferOp,
        paths: Vec<PathBuf>,
        dest: PathBuf,
    },
}
