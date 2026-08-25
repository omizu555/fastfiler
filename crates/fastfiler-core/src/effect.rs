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
    /// 仮想ファイル貼り付けジョブ (RDP/Outlook — クリップボードの FILECONTENTS
    /// を dest へ抽出する。常にコピー動作。進捗は SpawnJob と同じ経路)。
    PasteVirtual {
        pane: PaneId,
        dest: PathBuf,
        entries: Vec<crate::transfer::VirtualEntry>,
    },
    CancelJob {
        id: u64,
    },
    /// リネーム (上書きなし)。成功時は Undo 記録 (実行側)。
    Rename {
        from: PathBuf,
        to: PathBuf,
    },
    /// フォルダの一括作成 (F7 複数行 = 1 行 1 フォルダ、`aaa\iii\uuu` の階層可)。
    /// 1 Effect にまとめて完了通知 → 明示 reload を 1 回に抑える (N 行で N 回
    /// reload しない)。names は dir 相対 — 失敗通知に入力行の表記のまま出す
    /// (絶対パスに畳むと階層行の失敗が葉の名前しか出せない)。
    CreateDirs {
        dir: PathBuf,
        names: Vec<String>,
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
    /// 実行中の検索を止める (バーを閉じた / ヒットを開いた / ペインを閉じた)。
    /// job_id が最新の検索と一致するときだけ実行側が cancel する
    /// (古い ID で新しい検索を殺さないためのガード)。
    CancelSearch {
        job_id: u64,
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
    /// プロパティダイアログ (SHObjectProperties — 専用 STA スレッドで
    /// 投げっぱなし。ADR 0007 追記)。
    ShowProperties {
        path: PathBuf,
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
