//! Phase 1〜2: 単一ペインのファイル一覧。
//!
//! 本計画の中核思想の実証:
//!   - 一覧/アイコン/監視/操作は `fastfiler-domain` (GUI 非依存) を**無改造で再利用**。
//!   - 描画は GPUI の `uniform_list` で**可視範囲のみ仮想化描画**。
//!   - 状態は単一の `Entity<PaneView>`。更新は `cx.notify()` の 1 ルートに統一。
//!   - watcher は `EventSink → async-channel → cx.spawn` で橋渡し。PaneView が drop
//!     されると送信端が落ちて受信ループが終了する → floem 版のスレッド/シグナル
//!     リーク (create_signal_from_channel) を構造的に排除する。
//!
//! Phase 2 のスコープ: キーボードナビ / ファイルを開く / ごみ箱削除 / ソート列切替。
//! リネーム・新規作成 (テキスト入力 UI が必要) は次フェーズ。

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use fastfiler_domain::events::EventSink;
use fastfiler_domain::file_jobs::{JobItem, JobRegistry};
use fastfiler_domain::ole_dnd::{self, DragButton, DragOutcome, DragRequest, PreferredEffect};
use fastfiler_domain::fs::{self, FileEntry};
use fastfiler_domain::search::{SearchOptions, SearchState};
use fastfiler_domain::undo::{MoveItem, TrashedItem, UndoManager, UndoOp};
use fastfiler_domain::watcher::WatcherCore;
use fastfiler_domain::user_commands::{self, RunCtx};
use fastfiler_domain::{file_ops, icons, path_util, shell, templates, win_clipboard};
use gpui::{
    AnyElement, ClickEvent, Context, DragMoveEvent, Empty, Entity, EventEmitter, ExternalPaths,
    FocusHandle, Image, ImageFormat, IntoElement, KeyDownEvent, Keystroke, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, NavigationDirection, Pixels, Point,
    ScrollStrategy, SharedString, Size, Subscription, UniformListScrollHandle, Window, anchored,
    deferred, div,
    img, prelude::*, px, relative, uniform_list,
};

use crate::hotkeys;
use crate::sink::ChannelSink;
use crate::text_input::TextInput;
use crate::theme::{self, th};

/// 入力モーダルの種類。
enum ModalKind {
    Rename { original: String },
    NewFolder,
    NewFile,
}

/// 入力モーダル (リネーム / 新規作成)。閉じると `Entity<TextInput>` ごと drop。
struct ModalState {
    kind: ModalKind,
    input: Entity<TextInput>,
}

/// 右クリックメニューの項目アクション。
#[derive(Clone)]
enum MenuAction {
    Open,
    Copy,
    Cut,
    Paste,
    Rename,
    Delete,
    NewFolder,
    /// テンプレートから新規ファイル (テンプレのフルパス)。
    NewFromTemplate(String),
    /// テンプレートフォルダを開く。
    OpenTemplatesDir,
    /// ユーザーコマンド実行 (commands.json の id)。
    RunUserCommand(String),
    /// ユーザーコマンドの設定フォルダを開く。
    OpenCommandsDir,
    /// ホットキー設定 (gpui_hotkeys.json) を既定エディタで開く。
    OpenHotkeys,
    /// ホットキー設定を再読み込み。
    ReloadHotkeys,
    Refresh,
    /// Windows のプロパティダイアログを開く (選択先頭の項目)。
    Properties,
}

/// 検索バーの状態 (Ctrl+F)。閉じると入力 Entity ごと drop。
struct SearchUi {
    input: Entity<TextInput>,
    results: Vec<SearchResult>,
    running: bool,
    job_id: Option<u64>,
    /// 状態表示 ("検索中…" / "N 件 (Everything)" 等)。
    info: SharedString,
    /// Enter で検索を実行済みか。false の間はファイル一覧を表示したまま
    /// (検索バーだけ出る)。true で結果リストに切り替わる。
    executed: bool,
}

struct SearchResult {
    path: String,
    name: String,
    is_dir: bool,
}

/// OLE ドラッグ中に自アプリ内へドロップされたことを示すフラグ。
/// `DoDragDrop` は同一スレッドのメッセージポンプ内で自ウィンドウの
/// IDropTarget (→ ExternalPaths on_drop) を呼ぶため、ドロップ処理側で立てて
/// `start_drag` 戻り後に「内部で処理済み → 元削除等をスキップ」を判定する。
static SELF_DROP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 現在の OLE ドラッグが右ボタンか (自アプリ内ドロップでチューザーを出す判定)。
static RIGHT_DRAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// OLE ドラッグの開始候補 (ボタン押下時に記録、5px 移動で発動)。
struct DragCand {
    pos: Point<Pixels>,
    paths: Vec<String>,
    /// 右ボタンドラッグか。
    right: bool,
    /// 右ボタン用: 動かず離した場合にメニューを出す対象行。
    row_ix: usize,
    /// 右ボタン用: 押下時に Shift が押されていたか (シェルメニュー)。
    shift: bool,
}

/// ラバーバンド (矩形) 選択の状態。一覧の空白部分で左ボタンを押下し、
/// ドラッグした矩形に重なる行を範囲選択する。座標はすべてウィンドウ座標。
struct RubberBand {
    /// 押下した開始位置。
    origin: Point<Pixels>,
    /// 現在のポインタ位置 (ドラッグ追従)。
    current: Point<Pixels>,
    /// Ctrl 押下で開始したか (既存選択へ加算する)。
    additive: bool,
    /// additive のときの加算元 (開始時点の選択スナップショット)。
    base: BTreeSet<usize>,
}

/// 列幅リサイズのドラッグペイロード (見出しの仕切り)。対象は固定幅列のみ。
/// on_drag_move は全ペインで発火するため、ドラッグ元ペインの id を持たせて
/// 他ペインは無視する (複数ペインが互いの右端基準で書き換え合うと幅が暴れる)。
struct ColHandle {
    col: SortCol,
    pane: gpui::EntityId,
}

/// 右ボタン D&D のドロップ先チューザー (ここにコピー / ここに移動 / キャンセル)。
struct DropMenu {
    position: Point<Pixels>,
    dst: PathBuf,
    paths: Vec<String>,
    /// `when: "drop"` のユーザーコマンド (id, ラベル)。開いた時点で取得。
    user_cmds: Vec<(String, String)>,
}

/// 同名衝突の確認待ち転送 (上書き / 別名で保存 / キャンセル)。
struct PendingTransfer {
    items: Vec<JobItem>,
    is_move: bool,
    /// 衝突しているファイル名 (表示用、先頭数件)。
    conflict_names: Vec<String>,
    conflict_count: usize,
}

/// 右クリックメニュー内のサブメニュー種別。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Submenu {
    /// 「新しいファイル ▸」: テンプレート一覧 + テンプレートフォルダを開く。
    NewFile,
    /// 「設定 ▸」: ユーザーコマンド設定 / ホットキー設定 (背景メニューのみ)。
    Settings,
}

/// 右クリックメニューの状態。
struct MenuState {
    /// ウィンドウ座標 (anchored で配置)。
    position: Point<Pixels>,
    /// 行上で開いたか (false = 背景)。
    on_row: bool,
    /// 開いた時点でクリップボードに貼り付け可能なファイルがあるか。
    can_paste: bool,
    /// 新規ファイル用テンプレート一覧 (名前, フルパス)。開いた時点で取得 (先頭20件)。
    templates: Vec<(String, String)>,
    /// 表示対象のユーザーコマンド (id, ラベル)。when/extensions で絞り込み済み。
    user_cmds: Vec<(String, String)>,
    /// 展開中のサブメニュー (hover / クリックで開く)。
    submenu: Option<Submenu>,
}

/// 分割方向。`Row`=左右に並べる / `Column`=上下に積む。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SplitDir {
    Row,
    Column,
}

/// PaneView がコンテナ (タブ) へ送るイベント。
pub enum PaneEvent {
    /// このペインが操作された → フォーカス対象にする。
    Activated,
    /// このペインを指定方向に分割してほしい。
    SplitRequested(SplitDir),
    /// このペインを閉じてほしい。
    CloseRequested,
    /// タブ内の次のペインへフォーカスを回してほしい (F6)。
    FocusNextPane,
    /// タブを相対移動してほしい (Ctrl+Tab = +1 / Ctrl+Shift+Tab = -1)。
    SwitchTab(i32),
    /// ロック中タブでの移動要求 → このパスを新しいタブで開いてほしい。
    OpenInNewTab(PathBuf),
}

impl EventEmitter<PaneEvent> for PaneView {}

/// 生存中の PaneView 数 (メモリ目標の可視化 / リーク検出用)。
/// `new` で +1、`Drop` で -1。タブ/ペインを閉じてベースラインへ戻るかを確認する。
/// floem 版で増殖していたのはまさにこのライフサイクルだった。
pub static PANES_ALIVE: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// 並べ替え基準の列。
#[derive(Clone, Copy, PartialEq, Eq)]
enum SortCol {
    Name,
    Modified,
    Size,
    Type,
}

/// 1 ペイン分の状態 + 描画。
pub struct PaneView {
    cur_path: PathBuf,
    entries: Vec<FileEntry>,
    /// 各行のアイコン (entries と同じ index)。拡張子/フォルダ単位で共有。
    row_icons: Vec<Option<Arc<Image>>>,
    /// キーボード/操作の現在位置 (エクスプローラのフォーカス項目相当)。
    cursor: Option<usize>,
    /// 選択中の行 index 集合 (複数選択)。
    selected: BTreeSet<usize>,
    /// Shift 範囲選択の起点。
    anchor: Option<usize>,
    scroll: UniformListScrollHandle,
    status: SharedString,

    sort_col: SortCol,
    sort_asc: bool,

    /// 列幅 [更新日時, サイズ, 種類] (px)。ペイン個別 — 見出しの仕切りドラッグで
    /// 変更し、セッションにペイン単位で保存・復元される。
    col_widths: [f32; 3],

    /// タブロック中か (タブの中ボタンクリックで切替)。ロック中は移動できず、
    /// フォルダへ入ろうとすると新しいタブで開く (OpenInNewTab を emit)。
    locked: bool,

    /// app 側 (ペイン切替) からも focus できるよう crate 公開。
    pub(crate) focus_handle: FocusHandle,
    focused_once: bool,
    /// watcher バースト対策の reload デバウンスフラグ。
    reload_pending: bool,

    /// リネーム / 新規作成の入力モーダル。
    modal: Option<ModalState>,
    /// 右クリックメニュー。
    context_menu: Option<MenuState>,
    /// 検索バー (Ctrl+F)。開いている間は一覧の代わりに結果を表示。
    search_ui: Option<SearchUi>,
    /// 検索の実行状態 (前回検索の自動キャンセルを管理)。
    searcher: Arc<SearchState>,
    /// 実行中の移動ジョブの Undo 候補 (job_id, 移動アイテム)。完了 (ok) 時に
    /// グローバル履歴へ push する (ADR 0006/0008)。
    pending_move_undo: Option<(u64, Vec<MoveItem>)>,
    /// ナビゲーション履歴 (戻る / 進む)。
    history_back: Vec<PathBuf>,
    history_fwd: Vec<PathBuf>,
    /// アドレスバーの直接入力モード (パス文字列クリックで開始)。
    path_edit: Option<Entity<TextInput>>,
    /// `path_edit` 入力欄の blur 購読。フォーカスが外れたら編集を破棄するため保持する。
    path_edit_blur: Option<Subscription>,
    /// OLE ドラッグ開始候補 (行でボタン押下時に記録、5px 移動で発動)。
    drag_candidate: Option<DragCand>,
    /// ラバーバンド (矩形) 選択中の状態 (空白部分で左ドラッグ中のみ Some)。
    rubber: Option<RubberBand>,
    /// 右ボタン D&D のドロップチューザー。
    drop_menu: Option<DropMenu>,
    /// 同名衝突の確認待ち転送。
    pending_transfer: Option<PendingTransfer>,

    // --- domain 連携 (watcher / ファイルジョブ) ---
    watcher: Arc<WatcherCore>,
    sink: Arc<dyn EventSink>,
    /// 現在 watch 中のパス (navigate 時に unwatch するため保持)。
    watched: Option<String>,
    /// コピー/移動ジョブのレジストリ (キャンセルフラグ管理)。
    jobs: Arc<JobRegistry>,
    next_job_id: u64,
    /// 実行中ジョブの進捗表示 (footer で status より優先)。
    job_status: Option<SharedString>,
    /// 実行中ジョブの id (Esc / キャンセルボタンの対象。直近開始分)。
    active_job: Option<u64>,
}

/// グローバル Undo 履歴 (ADR 0008 D1)。どのペイン / タブで Ctrl+Z を押しても
/// 直近の操作 1 つを戻す。ペインを閉じても履歴は残る。
fn undo_store() -> &'static Mutex<UndoManager> {
    static STORE: OnceLock<Mutex<UndoManager>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(UndoManager::new()))
}

impl PaneView {
    pub fn new(path: PathBuf, cx: &mut Context<Self>) -> Self {
        PANES_ALIVE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (sink, rx) = ChannelSink::new();
        let sink: Arc<dyn EventSink> = Arc::new(sink);

        // domain イベントを UI スレッドへ流す drain ループ。
        // PaneView drop → sink/watcher drop → rx 閉 → このループ終了 (リーク無し)。
        cx.spawn(async move |this, cx| {
            while let Ok((event, payload)) = rx.recv().await {
                if this
                    .update(cx, |pane, cx| pane.on_domain_event(&event, payload, cx))
                    .is_err()
                {
                    break; // entity が既に drop 済み
                }
            }
        })
        .detach();

        let mut this = Self {
            cur_path: path.clone(),
            entries: Vec::new(),
            row_icons: Vec::new(),
            cursor: None,
            selected: BTreeSet::new(),
            anchor: None,
            scroll: UniformListScrollHandle::new(),
            status: SharedString::default(),
            sort_col: SortCol::Name,
            sort_asc: true,
            col_widths: [132.0, 96.0, 96.0],
            locked: false,
            focus_handle: cx.focus_handle(),
            focused_once: false,
            reload_pending: false,
            modal: None,
            context_menu: None,
            search_ui: None,
            searcher: Arc::new(SearchState::default()),
            pending_move_undo: None,
            history_back: Vec::new(),
            history_fwd: Vec::new(),
            path_edit: None,
            path_edit_blur: None,
            drag_candidate: None,
            rubber: None,
            drop_menu: None,
            pending_transfer: None,
            watcher: Arc::new(WatcherCore::default()),
            sink,
            watched: None,
            jobs: Arc::new(JobRegistry::default()),
            next_job_id: 1,
            job_status: None,
            active_job: None,
        };
        // 初期表示は履歴に積まない。
        this.open_inner(path, cx, true);
        this
    }

    /// ユーザー操作による移動: 履歴 (戻る) に積んでから開く。
    /// タブロック中は移動せず、新しいタブで開くようコンテナへ依頼する。
    fn navigate(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if path == self.cur_path {
            return;
        }
        if self.locked {
            cx.emit(PaneEvent::OpenInNewTab(path));
            return;
        }
        self.history_back.push(self.cur_path.clone());
        self.history_fwd.clear();
        self.open_inner(path, cx, true);
    }

    /// 履歴: 戻る (Alt+← / マウス第4ボタン)。ロック中は不可。
    fn go_back(&mut self, cx: &mut Context<Self>) {
        if self.locked {
            self.status = "タブはロックされています".into();
            cx.notify();
            return;
        }
        if let Some(p) = self.history_back.pop() {
            self.history_fwd.push(self.cur_path.clone());
            self.open_inner(p, cx, true);
        }
    }

    /// 履歴: 進む (Alt+→ / マウス第5ボタン)。ロック中は不可。
    fn go_forward(&mut self, cx: &mut Context<Self>) {
        if self.locked {
            self.status = "タブはロックされています".into();
            cx.notify();
            return;
        }
        if let Some(p) = self.history_fwd.pop() {
            self.history_back.push(self.cur_path.clone());
            self.open_inner(p, cx, true);
        }
    }

    /// 表示フォルダを切り替える: 旧 watch を外し、新 watch を張り、再読込。
    /// (履歴は積まない。履歴管理は navigate / go_back / go_forward 側)
    fn open_inner(&mut self, path: PathBuf, cx: &mut Context<Self>, reset_view: bool) {
        // フォルダ移動したら検索モードは閉じる (結果が古くなるため)。
        if self.search_ui.take().is_some() {
            self.searcher.cancel();
        }
        if let Some(old) = self.watched.take() {
            self.watcher.unwatch(&old);
        }
        self.cur_path = path;
        let p = self.cur_path.to_string_lossy().to_string();
        if self.watcher.watch_with_sink(p.clone(), self.sink.clone()).is_ok() {
            self.watched = Some(p);
        }
        self.reload(cx, reset_view);
    }

    /// 表示中フォルダを domain から読み直す。
    /// `reset_view=false` のときは選択を名前で復元しスクロールも維持 (watcher 自動更新用)。
    fn reload(&mut self, cx: &mut Context<Self>, reset_view: bool) {
        // 自動更新時はカーソル/選択を「名前」で記憶して復元する。
        let keep: Option<(Option<String>, HashSet<String>)> = if reset_view {
            None
        } else {
            let cursor_name = self
                .cursor
                .and_then(|i| self.entries.get(i))
                .map(|e| e.name.clone());
            let sel_names: HashSet<String> = self
                .selected
                .iter()
                .filter_map(|&i| self.entries.get(i))
                .map(|e| e.name.clone())
                .collect();
            Some((cursor_name, sel_names))
        };

        let path = self.cur_path.to_string_lossy().to_string();
        match fs::list_dir(path) {
            Ok(mut v) => {
                self.sort_entries(&mut v);
                let n = v.len();
                self.row_icons = load_row_icons(&v, &self.cur_path);
                self.entries = v;
                // パスは上部アドレスバーに常時出ているのでフッターでは項目数のみ。
                self.status = format!("{} 項目", n).into();
            }
            Err(e) => {
                self.entries.clear();
                self.row_icons.clear();
                self.status = format!("読み込みエラー: {e}").into();
            }
        }

        if let Some((cursor_name, sel_names)) = keep {
            self.cursor =
                cursor_name.and_then(|n| self.entries.iter().position(|e| e.name == n));
            self.selected = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| sel_names.contains(&e.name))
                .map(|(i, _)| i)
                .collect();
            self.anchor = self.cursor;
        } else {
            self.cursor = None;
            self.anchor = None;
            self.selected.clear();
        }
        if reset_view {
            self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        }
        cx.notify();
    }

    /// フォルダを常に先頭にし、選択列で昇順/降順に並べ替える。
    fn sort_entries(&self, v: &mut [FileEntry]) {
        let col = self.sort_col;
        let asc = self.sort_asc;
        v.sort_by(|a, b| {
            let ad = a.kind == "dir";
            let bd = b.kind == "dir";
            let dir_ord = bd.cmp(&ad); // dir(true) を先頭へ
            if dir_ord != Ordering::Equal {
                return dir_ord;
            }
            let ord = match col {
                SortCol::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortCol::Modified => a.modified.cmp(&b.modified),
                SortCol::Size => a.size.cmp(&b.size),
                SortCol::Type => a
                    .ext
                    .cmp(&b.ext)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            };
            if asc { ord } else { ord.reverse() }
        });
    }

    /// 列見出しの仕切りドラッグ → このペインの列幅を更新。
    /// 仕切りは各列の左端にあり、右端基準で幅を逆算する。
    /// (notify → app の observe → セッション保存、でペイン単位に永続化される)
    fn on_col_handle_drag(&mut self, e: &DragMoveEvent<ColHandle>, cx: &mut Context<Self>) {
        // ドラッグ元のペインだけが処理する (他ペインの右端で計算すると幅が暴れる)。
        if e.drag(cx).pane != cx.entity_id() {
            return;
        }
        let [dw, sw, tw] = self.col_widths;
        let gap = 8.0; // 行の gap_2
        // ペイン右端 - px_2 の右パディング = 種類列の右端。
        let right = e.bounds.right() / px(1.0) - 8.0;
        let x = e.event.position.x / px(1.0);
        match e.drag(cx).col {
            SortCol::Type => {
                self.col_widths[2] = (right - x).clamp(40.0, 400.0);
            }
            SortCol::Size => {
                self.col_widths[1] = (right - tw - gap - x).clamp(40.0, 400.0);
            }
            SortCol::Modified => {
                self.col_widths[0] = (right - tw - gap - sw - gap - x).clamp(40.0, 400.0);
            }
            SortCol::Name => {}
        }
        let _ = dw;
        cx.notify();
    }

    fn set_sort(&mut self, col: SortCol, cx: &mut Context<Self>) {
        if self.sort_col == col {
            self.sort_asc = !self.sort_asc;
        } else {
            self.sort_col = col;
            self.sort_asc = true;
        }
        self.reload(cx, false);
    }

    fn go_up(&mut self, cx: &mut Context<Self>) {
        if let Some(parent) = self.cur_path.parent() {
            let parent = parent.to_path_buf();
            self.navigate(parent, cx);
        }
    }

    /// 外部 (ワークスペースツリー等) からフォルダを開く。
    pub fn open_dir(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.navigate(path, cx);
    }

    /// タブ見出し用の名前。どのドライブか常に分かるよう、ローカルは「C: 名前」、
    /// ネットワーク (UNC) は「🌐 名前」を必ず前置する。
    pub fn title(&self) -> String {
        let s = self.cur_path.to_string_lossy();
        let name = self
            .cur_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string());
        if s.starts_with(r"\\") {
            // UNC: share ルート (file_name なし) はパス全体を出す。
            match name {
                Some(n) => format!("🌐 {n}"),
                None => format!("🌐 {s}"),
            }
        } else {
            let drive = s
                .chars()
                .next()
                .filter(|c| c.is_ascii_alphabetic() && s.chars().nth(1) == Some(':'))
                .map(|c| c.to_ascii_uppercase());
            match (drive, name) {
                (Some(d), Some(n)) => format!("{d}: {n}"),
                // ドライブルート ("C:\") はそのまま。
                (Some(d), None) => format!("{d}:\\"),
                (None, Some(n)) => n,
                (None, None) => s.to_string(),
            }
        }
    }

    pub fn cur_path(&self) -> &Path {
        &self.cur_path
    }

    /// 列幅 [更新日時, サイズ, 種類] (セッション保存用)。
    pub fn col_widths(&self) -> [f32; 3] {
        self.col_widths
    }

    /// 列幅を復元 (セッション復元用)。不正値は clamp。
    pub fn set_col_widths(&mut self, w: [f32; 3]) {
        self.col_widths = w.map(|v| v.clamp(40.0, 400.0));
    }

    /// タブロック状態を設定 (タブ単位 — 所属する全ペインに app 側が反映する)。
    pub fn set_locked(&mut self, v: bool) {
        self.locked = v;
    }

    // ── 選択モデル (cursor + selected set + anchor) ─────────────────

    /// ix だけを選択 (プレーンクリック / 通常のキー移動)。
    fn select_only(&mut self, ix: usize) {
        self.cursor = Some(ix);
        self.anchor = Some(ix);
        self.selected.clear();
        self.selected.insert(ix);
    }

    /// Ctrl+クリック: ix の選択をトグル。
    fn toggle_select(&mut self, ix: usize) {
        if !self.selected.remove(&ix) {
            self.selected.insert(ix);
        }
        self.cursor = Some(ix);
        self.anchor = Some(ix);
    }

    /// Shift: anchor〜ix の範囲選択 (置換)。
    fn select_range_from_anchor(&mut self, ix: usize) {
        let a = self.anchor.unwrap_or(ix);
        let (s, e) = if a <= ix { (a, ix) } else { (ix, a) };
        self.selected = (s..=e).collect();
        self.cursor = Some(ix);
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = (0..self.entries.len()).collect();
        self.anchor = Some(0);
        if self.cursor.is_none() {
            self.cursor = Some(0);
        }
        cx.notify();
    }

    /// 選択中 (複数) の項目のフルパス一覧。
    fn selected_paths(&self) -> Vec<String> {
        self.selected
            .iter()
            .filter_map(|&i| self.entries.get(i))
            .map(|e| self.cur_path.join(&e.name).to_string_lossy().to_string())
            .collect()
    }

    /// カーソル項目を「開く」: フォルダ→移動 / ファイル→既定アプリ。
    fn activate_selected(&mut self, cx: &mut Context<Self>) {
        let Some(ix) = self.cursor else { return };
        let Some(entry) = self.entries.get(ix) else { return };
        let path = self.cur_path.join(&entry.name);
        if entry.kind == "dir" {
            self.navigate(path, cx);
        } else {
            self.open_in_shell(path, cx);
        }
    }

    fn open_in_shell(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        // ShellExecuteW は関連付け先 (Office の DDE 等) でメッセージポンプが回り、
        // UI スレッドの update サイクル中に呼ぶと "RefCell already borrowed" で
        // 落ちるため、専用 STA スレッドで実行し結果だけ受け取る。
        let path = path.to_string_lossy().to_string();
        cx.spawn(async move |this, cx| {
            let err: Option<String> = cx
                .background_executor()
                .spawn(async move {
                    match shell::open_with_shell_async(path).join() {
                        Ok(Ok(())) => None,
                        Ok(Err(e)) => Some(e.to_string()),
                        Err(_) => Some("シェル起動スレッドが異常終了しました".into()),
                    }
                })
                .await;
            if let Some(e) = err {
                let _ = this.update(cx, |pane, cx| {
                    pane.status = format!("開けません: {e}").into();
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// 選択中 (複数可) の項目をごみ箱へ。watcher でも更新されるが即時反映のため明示 reload。
    fn delete_selected(&mut self, cx: &mut Context<Self>) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        // Undo 用に削除前のメタデータを記録 (restore_from_trash がこれで照合する)。
        let now = std::time::SystemTime::now();
        let trashed: Vec<TrashedItem> = self
            .selected
            .iter()
            .filter_map(|&i| self.entries.get(i))
            .map(|e| TrashedItem {
                original_path: self.cur_path.join(&e.name),
                file_name: std::ffi::OsString::from(e.name.as_str()),
                size: e.size,
                modified: std::time::UNIX_EPOCH
                    + std::time::Duration::from_secs(e.modified.max(0) as u64),
                is_dir: e.kind == "dir",
                deleted_at: now,
            })
            .collect();
        let n = paths.len();
        match file_ops::delete_to_trash(paths) {
            Ok(()) => {
                undo_store().lock().unwrap().push(UndoOp::Trash { items: trashed });
                if n > 1 {
                    self.status = format!("{n} 個をごみ箱へ移動しました").into();
                }
                self.reload(cx, false);
            }
            Err(e) => {
                self.status = format!("削除に失敗: {e}").into();
                cx.notify();
            }
        }
    }

    /// 直近の操作を元に戻す (Ctrl+Z)。リネーム / 移動 / ごみ箱送りが対象 (ADR 0006/0008)。
    /// 部分失敗は失敗分だけ履歴へ push し直す (次の Ctrl+Z で再試行 — ADR 0008 S2)。
    fn undo_last(&mut self, cx: &mut Context<Self>) {
        // ロック内は取り出しのみ (ADR 0008 S5)。I/O はロック外で行う。
        let op = undo_store().lock().unwrap().pop();
        let Some(op) = op else {
            self.status = "元に戻す操作はありません".into();
            cx.notify();
            return;
        };
        let label = op.label();
        // (成功数, 失敗数, 最初のエラー, 再試行用に積み直す op)
        let (done, failed, first_err, retry): (usize, usize, Option<String>, Option<UndoOp>) =
            match op {
                UndoOp::Rename { from, to } => {
                    match file_ops::rename_path_no_overwrite(&to, &from) {
                        Ok(()) => (1, 0, None, None),
                        Err(e) => {
                            (0, 1, Some(e.to_string()), Some(UndoOp::Rename { from, to }))
                        }
                    }
                }
                UndoOp::Move { items } => {
                    let mut ok = 0usize;
                    let mut ng = Vec::new();
                    let mut err = None;
                    for it in items {
                        match file_ops::move_path_no_overwrite(&it.to, &it.from) {
                            Ok(()) => ok += 1,
                            Err(e) => {
                                err.get_or_insert_with(|| e.to_string());
                                ng.push(it);
                            }
                        }
                    }
                    let n = ng.len();
                    let retry = (!ng.is_empty()).then_some(UndoOp::Move { items: ng });
                    (ok, n, err, retry)
                }
                UndoOp::Trash { items } => {
                    let mut ok = 0usize;
                    let mut ng = Vec::new();
                    let mut err = None;
                    for it in items {
                        match file_ops::restore_from_trash(&it) {
                            Ok(()) => ok += 1,
                            Err(e) => {
                                err.get_or_insert_with(|| e.to_string());
                                ng.push(it);
                            }
                        }
                    }
                    let n = ng.len();
                    let retry = (!ng.is_empty()).then_some(UndoOp::Trash { items: ng });
                    (ok, n, err, retry)
                }
            };
        if let Some(retry_op) = retry {
            undo_store().lock().unwrap().push(retry_op);
        }
        self.status = if failed == 0 {
            format!("元に戻しました: {label}").into()
        } else if done > 0 {
            format!("元に戻しました {done} 件 / 失敗 {failed} 件 (Ctrl+Z で再試行)").into()
        } else {
            let e = first_err.unwrap_or_default();
            format!("元に戻せませんでした: {e} (Ctrl+Z で再試行)").into()
        };
        self.reload(cx, false);
    }

    /// カーソル移動。extend=true (Shift) なら anchor からの範囲選択。
    fn move_cursor(&mut self, delta: isize, extend: bool, cx: &mut Context<Self>) {
        let n = self.entries.len() as isize;
        if n == 0 {
            return;
        }
        let cur = self.cursor.map(|i| i as isize).unwrap_or(-1);
        let next = (cur + delta).clamp(0, n - 1) as usize;
        if extend {
            self.select_range_from_anchor(next);
        } else {
            self.select_only(next);
        }
        self.scroll.scroll_to_item(next, ScrollStrategy::Nearest);
        cx.notify();
    }

    /// 指定 index へジャンプ (Home/End)。extend=true なら範囲選択。
    fn jump_to(&mut self, ix: usize, extend: bool, cx: &mut Context<Self>) {
        if ix < self.entries.len() {
            if extend {
                self.select_range_from_anchor(ix);
            } else {
                self.select_only(ix);
            }
            self.scroll.scroll_to_item(ix, ScrollStrategy::Nearest);
            cx.notify();
        }
    }

    fn on_key(&mut self, ks: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        // モーダル表示中は Enter/Esc だけを処理 (Delete 等の誤爆を防ぐ)。
        if self.modal.is_some() {
            match ks.key.as_str() {
                "enter" => self.confirm_modal(window, cx),
                "escape" => self.cancel_modal(window, cx),
                _ => {}
            }
            return;
        }

        // コンテキストメニュー表示中: Esc で閉じ、他キーは無視。
        if self.context_menu.is_some() {
            if ks.key.as_str() == "escape" {
                self.context_menu = None;
                cx.notify();
            }
            return;
        }

        // 右ボタン D&D チューザー表示中: Esc でキャンセル、他キーは無視。
        if self.drop_menu.is_some() {
            if ks.key.as_str() == "escape" {
                self.drop_menu_action(None, cx);
            }
            return;
        }

        // 同名衝突の確認モーダル表示中: Esc でキャンセル、他キーは無視。
        if self.pending_transfer.is_some() {
            if ks.key.as_str() == "escape" {
                self.resolve_transfer(None, cx);
            }
            return;
        }

        // アドレスバー編集中: Enter=移動 / Esc=取消 (他キーは入力欄が処理)。
        if self.path_edit.is_some() {
            match ks.key.as_str() {
                "enter" => self.commit_path_edit(window, cx),
                "escape" => self.cancel_path_edit(window, cx),
                _ => {}
            }
            return;
        }

        // 検索バー表示中: Enter=実行 / Esc=閉じる (他キーは入力欄が処理)。
        if self.search_ui.is_some() {
            match ks.key.as_str() {
                "enter" => self.start_search(cx),
                "escape" => self.close_search(window, cx),
                _ => {}
            }
            return;
        }

        // 実行中のコピー/移動ジョブは Esc でキャンセル要求。
        if ks.key.as_str() == "escape" && self.active_job.is_some() {
            self.cancel_job(cx);
            return;
        }

        // Esc: 選択・カーソルを解除 (モーダル類やジョブが無いとき)。
        if ks.key.as_str() == "escape" {
            if !self.selected.is_empty() || self.cursor.is_some() {
                self.selected.clear();
                self.cursor = None;
                self.anchor = None;
                cx.notify();
            }
            return;
        }

        // コマンド系: カスタマイズ可能なホットキー (gpui_hotkeys.json)。
        if let Some(act) = hotkeys::lookup(ks) {
            use hotkeys::HotAction as A;
            match act {
                A::Open => self.activate_selected(cx),
                A::Parent => self.go_up(cx),
                A::Delete => self.delete_selected(cx),
                A::Rename => self.start_rename(window, cx),
                A::NewFolder => {
                    self.open_modal(ModalKind::NewFolder, String::new(), 0..0, window, cx)
                }
                A::NewFile => {
                    self.open_modal(ModalKind::NewFile, String::new(), 0..0, window, cx)
                }
                A::Refresh => self.reload(cx, false),
                A::Search => self.open_search(window, cx),
                A::Undo => self.undo_last(cx),
                A::Copy => self.clipboard_copy("copy", cx),
                A::Cut => self.clipboard_copy("cut", cx),
                A::Paste => self.paste(cx),
                A::SelectAll => self.select_all(cx),
                A::Back => self.go_back(cx),
                A::Forward => self.go_forward(cx),
                A::NextPane => cx.emit(PaneEvent::FocusNextPane),
                A::NextTab => cx.emit(PaneEvent::SwitchTab(1)),
                A::PrevTab => cx.emit(PaneEvent::SwitchTab(-1)),
            }
            return;
        }

        // 移動系は固定 (Shift で範囲選択拡張)。
        let shift = ks.modifiers.shift;
        match ks.key.as_str() {
            "up" => self.move_cursor(-1, shift, cx),
            "down" => self.move_cursor(1, shift, cx),
            "pageup" => self.move_cursor(-10, shift, cx),
            "pagedown" => self.move_cursor(10, shift, cx),
            "home" => self.jump_to(0, shift, cx),
            "end" => {
                if !self.entries.is_empty() {
                    self.jump_to(self.entries.len() - 1, shift, cx);
                }
            }
            _ => {}
        }
    }

    /// リネームモーダルを開く (カーソル項目の名前を初期値に、拡張子手前まで選択)。
    fn start_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let info = self
            .cursor
            .and_then(|ix| self.entries.get(ix))
            .map(|e| (e.name.clone(), e.kind == "dir"));
        if let Some((name, is_dir)) = info {
            let stem_end = if is_dir {
                name.len()
            } else {
                name.rfind('.').filter(|&i| i > 0).unwrap_or(name.len())
            };
            self.open_modal(
                ModalKind::Rename {
                    original: name.clone(),
                },
                name,
                0..stem_end,
                window,
                cx,
            );
        }
    }

    fn open_modal(
        &mut self,
        kind: ModalKind,
        initial: String,
        select: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| TextInput::new(cx));
        input.update(cx, |i, cx| i.set_text_and_select(initial, select, cx));
        let fh = input.read(cx).focus_handle.clone();
        self.modal = Some(ModalState { kind, input });
        fh.focus(window, cx);
        cx.notify();
    }

    fn confirm_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(modal) = self.modal.take() else {
            return;
        };
        let text = modal.input.read(cx).text().trim().to_string();
        self.focus_handle.focus(window, cx);
        if text.is_empty() {
            cx.notify();
            return;
        }
        let result: Result<(), String> = match &modal.kind {
            ModalKind::Rename { original } => {
                if *original == text {
                    Ok(())
                } else {
                    let from = self.cur_path.join(original);
                    let to = self.cur_path.join(&text);
                    match file_ops::rename_path_no_overwrite(&from, &to) {
                        Ok(()) => {
                            // Undo 用に記録 (Ctrl+Z で to → from へ戻す)。
                            undo_store().lock().unwrap().push(UndoOp::Rename { from, to });
                            Ok(())
                        }
                        Err(e) => Err(e.to_string()),
                    }
                }
            }
            ModalKind::NewFolder => {
                file_ops::create_dir(&self.cur_path.join(&text)).map_err(|e| e.to_string())
            }
            // 空ファイル作成は std で十分 (既存は上書きしない)。domain は凍結中。
            ModalKind::NewFile => std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.cur_path.join(&text))
                .map(|_| ())
                .map_err(|e| e.to_string()),
        };
        match result {
            Ok(()) => {
                self.reload(cx, false);
                if let Some(ix) = self.entries.iter().position(|e| e.name == text) {
                    self.select_only(ix);
                }
                cx.notify();
            }
            Err(e) => {
                self.status = format!("失敗: {e}").into();
                cx.notify();
            }
        }
    }

    fn cancel_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.take().is_some() {
            self.focus_handle.focus(window, cx);
            cx.notify();
        }
    }

    // ── アドレスバー直接入力 ────────────────────────────────────────

    fn start_path_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| TextInput::new(cx));
        let text = self.cur_path.display().to_string();
        let len = text.len();
        input.update(cx, |i, cx| i.set_text_and_select(text, 0..len, cx));
        let fh = input.read(cx).focus_handle.clone();
        // 入力欄がフォーカスを失ったら (別の場所をクリックした等) 編集を破棄する。
        // Esc と異なりフォーカスはペインへ奪い返さない (ユーザのクリック先を尊重する)。
        let blur = cx.on_blur(&fh, window, |this, _window, cx| {
            if this.path_edit.take().is_some() {
                cx.notify();
            }
        });
        self.path_edit = Some(input);
        self.path_edit_blur = Some(blur);
        fh.focus(window, cx);
        cx.notify();
    }

    fn commit_path_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.path_edit.take() else {
            return;
        };
        self.path_edit_blur = None;
        let text = input.read(cx).text().trim().to_string();
        self.focus_handle.focus(window, cx);
        if text.is_empty() {
            cx.notify();
            return;
        }
        let p = PathBuf::from(&text);
        if p.is_dir() {
            self.navigate(p, cx);
        } else {
            self.status = format!("フォルダが見つかりません: {text}").into();
            cx.notify();
        }
    }

    fn cancel_path_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.path_edit.take().is_some() {
            self.path_edit_blur = None;
            self.focus_handle.focus(window, cx);
            cx.notify();
        }
    }

    // ── 検索 (Ctrl+F) ──────────────────────────────────────────────

    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ui) = &self.search_ui {
            // 既に開いている → 入力へフォーカスだけ戻す。
            let fh = ui.input.read(cx).focus_handle.clone();
            fh.focus(window, cx);
            return;
        }
        let input = cx.new(|cx| TextInput::new(cx));
        let fh = input.read(cx).focus_handle.clone();
        self.search_ui = Some(SearchUi {
            input,
            results: Vec::new(),
            running: false,
            job_id: None,
            info: "Enter で検索".into(),
            executed: false,
        });
        fh.focus(window, cx);
        cx.notify();
    }

    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_ui.take().is_some() {
            self.searcher.cancel();
            self.focus_handle.focus(window, cx);
            cx.notify();
        }
    }

    /// 入力中のパターンで検索開始 (このペインの表示フォルダ起点)。
    /// Everything (HTTP, port 80) が応答すれば利用し、不達なら内蔵検索へ自動フォールバック。
    fn start_search(&mut self, cx: &mut Context<Self>) {
        let root = self.cur_path.to_string_lossy().to_string();
        let sink = self.sink.clone();
        let Some(ui) = self.search_ui.as_mut() else {
            return;
        };
        let pattern = ui.input.read(cx).text().trim().to_string();
        if pattern.is_empty() {
            return;
        }
        ui.results.clear();
        let opts = SearchOptions {
            case_sensitive: false,
            use_regex: false,
            include_hidden: true,
            max_results: 2000,
            backend: "everything".to_string(),
            everything_port: crate::settings_store::get().everything_port,
            everything_scope: true,
        };
        match self.searcher.start_with_sink(sink, root, pattern, opts) {
            Ok(id) => {
                ui.job_id = Some(id);
                ui.running = true;
                // 実行して初めて結果リストへ切り替える (それまでは一覧のまま)。
                ui.executed = true;
                ui.info = "検索中…".into();
            }
            Err(e) => {
                ui.info = format!("検索エラー: {e}").into();
            }
        }
        cx.notify();
    }

    /// 検索結果へジャンプ: フォルダ→開く / ファイル→親を開いて選択。
    fn open_search_result(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((path, name, is_dir)) = self
            .search_ui
            .as_ref()
            .and_then(|ui| ui.results.get(ix))
            .map(|r| (r.path.clone(), r.name.clone(), r.is_dir))
        else {
            return;
        };
        self.close_search(window, cx);
        let p = PathBuf::from(&path);
        if is_dir {
            self.navigate(p, cx);
        } else if let Some(parent) = p.parent() {
            self.navigate(parent.to_path_buf(), cx);
            if let Some(i) = self.entries.iter().position(|e| e.name == name) {
                self.select_only(i);
                self.scroll.scroll_to_item(i, ScrollStrategy::Center);
            }
        }
        cx.notify();
    }

    fn render_search_row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(r) = self.search_ui.as_ref().and_then(|ui| ui.results.get(ix)) else {
            return div().into_any_element();
        };
        let name = r.name.clone();
        let parent = Path::new(&r.path)
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let accent = if r.is_dir { th().accent } else { th().accent_file };

        div()
            .id(ix)
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .h(theme::row_h())
            .px_2()
            .gap_2()
            .bg(if ix % 2 == 0 {
                th().row_even
            } else {
                th().row_odd
            })
            .cursor_pointer()
            .hover(|s| s.bg(th().hover_bg))
            .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| {
                if e.click_count() > 1 {
                    this.open_search_result(ix, window, cx);
                }
            }))
            .child(div().w(px(6.0)).flex_shrink_0().h(px(14.0)).rounded(theme::radius_sm()).bg(accent))
            .child(div().flex_shrink_0().text_color(th().text).child(name))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .truncate()
                    .text_color(th().text_faint)
                    .child(parent),
            )
            .into_any_element()
    }

    // ── 右クリックメニュー ─────────────────────────────────────────

    fn open_menu(&mut self, position: Point<Pixels>, on_row: bool, cx: &mut Context<Self>) {
        let can_paste = win_clipboard::clipboard_read_paths()
            .ok()
            .flatten()
            .map(|c| !c.paths.is_empty())
            .unwrap_or(false);
        // 新規ファイル用テンプレート (%APPDATA%\fastfiler\templates、先頭20件)。
        let templates: Vec<(String, String)> = templates::list_templates()
            .map(|v| v.into_iter().take(20).map(|t| (t.name, t.path)).collect())
            .unwrap_or_default();
        // ユーザーコマンド (commands.json) を when / extensions で絞り込み。
        let cursor_entry = self.cursor.and_then(|i| self.entries.get(i));
        let (sel_is_dir, sel_ext) = match cursor_entry {
            Some(e) => (e.kind == "dir", e.ext.clone()),
            None => (false, None),
        };
        let user_cmds: Vec<(String, String)> = user_commands::list_user_commands()
            .map(|v| {
                v.into_iter()
                    .filter(|c| {
                        if on_row {
                            let when_ok = match c.when.as_str() {
                                "file" => !sel_is_dir,
                                "dir" | "folder" => sel_is_dir,
                                // 行専用 (ファイル・フォルダ両方)。背景には出さない。
                                "selection" => true,
                                // 背景専用コマンドは行メニューに出さない。
                                "background" => false,
                                _ => true, // "any" (行・背景の両方)
                            };
                            let ext_ok = c.extensions.is_empty()
                                || sel_ext
                                    .as_ref()
                                    .map(|e| {
                                        c.extensions
                                            .iter()
                                            .any(|x| x.trim_start_matches('.').eq_ignore_ascii_case(e))
                                    })
                                    .unwrap_or(false);
                            when_ok && ext_ok
                        } else {
                            // 背景メニューは選択非依存のコマンドのみ
                            // ("background" は背景専用 — commands.json.sample 参照)。
                            c.when == "any" || c.when == "background"
                        }
                    })
                    .take(10)
                    .map(|c| (c.id, c.label))
                    .collect()
            })
            .unwrap_or_default();
        self.context_menu = Some(MenuState {
            position,
            on_row,
            can_paste,
            templates,
            user_cmds,
            submenu: None,
        });
        cx.notify();
    }

    /// 開いている右クリックメニュー / 右ドラッグチューザーを閉じる
    /// (他ペインの操作時にコンテナ側から呼ばれる。メニュー類は常に 1 か所だけ表示する)。
    pub fn close_overlay_menus(&mut self, cx: &mut Context<Self>) {
        let closed = self.context_menu.take().is_some() | self.drop_menu.take().is_some();
        if closed {
            cx.notify();
        }
    }

    fn on_row_right_click(
        &mut self,
        ix: usize,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal.is_some() {
            return;
        }
        self.focus_handle.focus(window, cx);
        cx.emit(PaneEvent::Activated);
        // 選択外の行なら単一選択に。選択内ならそのまま (複数対象の操作を許す)。
        if !self.selected.contains(&ix) {
            self.select_only(ix);
        } else {
            self.cursor = Some(ix);
        }
        self.open_menu(position, true, cx);
    }

    fn on_bg_right_click(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal.is_some() || self.context_menu.is_some() {
            return;
        }
        self.focus_handle.focus(window, cx);
        cx.emit(PaneEvent::Activated);
        self.open_menu(position, false, cx);
    }

    /// Shift+右クリック: Windows 標準のシェルコンテキストメニュー (IContextMenu)。
    fn shell_context_menu(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.is_some() {
            return;
        }
        self.focus_handle.focus(window, cx);
        cx.emit(PaneEvent::Activated);
        if !self.selected.contains(&ix) {
            self.select_only(ix);
        } else {
            self.cursor = Some(ix);
        }
        cx.notify();

        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let Some(hwnd) = hwnd_of(window) else {
            return;
        };
        // ブロッキング (メニューを閉じるまで戻らない)。閉じた後は変更を反映。
        match shell::show_shell_context_menu(hwnd, paths) {
            Ok(()) => self.reload(cx, false),
            Err(e) => {
                self.status = format!("シェルメニュー失敗: {e}").into();
                cx.notify();
            }
        }
    }

    fn menu_action(&mut self, action: MenuAction, window: &mut Window, cx: &mut Context<Self>) {
        self.context_menu = None;
        match action {
            MenuAction::Open => self.activate_selected(cx),
            MenuAction::Copy => self.clipboard_copy("copy", cx),
            MenuAction::Cut => self.clipboard_copy("cut", cx),
            MenuAction::Paste => self.paste(cx),
            MenuAction::Rename => self.start_rename(window, cx),
            MenuAction::Delete => self.delete_selected(cx),
            MenuAction::NewFolder => {
                self.open_modal(ModalKind::NewFolder, String::new(), 0..0, window, cx)
            }
            MenuAction::NewFromTemplate(tpl) => self.create_from_template(tpl, window, cx),
            MenuAction::OpenTemplatesDir => {
                if let Ok(dir) = templates::templates_dir() {
                    self.navigate(PathBuf::from(dir), cx);
                }
            }
            MenuAction::RunUserCommand(id) => {
                let ctx = RunCtx {
                    paths: self.selected_paths(),
                    cwd: self.cur_path.to_string_lossy().to_string(),
                };
                self.run_user_command_bg(id, ctx, cx);
            }
            MenuAction::OpenCommandsDir => {
                if let Ok(dir) = user_commands::user_commands_dir() {
                    self.navigate(PathBuf::from(dir), cx);
                }
            }
            MenuAction::OpenHotkeys => {
                if let Some(file) = hotkeys::config_file() {
                    // UI スレッド再入対策 (open_in_shell と同様)。結果は見ない。
                    let _ = shell::open_with_shell_async(file);
                }
            }
            MenuAction::ReloadHotkeys => {
                hotkeys::load();
                self.status = "ホットキーを再読み込みしました".into();
            }
            MenuAction::Refresh => self.reload(cx, false),
            MenuAction::Properties => {
                // 選択先頭 (なければカーソル) の項目を対象に。
                let target = self.selected_paths().into_iter().next().or_else(|| {
                    self.cursor
                        .and_then(|i| self.entries.get(i))
                        .map(|e| self.cur_path.join(&e.name).to_string_lossy().to_string())
                });
                if let Some(path) = target {
                    // ダイアログを閉じるまでブロックするため投げっぱなし (UI 再入回避)。
                    let _ = shell::show_properties_async(path);
                }
            }
        }
        cx.notify();
    }

    /// テンプレートから新規ファイルを作成 (一意名)。作成後は選択してリネームへ。
    fn create_from_template(
        &mut self,
        template_path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match templates::create_file_from_template(
            template_path,
            self.cur_path.to_string_lossy().to_string(),
            None,
        ) {
            Ok(created) => {
                self.reload(cx, false);
                let name = Path::new(&created)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                if let Some(ix) = self.entries.iter().position(|e| e.name == name) {
                    self.select_only(ix);
                    self.scroll.scroll_to_item(ix, ScrollStrategy::Center);
                }
                // エクスプローラ同様、作成直後にリネームモードへ。
                self.start_rename(window, cx);
            }
            Err(e) => {
                self.status = format!("作成に失敗: {e}").into();
                cx.notify();
            }
        }
    }

    fn menu_item(
        &self,
        label: impl Into<String>,
        shortcut: &'static str,
        enabled: bool,
        action: MenuAction,
        cx: &Context<Self>,
    ) -> AnyElement {
        let label: String = label.into();
        let row = div()
            .id(SharedString::from(format!("mi-{label}")))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_3()
            .px_3()
            .py_1()
            .mx_1()
            .rounded(theme::radius_sm());
        if enabled {
            row.cursor_pointer()
                .hover(|s| s.bg(th().menu_hover))
                .on_click(cx.listener(move |this, _e, w, cx| {
                    this.menu_action(action.clone(), w, cx)
                }))
                .child(div().child(label))
                .child(div().text_color(th().text_faint).child(shortcut))
                .into_any_element()
        } else {
            row.text_color(th().text_disabled)
                .child(div().child(label))
                .child(div().text_color(th().text_disabled).child(shortcut))
                .into_any_element()
        }
    }

    /// サブメニュー付きの親項目。hover / クリックで右隣にサブメニューを展開する。
    ///
    /// `row_offset` はメニュー先頭からこの親項目までの推定オフセット (px)。
    /// ウィンドウ端では展開方向を自動調整する (下にはみ出す → 上方向、
    /// 右にはみ出す → 左方向) ため、展開位置の見積もりに使う。
    fn submenu_parent(
        &self,
        label: &'static str,
        kind: Submenu,
        m: &MenuState,
        row_offset: f32,
        viewport: Size<Pixels>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let open = m.submenu == Some(kind);
        // サブメニューの高さをざっくり見積もる (1 行 ≈ フォント×1.75 + 区切り/padding)。
        let sub_rows = match kind {
            Submenu::NewFile => m.templates.len().max(1) + 1,
            Submenu::Settings => 3,
        };
        let est_h = sub_rows as f32 * (theme::font_px() * 1.75) + 19.0;
        // メニュー anchor (右クリック位置) は snap で実際より下を指すことがあるが、
        // その場合は上方向に倒れるだけなので安全側。
        let open_up =
            m.position.y / px(1.0) + row_offset + est_h > viewport.height / px(1.0) - 8.0;
        let open_left =
            m.position.x / px(1.0) + 210.0 * 2.0 > viewport.width / px(1.0) - 8.0;
        let mut row = div()
            .id(SharedString::from(format!("smp-{label}")))
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_3()
            .px_3()
            .py_1()
            .mx_1()
            .rounded(theme::radius_sm())
            .cursor_pointer()
            .hover(|s| s.bg(th().menu_hover))
            // クリックで開閉 (素通りの hover では開かない)。
            // 開いたまま離れても閉じない — サブメニュー側へポインタを移動できるようにする。
            .on_click(cx.listener(move |this, _e, _w, cx| {
                if let Some(menu) = this.context_menu.as_mut() {
                    menu.submenu = if menu.submenu == Some(kind) {
                        None
                    } else {
                        Some(kind)
                    };
                    cx.notify();
                }
            }))
            .child(div().child(label))
            .child(div().text_color(th().text_faint).child("▸"));
        if open {
            // 親項目の隣に本体メニューと同スタイルのパネルを重ねる。
            // 既定は右隣・下方向。ウィンドウ端では左隣・上方向に切り替える。
            let mut panel = div()
                .occlude()
                .absolute()
                .flex()
                .flex_col()
                .py_1()
                .w(px(210.0))
                .rounded(theme::radius_md())
                .bg(th().surface_bg)
                .border_1()
                .border_color(th().button_hover)
                .text_color(th().text);
            panel = if open_left {
                panel.right(relative(1.0))
            } else {
                panel.left(relative(1.0))
            };
            panel = if open_up {
                panel.bottom(px(-4.0))
            } else {
                panel.top(px(-4.0))
            };
            row = row.child(panel.children(self.submenu_items(kind, m, cx)));
        }
        row.into_any_element()
    }

    /// サブメニューの中身を構築する。
    fn submenu_items(&self, kind: Submenu, m: &MenuState, cx: &Context<Self>) -> Vec<AnyElement> {
        let mut items: Vec<AnyElement> = Vec::new();
        match kind {
            Submenu::NewFile => {
                if m.templates.is_empty() {
                    items.push(self.menu_item(
                        "(テンプレートなし)",
                        "",
                        false,
                        MenuAction::OpenTemplatesDir,
                        cx,
                    ));
                } else {
                    for (name, path) in &m.templates {
                        items.push(self.menu_item(
                            format!("新規: {name}"),
                            "",
                            true,
                            MenuAction::NewFromTemplate(path.clone()),
                            cx,
                        ));
                    }
                }
                items.push(menu_sep());
                items.push(self.menu_item(
                    "テンプレートフォルダを開く",
                    "",
                    true,
                    MenuAction::OpenTemplatesDir,
                    cx,
                ));
            }
            Submenu::Settings => {
                items.push(self.menu_item(
                    "ユーザーコマンドの設定...",
                    "",
                    true,
                    MenuAction::OpenCommandsDir,
                    cx,
                ));
                items.push(menu_sep());
                items.push(self.menu_item(
                    "ホットキー設定を開く",
                    "",
                    true,
                    MenuAction::OpenHotkeys,
                    cx,
                ));
                items.push(self.menu_item(
                    "ホットキーを再読み込み",
                    "",
                    true,
                    MenuAction::ReloadHotkeys,
                    cx,
                ));
            }
        }
        items
    }

    /// 右クリックメニューのオーバーレイ (開いていなければ None)。
    fn render_context_menu(&self, window: &Window, cx: &Context<Self>) -> Option<AnyElement> {
        let m = self.context_menu.as_ref()?;
        let viewport = window.viewport_size();
        // サブメニュー展開方向の判定用: メニュー先頭から各親項目までの推定オフセット
        // (1 行 ≈ フォント×1.75 [既定 16px で 28px] / 区切り ≈ 9px / パネル上 padding 4px)。
        let row_h: f32 = theme::font_px() * 1.75;
        const SEP_H: f32 = 9.0;
        let newfile_offset = if m.on_row {
            4.0 + 7.0 * row_h + 3.0 * SEP_H
        } else {
            4.0 + 3.0 * row_h + SEP_H
        };
        let settings_offset = {
            let extra = if m.user_cmds.is_empty() { 0.0 } else { SEP_H };
            newfile_offset + (1.0 + m.user_cmds.len() as f32) * row_h + SEP_H + extra
        };
        let mut items: Vec<AnyElement> = Vec::new();
        if m.on_row {
            items.push(self.menu_item("開く", "Enter", true, MenuAction::Open, cx));
            items.push(menu_sep());
            items.push(self.menu_item("コピー", "Ctrl+C", true, MenuAction::Copy, cx));
            items.push(self.menu_item("切り取り", "Ctrl+X", true, MenuAction::Cut, cx));
            items.push(self.menu_item("貼り付け", "Ctrl+V", m.can_paste, MenuAction::Paste, cx));
            items.push(menu_sep());
            items.push(self.menu_item("名前の変更", "F2", true, MenuAction::Rename, cx));
            items.push(self.menu_item("削除", "Del", true, MenuAction::Delete, cx));
            items.push(menu_sep());
        } else {
            items.push(self.menu_item("貼り付け", "Ctrl+V", m.can_paste, MenuAction::Paste, cx));
            items.push(self.menu_item("最新の情報に更新", "F5", true, MenuAction::Refresh, cx));
            items.push(menu_sep());
        }
        items.push(self.menu_item("新しいフォルダ", "F7", true, MenuAction::NewFolder, cx));
        // 「新しいファイル ▸」: テンプレート一覧のサブメニュー (空ファイル作成は F8)。
        items.push(self.submenu_parent(
            "新しいファイル",
            Submenu::NewFile,
            m,
            newfile_offset,
            viewport,
            cx,
        ));
        // ユーザーコマンド (commands.json — ADR 0003 の拡張点)
        if !m.user_cmds.is_empty() {
            items.push(menu_sep());
            for (id, label) in &m.user_cmds {
                items.push(self.menu_item(
                    label.clone(),
                    "",
                    true,
                    MenuAction::RunUserCommand(id.clone()),
                    cx,
                ));
            }
        }
        // 「設定 ▸」(背景メニューのみ): ユーザーコマンド設定 / ホットキー設定。
        if !m.on_row {
            items.push(menu_sep());
            items.push(self.submenu_parent(
                "設定",
                Submenu::Settings,
                m,
                settings_offset,
                viewport,
                cx,
            ));
        }
        // プロパティ (行メニューの最下部) — Windows の SHObjectProperties。
        if m.on_row {
            items.push(menu_sep());
            items.push(self.menu_item("プロパティ", "", true, MenuAction::Properties, cx));
        }

        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                // メニュー外クリックで閉じる (左右どちらでも)。
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _e: &MouseDownEvent, _w, cx| {
                        this.context_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _e: &MouseDownEvent, _w, cx| {
                        this.context_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    deferred(
                        anchored()
                            .position(m.position)
                            .snap_to_window_with_margin(px(8.0))
                            .child(
                                div()
                                    .occlude()
                                    .flex()
                                    .flex_col()
                                    .py_1()
                                    .w(px(210.0))
                                    .rounded(theme::radius_md())
                                    .bg(th().surface_bg)
                                    .border_1()
                                    .border_color(th().button_hover)
                                    .text_color(th().text)
                                    .children(items),
                            ),
                    ),
                )
                .into_any_element(),
        )
    }

    /// 入力モーダルのオーバーレイ (開いていなければ None)。
    fn render_modal(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let m = self.modal.as_ref()?;
        let title: &'static str = match &m.kind {
            ModalKind::Rename { .. } => "名前の変更",
            ModalKind::NewFolder => "新しいフォルダ",
            ModalKind::NewFile => "新しいファイル",
        };
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .bg(th().overlay_bg)
                // 背景クリックでキャンセル。
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _e, w, cx| this.cancel_modal(w, cx)),
                )
                .child(
                    div()
                        .occlude()
                        // パネル内クリックは背景へ伝播させない。
                        .on_mouse_down(MouseButton::Left, |_e, _w, cx| cx.stop_propagation())
                        .flex()
                        .flex_col()
                        .gap_2()
                        .w(px(420.0))
                        .p_3()
                        .rounded(theme::radius_md())
                        .bg(th().surface_bg)
                        .border_1()
                        .border_color(th().accent)
                        .text_color(th().text)
                        .child(title)
                        .child(m.input.clone())
                        .child(
                            div()
                                .text_color(th().text_faint)
                                .child("Enter: 実行 / Esc: キャンセル"),
                        ),
                )
                .into_any_element(),
        )
    }

    fn on_row_click(
        &mut self,
        ix: usize,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // クリックでキーボードフォーカスをペインへ戻し、コンテナへ activate 通知。
        self.focus_handle.focus(window, cx);
        cx.emit(PaneEvent::Activated);

        if event.click_count() > 1 {
            // ダブルクリック: 単一選択にして開く。
            self.select_only(ix);
            if let Some(entry) = self.entries.get(ix) {
                let path = self.cur_path.join(&entry.name);
                if entry.kind == "dir" {
                    self.navigate(path, cx);
                    return;
                } else {
                    self.open_in_shell(path, cx);
                    return;
                }
            }
            cx.notify();
            return;
        }

        let mods = event.modifiers();
        if mods.control {
            self.toggle_select(ix);
        } else if mods.shift {
            self.select_range_from_anchor(ix);
        } else {
            self.select_only(ix);
        }
        cx.notify();
    }

    // ── ラバーバンド (矩形) 選択 ───────────────────────────────────────
    //
    // 一覧は uniform_list で仮想化描画されるため、ウィンドウ座標から行 index を
    // 直接逆算する: 行高は固定 (theme::row_h)、ビューポート上端と縦スクロール量は
    // scroll ハンドル (base_handle.bounds()/offset()) から得る。

    /// ウィンドウ座標の y がどの一覧行に当たるか。空白・範囲外は None。
    fn row_at_y(&self, y: Pixels) -> Option<usize> {
        let row_h = theme::row_h() / px(1.0);
        if row_h <= 0.0 {
            return None;
        }
        let (top, offset_y) = {
            let st = self.scroll.0.borrow();
            (st.base_handle.bounds().origin.y, st.base_handle.offset().y)
        };
        let content_y = (y - top - offset_y) / px(1.0);
        if content_y < 0.0 {
            return None;
        }
        let ix = (content_y / row_h) as usize;
        (ix < self.entries.len()).then_some(ix)
    }

    /// 一覧領域での左ボタン押下。行の上なら何もしない (行ハンドラが OLE ドラッグ
    /// 候補を記録する)。空白部分ならラバーバンド選択を開始する。
    /// Ctrl なしは既存選択を即解除する (= 空白クリックでの選択解除も兼ねる)。
    fn on_list_mouse_down(&mut self, ev: &MouseDownEvent, cx: &mut Context<Self>) {
        if self.modal.is_some() || self.context_menu.is_some() || self.drop_menu.is_some() {
            return;
        }
        if self.row_at_y(ev.position.y).is_some() {
            return;
        }
        let additive = ev.modifiers.control;
        let base = if additive {
            self.selected.clone()
        } else {
            BTreeSet::new()
        };
        if !additive {
            self.selected.clear();
            self.cursor = None;
            self.anchor = None;
        }
        self.rubber = Some(RubberBand {
            origin: ev.position,
            current: ev.position,
            additive,
            base,
        });
        cx.notify();
    }

    /// ラバーバンドのドラッグ追従: 矩形に重なる行を選択し直す。
    fn update_rubber(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(rb) = self.rubber.as_ref() else {
            return;
        };
        let origin_y = rb.origin.y;
        let additive = rb.additive;
        let mut sel: BTreeSet<usize> = rb.base.clone();
        if !additive {
            sel.clear();
        }
        // 現在位置を更新。
        if let Some(rb) = self.rubber.as_mut() {
            rb.current = pos;
        }
        let row_h = theme::row_h() / px(1.0);
        let len = self.entries.len();
        if len > 0 && row_h > 0.0 {
            let (top, offset_y) = {
                let st = self.scroll.0.borrow();
                (st.base_handle.bounds().origin.y, st.base_handle.offset().y)
            };
            let oy = origin_y / px(1.0);
            let py = pos.y / px(1.0);
            let ty = top / px(1.0);
            let off = offset_y / px(1.0);
            // ウィンドウ座標 y → 一覧コンテンツ座標 (offset は下スクロールで負)。
            let to_content = |wy: f32| wy - ty - off;
            let first = (to_content(oy.min(py)).max(0.0) / row_h) as usize;
            let last = (to_content(oy.max(py)).max(0.0) / row_h) as usize;
            for ix in first..=last.min(len - 1) {
                sel.insert(ix);
            }
        }
        self.cursor = sel.iter().next_back().copied();
        self.selected = sel;
        cx.notify();
    }

    /// ラバーバンド矩形のオーバーレイ (ドラッグ中のみ)。一覧コンテナ内へ
    /// ビューポート相対の絶対配置で重ねる。クリックは小矩形 (= 単なるクリック) では
    /// 描かない。
    fn render_rubber(&self) -> Option<AnyElement> {
        let rb = self.rubber.as_ref()?;
        let ox = rb.origin.x / px(1.0);
        let oy = rb.origin.y / px(1.0);
        let cxp = rb.current.x / px(1.0);
        let cyp = rb.current.y / px(1.0);
        let w = (cxp - ox).abs();
        let h = (cyp - oy).abs();
        if w < 2.0 && h < 2.0 {
            return None;
        }
        let origin = self.scroll.0.borrow().base_handle.bounds().origin;
        let left = ox.min(cxp) - origin.x / px(1.0);
        let top = oy.min(cyp) - origin.y / px(1.0);
        Some(
            div()
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(w))
                .h(px(h))
                .bg(th().sel_translucent)
                .border_1()
                .border_color(th().accent)
                .into_any_element(),
        )
    }

    /// 選択中 (複数可) の項目を CF_HDROP でクリップボードへ (op: "copy" | "cut")。
    fn clipboard_copy(&mut self, op: &str, cx: &mut Context<Self>) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let n = paths.len();
        let first_name = Path::new(&paths[0])
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        match win_clipboard::clipboard_write_paths(paths, op.to_string()) {
            Ok(()) => {
                let verb = if op == "cut" { "切り取り" } else { "コピー" };
                self.status = if n > 1 {
                    format!("{verb}: {n} 個").into()
                } else {
                    format!("{verb}: {first_name}").into()
                };
            }
            Err(e) => {
                self.status = format!("失敗: {e}").into();
            }
        }
        cx.notify();
    }

    /// クリップボードの CF_HDROP を現在フォルダへ貼り付け (copy/move ジョブ)。
    /// ジョブは別スレッドで実行し、進捗は sink 経由で `fs:job:progress` が届く。
    fn paste(&mut self, cx: &mut Context<Self>) {
        let clip = match win_clipboard::clipboard_read_paths() {
            Ok(Some(c)) if !c.paths.is_empty() => c,
            Ok(_) => {
                self.status = "クリップボードにファイルがありません".into();
                cx.notify();
                return;
            }
            Err(e) => {
                self.status = format!("失敗: {e}").into();
                cx.notify();
                return;
            }
        };
        let items = build_job_items(&clip.paths, &self.cur_path);
        if items.is_empty() {
            self.status = "同じ場所への貼り付けはスキップしました".into();
            cx.notify();
            return;
        }
        self.run_transfer(items, clip.op == "cut", cx);
    }

    /// ペイン間 / 外部からのドロップを dst_dir へ転送。
    /// 効果は修飾キー優先 (Ctrl=コピー / Shift=移動)、なしはエクスプローラ準拠
    /// (同一ボリューム=移動 / 異なる=コピー)。
    fn drop_paths_into(
        &mut self,
        dst_dir: PathBuf,
        paths: Vec<String>,
        forced_move: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        let items = build_job_items(&paths, &dst_dir);
        if items.is_empty() {
            return;
        }
        let is_move = forced_move.unwrap_or_else(|| {
            let src_vol = path_util::volume_key(Path::new(&items[0].from));
            let dst_vol = path_util::volume_key(&dst_dir);
            src_vol.is_some() && src_vol == dst_vol
        });
        self.run_transfer(items, is_move, cx);
    }

    /// ドロップ時の修飾キー → 強制効果 (Ctrl=コピー / Shift=移動 / なし=既定)。
    ///
    /// `window.modifiers()` は使わない — エクスプローラ等の外部からのドラッグ中は
    /// キーボードフォーカスが相手側にあり修飾キーが更新されず、Ctrl+ドロップの
    /// 強制コピーが効かない。物理キー状態 (GetKeyState) を直接読む。
    fn drop_effect_override() -> Option<bool> {
        let (ctrl, shift) = ole_dnd::drop_modifiers();
        if ctrl {
            Some(false) // コピー
        } else if shift {
            Some(true) // 移動
        } else {
            None
        }
    }

    /// 行で押下した左ボタンが 5px 以上動いたら OLE ドラッグ (DoDragDrop) を開始。
    /// 内部ペイン間も Explorer への外部送信も同じ 1 ドラッグで動く。
    ///
    /// 重要: DoDragDrop はブロッキングかつメッセージポンプを回すため、UI スレッド
    /// では呼べない (wndproc が gpui の App RefCell を再借用して panic する)。
    /// → **専用 STA ワーカースレッド**で実行し (OleInitialize はスレッド単位)、
    /// 結果は sink (既存チャネル) 経由で `ole-drag-done` として UI へ戻す。
    fn maybe_start_ole_drag(&mut self, ev: &MouseMoveEvent, _cx: &mut Context<Self>) {
        let Some(cand) = &self.drag_candidate else {
            return;
        };
        let want = if cand.right {
            MouseButton::Right
        } else {
            MouseButton::Left
        };
        if ev.pressed_button != Some(want) {
            self.drag_candidate = None;
            return;
        }
        let dx = (ev.position.x - cand.pos.x) / px(1.0);
        let dy = (ev.position.y - cand.pos.y) / px(1.0);
        if dx.abs() < 5.0 && dy.abs() < 5.0 {
            return;
        }
        let Some(cand) = self.drag_candidate.take() else {
            return;
        };
        let (paths, right) = (cand.paths, cand.right);
        if paths.is_empty() {
            return;
        }

        SELF_DROP.store(false, std::sync::atomic::Ordering::SeqCst);
        RIGHT_DRAG.store(right, std::sync::atomic::Ordering::SeqCst);
        let sink = self.sink.clone();
        // UI スレッド (= マウスを押下したスレッド) の id。ワーカーの入力状態を
        // これに接続しないと DoDragDrop が「ボタン非押下」と誤判定して即終了する。
        let ui_tid = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
        std::thread::spawn(move || {
            use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};

            // このスレッドを STA として OLE 初期化 (UI スレッドとは独立)。
            ole_dnd::init_ole();

            // 入力状態 (マウスボタン押下) を UI スレッドと共有する。
            let worker_tid = unsafe { GetCurrentThreadId() };
            let attached =
                unsafe { AttachThreadInput(worker_tid, ui_tid, true) }.as_bool();

            let req = DragRequest {
                paths: paths.iter().map(PathBuf::from).collect(),
                preferred: PreferredEffect::Move,
                button: if right {
                    DragButton::Right
                } else {
                    DragButton::Left
                },
            };
            // ブロッキング (ドロップ or ESC まで戻らない)。UI スレッドは自由なので
            // 自ウィンドウへのドロップ受信・再描画は通常どおり動く。
            let outcome = ole_dnd::start_drag(req);

            if attached {
                let _ = unsafe { AttachThreadInput(worker_tid, ui_tid, false) };
            }
            let payload = match outcome {
                Ok(DragOutcome::Move { delete_source }) => serde_json::json!({
                    "result": "move",
                    "delete_source": delete_source,
                    "paths": paths,
                }),
                Ok(DragOutcome::Copy) => serde_json::json!({
                    "result": "copy",
                    "paths": paths,
                }),
                Ok(DragOutcome::Cancel) | Ok(DragOutcome::None) => {
                    serde_json::json!({ "result": "none" })
                }
                Ok(DragOutcome::Error(e)) => {
                    serde_json::json!({ "result": "error", "error": e })
                }
                Err(e) => serde_json::json!({ "result": "error", "error": e.to_string() }),
            };
            sink.emit_json("ole-drag-done", payload);
            ole_dnd::shutdown_ole();
        });
    }

    /// 行の右ボタン押下: 選択を更新して D&D / メニューの候補を記録する。
    /// (メニューを出すかドラッグかは mouse_move / mouse_up 側で確定)
    fn on_row_right_down(
        &mut self,
        ix: usize,
        ev: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal.is_some() {
            return;
        }
        self.focus_handle.focus(window, cx);
        cx.emit(PaneEvent::Activated);
        if !self.selected.contains(&ix) {
            self.select_only(ix);
        } else {
            self.cursor = Some(ix);
        }
        cx.notify();
        let paths = self.selected_paths();
        self.drag_candidate = Some(DragCand {
            pos: ev.position,
            paths,
            right: true,
            row_ix: ix,
            shift: ev.modifiers.shift,
        });
    }

    /// 右ボタンを離した: ドラッグに発展していなければメニューを出す。
    fn on_right_up(&mut self, ev: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(cand) = self.drag_candidate.take() else {
            return;
        };
        if !cand.right {
            // 左ボタン候補はそのまま戻す (右 up とは無関係)。
            self.drag_candidate = Some(cand);
            return;
        }
        if cand.shift {
            self.shell_context_menu(cand.row_ix, window, cx);
        } else {
            self.on_row_right_click(cand.row_ix, ev.position, window, cx);
        }
    }

    /// 右ボタン D&D のドロップチューザーを開く (ここにコピー / 移動 / キャンセル)。
    fn open_drop_menu(
        &mut self,
        dst: PathBuf,
        paths: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let position = window.mouse_position();
        // チューザー専用のユーザーコマンド (when: "drop")。
        // paths = ドラッグした項目 / cwd = ドロップ先フォルダ として実行する。
        let user_cmds: Vec<(String, String)> = user_commands::list_user_commands()
            .map(|v| {
                v.into_iter()
                    .filter(|c| c.when == "drop")
                    .take(10)
                    .map(|c| (c.id, c.label))
                    .collect()
            })
            .unwrap_or_default();
        self.drop_menu = Some(DropMenu {
            position,
            dst,
            paths,
            user_cmds,
        });
        cx.notify();
    }

    /// チューザーの選択: Some(true)=移動 / Some(false)=コピー / None=キャンセル。
    fn drop_menu_action(&mut self, mv: Option<bool>, cx: &mut Context<Self>) {
        let Some(m) = self.drop_menu.take() else {
            return;
        };
        if let Some(is_move) = mv {
            let items = build_job_items(&m.paths, &m.dst);
            if !items.is_empty() {
                self.run_transfer(items, is_move, cx);
            }
        }
        cx.notify();
    }

    /// チューザーからユーザーコマンド実行 (paths=ドラッグ項目 / cwd=ドロップ先)。
    fn drop_menu_run_user_command(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(m) = self.drop_menu.take() else {
            return;
        };
        let ctx = RunCtx {
            paths: m.paths,
            cwd: m.dst.to_string_lossy().to_string(),
        };
        self.run_user_command_bg(id, ctx, cx);
        cx.notify();
    }

    /// ユーザーコマンドをバックグラウンドで実行する。
    /// プロセス起動 (CreateProcess / ShellExecuteW) は UI スレッドで呼ばない —
    /// 起動中に新プロセスへのフォーカス移動メッセージが再入し、App RefCell の
    /// 二重借用 ("RefCell already borrowed") で落ちる
    /// (doc/plan-2026-06-03 の DoDragDrop 再入対策と同じ理由)。
    fn run_user_command_bg(&self, id: String, ctx: RunCtx, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_executor()
                .spawn(async move { user_commands::run_user_command(id, ctx) })
                .await;
            if let Err(e) = res {
                let _ = this.update(cx, |pane, cx| {
                    pane.status = format!("コマンド実行に失敗: {e}").into();
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// 右ボタン D&D チューザーのオーバーレイ。
    fn render_drop_menu(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let m = self.drop_menu.as_ref()?;
        let item = |id: &'static str, label: &'static str, mv: Option<bool>| {
            div()
                .id(id)
                .flex()
                .px_3()
                .py_1()
                .mx_1()
                .rounded(theme::radius_sm())
                .cursor_pointer()
                .hover(|s| s.bg(th().menu_hover))
                .child(label)
                .on_click(cx.listener(move |this, _e, _w, cx| this.drop_menu_action(mv, cx)))
                .into_any_element()
        };
        let mut items: Vec<AnyElement> = vec![
            item("dm-copy", "ここにコピー", Some(false)),
            item("dm-move", "ここに移動", Some(true)),
        ];
        // ユーザーコマンド (when: "drop")。paths=ドラッグ項目 / cwd=ドロップ先。
        if !m.user_cmds.is_empty() {
            items.push(menu_sep());
            for (id, label) in &m.user_cmds {
                let cid = id.clone();
                items.push(
                    div()
                        .id(SharedString::from(format!("dm-uc-{id}")))
                        .flex()
                        .px_3()
                        .py_1()
                        .mx_1()
                        .rounded(theme::radius_sm())
                        .cursor_pointer()
                        .hover(|s| s.bg(th().menu_hover))
                        .child(label.clone())
                        .on_click(cx.listener(move |this, _e, _w, cx| {
                            this.drop_menu_run_user_command(cid.clone(), cx)
                        }))
                        .into_any_element(),
                );
            }
        }
        items.push(menu_sep());
        items.push(item("dm-cancel", "キャンセル", None));
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _e: &MouseDownEvent, _w, cx| {
                        this.drop_menu_action(None, cx);
                        cx.stop_propagation();
                    }),
                )
                .child(
                    deferred(
                        anchored()
                            .position(m.position)
                            .snap_to_window_with_margin(px(8.0))
                            .child(
                                div()
                                    .occlude()
                                    .flex()
                                    .flex_col()
                                    .py_1()
                                    .w(px(210.0))
                                    .rounded(theme::radius_md())
                                    .bg(th().surface_bg)
                                    .border_1()
                                    .border_color(th().button_hover)
                                    .text_color(th().text)
                                    .children(items),
                            ),
                    ),
                )
                .into_any_element(),
        )
    }

    /// OLE ドラッグ終了通知 (`ole-drag-done`) の後始末。
    /// 自アプリ内へのドロップは on_drop 側で処理済みなのでスキップ。
    fn on_ole_drag_done(&mut self, payload: &serde_json::Value, cx: &mut Context<Self>) {
        RIGHT_DRAG.store(false, std::sync::atomic::Ordering::SeqCst);
        if SELF_DROP.swap(false, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let result = payload.get("result").and_then(|v| v.as_str()).unwrap_or("");
        let paths: Vec<String> = payload
            .get("paths")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        match result {
            "move" => {
                let delete = payload
                    .get("delete_source")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if delete {
                    // ターゲット (Explorer 等) がコピーを終えた move → 元を削除する。
                    let mut err = None;
                    for p in &paths {
                        if let Err(e) = file_ops::delete_path(Path::new(p), true) {
                            err = Some(e.to_string());
                        }
                    }
                    self.status = match err {
                        None => format!("{} 個を移動しました", paths.len()).into(),
                        Some(e) => format!("移動の元削除に失敗: {e}").into(),
                    };
                } else {
                    // 最適化ムーブ等 (ターゲット側で処理済み)。
                    self.status = "移動しました".into();
                }
                self.reload(cx, false);
            }
            "copy" => {
                self.status = format!("{} 個をコピーしました", paths.len()).into();
                cx.notify();
            }
            "error" => {
                let e = payload
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("不明");
                self.status = format!("D&D 失敗: {e}").into();
                cx.notify();
            }
            _ => {}
        }
    }

    /// copy/move 転送の入口。宛先に同名が存在する場合は確認モーダルを出す。
    /// (全転送経路 — 貼り付け / 内部D&D / 外部D&D / フォルダ行 / 右ドラッグ — が通る)
    fn run_transfer(&mut self, items: Vec<JobItem>, is_move: bool, cx: &mut Context<Self>) {
        let conflicts: Vec<String> = items
            .iter()
            .filter(|it| Path::new(&it.to).exists())
            .filter_map(|it| {
                Path::new(&it.to)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
            })
            .collect();
        if conflicts.is_empty() {
            self.run_transfer_now(items, is_move, cx);
            return;
        }
        let conflict_count = conflicts.len();
        self.pending_transfer = Some(PendingTransfer {
            items,
            is_move,
            conflict_names: conflicts.into_iter().take(5).collect(),
            conflict_count,
        });
        cx.notify();
    }

    /// 衝突確認モーダルの選択。
    /// overwrite: Some(true)=上書き / Some(false)=別名で保存 / None=キャンセル。
    fn resolve_transfer(&mut self, overwrite: Option<bool>, cx: &mut Context<Self>) {
        let Some(mut pending) = self.pending_transfer.take() else {
            return;
        };
        match overwrite {
            Some(true) => {
                self.run_transfer_now(pending.items, pending.is_move, cx);
            }
            Some(false) => {
                // 衝突分だけ "name (2).ext" 式の一意名へ振り替える。
                for it in pending.items.iter_mut() {
                    let dst = Path::new(&it.to);
                    if dst.exists() {
                        it.to = unique_dest(dst).to_string_lossy().to_string();
                    }
                }
                self.run_transfer_now(pending.items, pending.is_move, cx);
            }
            None => {
                self.status = "転送をキャンセルしました".into();
                cx.notify();
            }
        }
    }

    /// 衝突確認モーダルのオーバーレイ。
    fn render_conflict_modal(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let p = self.pending_transfer.as_ref()?;
        let verb = if p.is_move { "移動" } else { "コピー" };
        let mut names = p.conflict_names.join("、");
        if p.conflict_count > p.conflict_names.len() {
            names.push_str(&format!(" ほか {} 件", p.conflict_count - p.conflict_names.len()));
        }
        let btn = |id: &'static str, label: &'static str, bg, ov: Option<bool>| {
            div()
                .id(id)
                .px_3()
                .py_1()
                .rounded(theme::radius_md())
                .cursor_pointer()
                .bg(bg)
                .hover(|s| s.bg(th().button_hover))
                .child(label)
                .on_click(cx.listener(move |this, _e, _w, cx| this.resolve_transfer(ov, cx)))
                .into_any_element()
        };
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .bg(th().overlay_bg)
                .child(
                    div()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_e, _w, cx| cx.stop_propagation())
                        .flex()
                        .flex_col()
                        .gap_3()
                        .w(px(460.0))
                        .p_3()
                        .rounded(theme::radius_md())
                        .bg(th().surface_bg)
                        .border_1()
                        .border_color(th().accent)
                        .text_color(th().text)
                        .child(format!(
                            "{verb}先に同じ名前の項目が {} 件あります",
                            p.conflict_count
                        ))
                        .child(
                            div()
                                .text_color(th().text_dim)
                                .overflow_hidden()
                                .child(names),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap_2()
                                .justify_end()
                                .child(btn("ct-ow", "上書き", th().danger_bg, Some(true)))
                                .child(btn(
                                    "ct-rn",
                                    "別名で保存",
                                    th().button_bg,
                                    Some(false),
                                ))
                                .child(btn("ct-cancel", "キャンセル", th().button_bg, None)),
                        )
                        .child(
                            div()
                                .text_color(th().text_faint)
                                .child("別名で保存: 衝突する項目だけ「名前 (2)」形式になります / Esc: キャンセル"),
                        ),
                ),
        )
        .map(|d| d.into_any_element())
    }

    /// copy/move ジョブを別スレッドで開始する (進捗は sink 経由で届く)。
    fn run_transfer_now(&mut self, items: Vec<JobItem>, is_move: bool, cx: &mut Context<Self>) {
        let job_id = self.next_job_id;
        self.next_job_id += 1;
        // 移動は Undo 候補を記録し、ジョブ完了 (ok) 時に履歴へ push する (ADR 0006)。
        // resolve_transfer 後なので to は衝突リネーム (「名前 (2)」) 済みの最終位置。
        if is_move {
            let mv: Vec<MoveItem> = items
                .iter()
                .map(|it| MoveItem {
                    from: PathBuf::from(&it.from),
                    to: PathBuf::from(&it.to),
                })
                .collect();
            self.pending_move_undo = Some((job_id, mv));
        }
        let registry = self.jobs.clone();
        let sink = self.sink.clone();
        std::thread::spawn(move || {
            let _ = if is_move {
                registry.run_move(sink.as_ref(), job_id, items)
            } else {
                registry.run_copy(sink.as_ref(), job_id, items)
            };
        });
        self.job_status = Some(
            if is_move {
                "移動を開始しました…"
            } else {
                "コピーを開始しました…"
            }
            .into(),
        );
        self.active_job = Some(job_id);
        cx.notify();
    }

    /// 実行中ジョブのキャンセル要求 (Esc / フッタのボタン)。
    /// 実際の停止は job スレッドがフラグを見て行い、`fs:job:done` (canceled) が届く。
    fn cancel_job(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.active_job {
            self.jobs.cancel(id);
            self.job_status = Some("キャンセル中…".into());
            cx.notify();
        }
    }

    /// 別スレッド由来の domain イベント受信 (UI スレッド上で実行)。
    fn on_domain_event(&mut self, event: &str, payload: serde_json::Value, cx: &mut Context<Self>) {
        match event {
            "ole-drag-done" => self.on_ole_drag_done(&payload, cx),
            "fs-change" => {
                // notify はバーストするため 150ms デバウンスでまとめて reload。
                if !self.reload_pending {
                    self.reload_pending = true;
                    cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(150))
                            .await;
                        let _ = this.update(cx, |pane, cx| {
                            pane.reload_pending = false;
                            pane.reload(cx, false);
                        });
                    })
                    .detach();
                }
            }
            "fs:job:progress" => {
                let done = payload.get("done_files").and_then(|v| v.as_u64()).unwrap_or(0);
                let total = payload
                    .get("total_files")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let kind = payload.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                let current = payload.get("current").and_then(|v| v.as_str()).unwrap_or("");
                let kind_jp = match kind {
                    "copy" => "コピー中",
                    "move" => "移動中",
                    "delete" => "削除中",
                    _ => "処理中",
                };
                let cur_name = Path::new(current)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.job_status = Some(format!("{kind_jp} {done}/{total}  {cur_name}").into());
                cx.notify();
            }
            "search-hit" => {
                if let Some(ui) = self.search_ui.as_mut() {
                    let jid = payload.get("job_id").and_then(|v| v.as_u64());
                    if jid == ui.job_id {
                        ui.results.push(SearchResult {
                            path: payload
                                .get("path")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            name: payload
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            is_dir: payload
                                .get("is_dir")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false),
                        });
                        cx.notify();
                    }
                }
            }
            "search-done" => {
                if let Some(ui) = self.search_ui.as_mut() {
                    let jid = payload.get("job_id").and_then(|v| v.as_u64());
                    if jid == ui.job_id {
                        ui.running = false;
                        let total = payload.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
                        let backend =
                            payload.get("backend").and_then(|v| v.as_str()).unwrap_or("");
                        let fallback = payload
                            .get("fallback")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let canceled = payload
                            .get("canceled")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let err = payload.get("error").and_then(|v| v.as_str());
                        ui.info = if let Some(e) = err {
                            format!("エラー: {e}").into()
                        } else if canceled {
                            "キャンセル".into()
                        } else {
                            let tag = if backend == "everything" {
                                " (Everything)"
                            } else if fallback {
                                " (内蔵検索)"
                            } else {
                                ""
                            };
                            format!("{total} 件{tag}").into()
                        };
                        cx.notify();
                    }
                }
            }
            "fs:job:done" => {
                let ok = payload.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                let canceled = payload
                    .get("canceled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                // 移動ジョブの Undo 記録。ジョブはアイテム別成否を返さないため
                // 全件成功 (ok) のときだけ履歴へ push する (失敗/キャンセルは記録しない)。
                let done_id = payload.get("job_id").and_then(|v| v.as_u64());
                if let Some((pending_id, items)) = self.pending_move_undo.take() {
                    if done_id == Some(pending_id) {
                        if ok && !canceled {
                            undo_store().lock().unwrap().push(UndoOp::Move { items });
                        }
                    } else {
                        // 別ジョブの完了通知 — 候補は保持したままにする。
                        self.pending_move_undo = Some((pending_id, items));
                    }
                }
                self.job_status = None;
                self.active_job = None;
                self.status = if canceled {
                    "キャンセルしました".into()
                } else if ok {
                    "完了".into()
                } else {
                    let err = payload
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("不明なエラー");
                    format!("失敗: {err}").into()
                };
                self.reload(cx, false);
            }
            _ => {}
        }
    }

    fn render_row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let entry = &self.entries[ix];
        // 列幅 (更新日時 / サイズ / 種類) — ペイン個別、見出しの仕切りドラッグで可変。
        let col_w = (
            self.col_widths[0],
            self.col_widths[1],
            self.col_widths[2],
        );
        let is_dir = entry.kind == "dir";
        let is_selected = self.selected.contains(&ix);
        let is_cursor = self.cursor == Some(ix);

        let name = entry.name.clone();
        let size_text = if is_dir {
            String::new()
        } else {
            human_size(entry.size)
        };
        let kind_text = if is_dir {
            "フォルダ".to_string()
        } else {
            entry
                .ext
                .clone()
                .map(|e| e.to_uppercase())
                .unwrap_or_else(|| "ファイル".to_string())
        };

        // ドラッグ元ペイロード: 選択内の行なら選択全体、選択外なら単一。
        let drag_paths: Vec<String> = if is_selected {
            self.selected_paths()
        } else {
            vec![
                self.cur_path
                    .join(&entry.name)
                    .to_string_lossy()
                    .to_string(),
            ]
        };

        // 選択=青地 / カーソル=明るめ (選択+カーソルが最も明るい)。
        let row_bg = if is_selected && is_cursor {
            th().sel_cursor_bg
        } else if is_selected {
            th().sel_bg
        } else if is_cursor {
            th().cursor_bg
        } else if ix % 2 == 0 {
            th().row_even
        } else {
            th().row_odd
        };

        // アイコン: 取得できれば実アイコン、失敗時は種別アクセントの代替。
        let icon_el: AnyElement = match self.row_icons.get(ix).cloned().flatten() {
            Some(handle) => img(handle).w(px(16.0)).h(px(16.0)).into_any_element(),
            None => {
                let accent = if is_dir { th().accent } else { th().accent_file };
                div()
                    .w(px(6.0))
                    .h(px(14.0))
                    .rounded(theme::radius_sm())
                    .bg(accent)
                    .into_any_element()
            }
        };

        let row = div()
            .id(ix)
            .flex()
            .flex_row()
            .items_center()
            // 行は常に全幅 (内容幅で縮むと サイズ/種類 列が行ごとにずれる)。
            .w_full()
            .h(theme::row_h())
            .px_2()
            .gap_2()
            .bg(row_bg)
            .cursor_pointer()
            .hover(|s| s.bg(th().hover_bg))
            .on_click(cx.listener(move |this, event, window, cx| {
                this.on_row_click(ix, event, window, cx);
            }))
            // 右ボタン: 押下で候補記録 (5px 動けば右ボタン D&D / 動かず離せば
            // 通常メニュー or Shift でシェルメニュー)。判定は mouse_up / move 側。
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                    this.on_row_right_down(ix, ev, window, cx);
                    cx.stop_propagation();
                }),
            )
            // ドラッグ元 (OLE 統一): 左ボタン押下で候補を記録し、5px 動いたら
            // DoDragDrop 開始 (maybe_start_ole_drag)。内部ペイン間も Explorer への
            // 外部送信も同じ 1 ドラッグで動く。
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, _w, _cx| {
                    this.drag_candidate = Some(DragCand {
                        pos: ev.position,
                        paths: drag_paths.clone(),
                        right: false,
                        row_ix: ix,
                        shift: false,
                    });
                }),
            )
            .child(
                div()
                    .w(px(16.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon_el),
            )
            // 名前: 長くても折返さず … 省略 (サイズ/種類列を押し出さない)。
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .truncate()
                    .text_color(th().text)
                    .child(name),
            )
            .child(
                div()
                    .w(px(col_w.0))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(th().text_dim)
                    .child(format_modified(entry.modified)),
            )
            .child(
                div()
                    .w(px(col_w.1))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_right()
                    .text_color(th().text_dim)
                    .child(size_text),
            )
            .child(
                div()
                    .w(px(col_w.2))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .truncate()
                    .text_color(th().text_dim)
                    .child(kind_text),
            );

        // フォルダ行は直接ドロップ先になる (その行のフォルダへ転送)。
        if is_dir {
            let dirname = entry.name.clone();
            row.drag_over::<ExternalPaths>(|s, _, _, _| s.bg(th().drop_row_bg))
                .on_drop(cx.listener(move |this, files: &ExternalPaths, window, cx| {
                    SELF_DROP.store(true, std::sync::atomic::Ordering::SeqCst);
                    let dst = this.cur_path.join(&dirname);
                    let paths: Vec<String> = files
                        .paths()
                        .iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect();
                    if RIGHT_DRAG.load(std::sync::atomic::Ordering::SeqCst) {
                        this.open_drop_menu(dst, paths, window, cx);
                    } else {
                        let forced = Self::drop_effect_override();
                        this.drop_paths_into(dst, paths, forced, cx);
                    }
                    // ペイン全体のドロップ (表示中フォルダへ) と二重処理しない。
                    cx.stop_propagation();
                }))
                .into_any_element()
        } else {
            row.into_any_element()
        }
    }

    /// 列見出しセル (クリックでその列ソート / 再クリックで昇降反転)。
    fn header_cell(&self, label: &str, col: SortCol, cx: &mut Context<Self>) -> AnyElement {
        let text = if self.sort_col == col {
            format!("{} {}", label, if self.sort_asc { "▲" } else { "▼" })
        } else {
            label.to_string()
        };
        div()
            .id(SharedString::from(format!("hdr-{label}")))
            .cursor_pointer()
            .hover(|s| s.text_color(th().text_soft))
            .on_click(cx.listener(move |this, _e, _w, cx| this.set_sort(col, cx)))
            .child(text)
            .into_any_element()
    }
}

impl Drop for PaneView {
    fn drop(&mut self) {
        // PaneView が落ちると watcher(Arc<WatcherCore>) と sink も連鎖して落ち、
        // チャネルが閉じて cx.spawn の受信ループも終了する → リーク無し。
        PANES_ALIVE.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Render for PaneView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 初回描画時にキーボードフォーカスをペインへ。
        if !self.focused_once {
            self.focus_handle.focus(window, cx);
            self.focused_once = true;
        }

        let path_text = self.cur_path.display().to_string();
        // アドレスバー: 通常はクリック可能なパス表示 / 編集中は入力欄。
        let path_area: AnyElement = if let Some(input) = &self.path_edit {
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .child(input.clone())
                .into_any_element()
        } else {
            div()
                .id("path")
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                // 長いパスは折返さず … 省略 (右側のボタンへはみ出さない)。
                // クリックで入力欄になり、マウスドラッグで部分選択できる。
                .truncate()
                .px_1()
                .rounded(theme::radius_sm())
                .cursor_pointer()
                .hover(|s| s.bg(th().surface_hover))
                .child(path_text)
                .on_click(cx.listener(|this, _e, w, cx| this.start_path_edit(w, cx)))
                .into_any_element()
        };
        // 実行中ジョブの進捗があれば status より優先表示。
        let status = self
            .job_status
            .clone()
            .unwrap_or_else(|| self.status.clone());
        let sel_text = if self.selected.len() > 1 {
            format!("{} 個選択", self.selected.len())
        } else {
            String::new()
        };
        // 実行中ジョブがあればフッタにキャンセルボタンを出す。
        let cancel_btn = self.active_job.map(|_| {
            div()
                .id("job-cancel")
                .px_2()
                .rounded(theme::radius_sm())
                .cursor_pointer()
                .bg(th().danger_bg)
                .hover(|s| s.bg(th().danger_hover).text_color(th().text_bright))
                .text_color(th().text)
                .child("キャンセル (Esc)")
                .on_click(cx.listener(|this, _e, _w, cx| this.cancel_job(cx)))
        });
        let count = self.entries.len();

        // 検索を「実行」するまではファイル一覧を表示したまま (バーだけ出す)。
        let searching = self.search_ui.as_ref().is_some_and(|u| u.executed);

        // 検索バー (Ctrl+F で表示)。
        let search_bar = self.search_ui.as_ref().map(|ui| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_2()
                .h(theme::bar_h())
                .bg(th().search_bar_bg)
                .child(div().text_color(th().text_dim).child("検索"))
                .child(div().flex_1().child(ui.input.clone()))
                .child(div().text_color(th().text_faint).child(ui.info.clone()))
                .child(
                    div()
                        .id("search-close")
                        .px_2()
                        .rounded(theme::radius_sm())
                        .cursor_pointer()
                        .hover(|s| s.bg(th().button_bg))
                        .child("×")
                        .on_click(cx.listener(|this, _e, w, cx| this.close_search(w, cx))),
                )
        });

        // 列見出し (検索中は結果リストと列が合わないため非表示)。
        // 固定幅列 (更新日時/サイズ/種類) の左端に仕切りハンドル — ドラッグで列幅変更。
        let header = (!searching).then(|| {
            let [col_mod, col_size, col_type] = self.col_widths;
            let name_hdr = self.header_cell("名前", SortCol::Name, cx);
            let mod_hdr = self.header_cell("更新日時", SortCol::Modified, cx);
            let size_hdr = self.header_cell("サイズ", SortCol::Size, cx);
            let type_hdr = self.header_cell("種類", SortCol::Type, cx);
            let pane_id = cx.entity_id();
            let col_handle = move |id: &'static str, col: SortCol| {
                div()
                    .id(id)
                    .absolute()
                    .left(px(-7.0))
                    .top_0()
                    .h_full()
                    .w(px(6.0))
                    .cursor_col_resize()
                    .hover(|st| st.bg(th().handle_hover))
                    .on_drag(ColHandle { col, pane: pane_id }, |_, _, _, cx| {
                        cx.new(|_| Empty)
                    })
            };
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_2()
                .h(theme::header_h())
                .bg(th().header_bg)
                .text_color(th().text_faint)
                .child(div().w(px(16.0)).flex_shrink_0())
                .child(div().flex_1().min_w_0().overflow_hidden().child(name_hdr))
                .child(
                    div()
                        .w(px(col_mod))
                        .flex_shrink_0()
                        .relative()
                        .child(
                            div()
                                .w_full()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(mod_hdr),
                        )
                        .child(col_handle("ch-mod", SortCol::Modified)),
                )
                .child(
                    div()
                        .w(px(col_size))
                        .flex_shrink_0()
                        .relative()
                        .child(
                            div()
                                .w_full()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .flex()
                                .justify_end()
                                .child(size_hdr),
                        )
                        .child(col_handle("ch-size", SortCol::Size)),
                )
                .child(
                    div()
                        .w(px(col_type))
                        .flex_shrink_0()
                        .relative()
                        .child(
                            div()
                                .w_full()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(type_hdr),
                        )
                        .child(col_handle("ch-type", SortCol::Type)),
                )
        });

        // 一覧領域: 通常はファイル一覧 / 検索中は検索結果 (どちらも仮想化)。
        let list_area: AnyElement = if searching {
            let rcount = self.search_ui.as_ref().map(|u| u.results.len()).unwrap_or(0);
            uniform_list(
                "search-results",
                rcount,
                cx.processor(|this, range: Range<usize>, _w, cx| {
                    range
                        .map(|ix| this.render_search_row(ix, cx))
                        .collect::<Vec<_>>()
                }),
            )
            .size_full()
            .into_any_element()
        } else {
            uniform_list(
                "file-list",
                count,
                cx.processor(|this, range: Range<usize>, _w, cx| {
                    range.map(|ix| this.render_row(ix, cx)).collect::<Vec<_>>()
                }),
            )
            .track_scroll(&self.scroll)
            .size_full()
            .into_any_element()
        };

        // 一覧コンテナ。検索結果でないときは空白部分の左ドラッグでラバーバンド
        // (矩形) 選択を受け付け、ドラッグ中の矩形を重ねて描く。
        let mut list_container = div().relative().flex_1().overflow_hidden().child(list_area);
        if !searching {
            list_container = list_container
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, ev: &MouseDownEvent, _w, cx| {
                        this.on_list_mouse_down(ev, cx);
                    }),
                )
                .children(self.render_rubber());
        }

        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                this.on_key(&ev.keystroke, window, cx);
            }))
            // ペイン内のどこをクリックしてもこのペインをアクティブにする。
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _ev, _w, cx| cx.emit(PaneEvent::Activated)),
            )
            // 背景の右クリック: 貼り付け/新規作成メニュー (行上は行側で処理済み)。
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                    this.on_bg_right_click(ev.position, window, cx);
                }),
            )
            // マウスの戻る/進むボタン (第4/第5)。
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Back),
                cx.listener(|this, _ev, _w, cx| this.go_back(cx)),
            )
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Forward),
                cx.listener(|this, _ev, _w, cx| this.go_forward(cx)),
            )
            // ドロップ受け入れ (ペイン間 / エクスプローラ等、全て OLE = ExternalPaths)。
            // ドロップ先はこのペインの表示中フォルダ (ADR 0009)。
            .drag_over::<ExternalPaths>(|style, _, _, _| style.bg(th().drop_bg))
            .on_drop(cx.listener(|this, files: &ExternalPaths, window, cx| {
                SELF_DROP.store(true, std::sync::atomic::Ordering::SeqCst);
                let paths: Vec<String> = files
                    .paths()
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();
                let dst = this.cur_path.clone();
                if RIGHT_DRAG.load(std::sync::atomic::Ordering::SeqCst) {
                    // 右ボタン D&D: 効果をユーザーに選ばせる (ADR 0010)。
                    this.open_drop_menu(dst, paths, window, cx);
                } else {
                    let forced = Self::drop_effect_override();
                    this.drop_paths_into(dst, paths, forced, cx);
                }
            }))
            // OLE ドラッグの開始判定 / ラバーバンド追従。
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _w, cx| {
                if this.rubber.is_some() {
                    this.update_rubber(ev.position, cx);
                } else {
                    this.maybe_start_ole_drag(ev, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _e, _w, cx| {
                    this.drag_candidate = None;
                    if this.rubber.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            // 右ボタン: 動かず離した場合はメニュー表示 (候補が残っていれば)。
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(|this, ev: &MouseUpEvent, window, cx| {
                    this.on_right_up(ev, window, cx);
                }),
            )
            // 列見出しの仕切りドラッグ (ペイン全域で追跡)。
            .on_drag_move(cx.listener(|this, e: &DragMoveEvent<ColHandle>, _w, cx| {
                this.on_col_handle_drag(e, cx);
            }))
            .flex()
            .flex_col()
            .size_full()
            .relative()
            .bg(th().pane_bg)
            .text_color(th().text)
            // 上部パスバー
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .h(theme::bar_h())
                    .bg(th().bar_bg)
                    // 戻る/進むはボタン非表示 (マウス第4/5ボタン・Alt+←→ で操作)。
                    .child(
                        div()
                            .id("up")
                            .px_2()
                            .py_1()
                            .rounded(theme::radius_md())
                            .bg(th().button_bg)
                            .cursor_pointer()
                            .hover(|s| s.bg(th().button_hover))
                            .child("↑")
                            .on_click(cx.listener(|this, _e, _w, cx| this.go_up(cx))),
                    )
                    .child(path_area)
                    // ペイン分割 / 閉じる
                    .child(
                        div()
                            .id("split-row")
                            .px_2()
                            .py_1()
                            .rounded(theme::radius_md())
                            .bg(th().button_bg)
                            .cursor_pointer()
                            .hover(|s| s.bg(th().button_hover))
                            .child("↔")
                            .on_click(cx.listener(|_t, _e, _w, cx| {
                                cx.emit(PaneEvent::SplitRequested(SplitDir::Row))
                            })),
                    )
                    .child(
                        div()
                            .id("split-col")
                            .px_2()
                            .py_1()
                            .rounded(theme::radius_md())
                            .bg(th().button_bg)
                            .cursor_pointer()
                            .hover(|s| s.bg(th().button_hover))
                            .child("↕")
                            .on_click(cx.listener(|_t, _e, _w, cx| {
                                cx.emit(PaneEvent::SplitRequested(SplitDir::Column))
                            })),
                    )
                    .child(
                        div()
                            .id("close-pane")
                            .px_2()
                            .py_1()
                            .rounded(theme::radius_md())
                            .bg(th().button_bg)
                            .cursor_pointer()
                            .hover(|s| s.bg(th().danger_hover))
                            .child("×")
                            .on_click(cx.listener(|_t, _e, _w, cx| {
                                cx.emit(PaneEvent::CloseRequested)
                            })),
                    ),
            )
            // 検索バー (Ctrl+F・表示中のみ)
            .children(search_bar)
            // 列見出し (検索中は非表示)
            .children(header)
            // 一覧 (仮想化: 通常 or 検索結果。空白部分のラバーバンド選択を内包)
            .child(list_container)
            // フッタ (ステータス + 選択数)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .h(theme::row_h())
                    .bg(th().bar_bg)
                    .text_color(th().text_dim)
                    .child(div().flex_1().overflow_hidden().child(status))
                    .children(cancel_btn)
                    .child(sel_text),
            )
            // 入力モーダル (開いている時のみ)
            .children(self.render_modal(cx))
            // 右クリックメニュー (開いている時のみ)
            .children(self.render_context_menu(window, cx))
            // 右ボタン D&D チューザー (開いている時のみ)
            .children(self.render_drop_menu(cx))
            // 同名衝突の確認モーダル (確認待ちのみ)
            .children(self.render_conflict_modal(cx))
    }
}

/// entries と同じ index で各行のアイコンを用意する。
/// フォルダ / 拡張子単位で結果を共有し、domain 呼び出し回数を最小化する。
fn load_row_icons(entries: &[FileEntry], dir: &Path) -> Vec<Option<Arc<Image>>> {
    let mut cache: HashMap<String, Option<Arc<Image>>> = HashMap::new();
    entries
        .iter()
        .map(|e| {
            let key = if e.kind == "dir" {
                "d".to_string()
            } else {
                format!("f:{}", e.ext.clone().unwrap_or_default())
            };
            cache
                .entry(key)
                .or_insert_with(|| load_icon(e, dir))
                .clone()
        })
        .collect()
}

/// domain からアイコン PNG を取り、GPUI の `Image` (PNG, 遅延デコード) に変換。
fn load_icon(entry: &FileEntry, dir: &Path) -> Option<Arc<Image>> {
    let png = if entry.kind == "dir" {
        icons::folder_icon_png(false)
    } else {
        // ext_only=true: 拡張子からの代表アイコン (実ファイルアクセス不要・共有可)。
        let p = dir.join(&entry.name);
        icons::system_icon_png(&p.to_string_lossy(), false, true)
    }
    .ok()?;
    Some(Arc::new(Image::from_bytes(ImageFormat::Png, png.as_ref().clone())))
}

/// パス一覧 → コピー/移動ジョブの from/to。安全のため以下をスキップ:
/// - 同一パスへの転送 (from == to)
/// - 自分自身 (またはその子孫) への転送 (無限再帰防止)
fn build_job_items(paths: &[String], dst_dir: &Path) -> Vec<JobItem> {
    paths
        .iter()
        .map(|p| {
            let name = Path::new(p)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| p.clone());
            JobItem {
                from: p.clone(),
                to: dst_dir.join(name).to_string_lossy().to_string(),
            }
        })
        .filter(|it| it.from != it.to)
        .filter(|it| !dst_dir.starts_with(Path::new(&it.from)))
        .collect()
}

/// GPUI ウィンドウから Win32 HWND を取得 (シェルメニュー等の Win32 連携用)。
/// gpui の継承メソッド `Window::window_handle` (AnyWindowHandle) と衝突するため
/// raw-window-handle のトレイトメソッドを明示的に呼ぶ。
fn hwnd_of(window: &Window) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = HasWindowHandle::window_handle(window).ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
        _ => None,
    }
}

/// 既存と衝突しない宛先パスを生成する ("name (2).ext" 式の連番)。
fn unique_dest(p: &Path) -> PathBuf {
    if !p.exists() {
        return p.to_path_buf();
    }
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = p.extension().map(|s| s.to_string_lossy().to_string());
    let parent = p.parent().unwrap_or_else(|| Path::new(""));
    for i in 2..10_000 {
        let name = match &ext {
            Some(e) => format!("{stem} ({i}).{e}"),
            None => format!("{stem} ({i})"),
        };
        let cand = parent.join(name);
        if !cand.exists() {
            return cand;
        }
    }
    p.to_path_buf()
}

/// メニューの区切り線。
fn menu_sep() -> AnyElement {
    div()
        .h(px(1.0))
        .mx_1()
        .my_1()
        .bg(th().separator)
        .into_any_element()
}

/// unix 秒 → ローカル時刻 "YYYY/MM/DD HH:MM"。0 (取得失敗) は空文字。
fn format_modified(unix: i64) -> String {
    if unix <= 0 {
        return String::new();
    }
    use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows::Win32::System::Time::SystemTimeToTzSpecificLocalTime;
    use windows::Win32::System::Time::FileTimeToSystemTime;
    // unix 秒 → FILETIME (1601-01-01 起点の 100ns 単位)。
    let ticks = (unix as u64)
        .wrapping_mul(10_000_000)
        .wrapping_add(116_444_736_000_000_000);
    let ft = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    unsafe {
        if FileTimeToSystemTime(&ft, &mut utc).is_err() {
            return String::new();
        }
        if SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).is_err() {
            return String::new();
        }
    }
    format!(
        "{:04}/{:02}/{:02} {:02}:{:02}",
        local.wYear, local.wMonth, local.wDay, local.wHour, local.wMinute
    )
}

/// バイト数を人間可読に整形 (B/KB/MB/GB/TB)。
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut b = bytes as f64;
    let mut i = 0;
    while b >= 1024.0 && i < UNITS.len() - 1 {
        b /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", bytes, UNITS[i])
    } else {
        format!("{:.1} {}", b, UNITS[i])
    }
}
