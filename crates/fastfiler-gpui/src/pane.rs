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
use std::sync::Arc;

use fastfiler_domain::events::EventSink;
use fastfiler_domain::file_jobs::{JobItem, JobRegistry};
use fastfiler_domain::fs::{self, FileEntry};
use fastfiler_domain::watcher::WatcherCore;
use fastfiler_domain::{file_ops, icons, path_util, shell, win_clipboard};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, ExternalPaths, FocusHandle, Image,
    ImageFormat, IntoElement, KeyDownEvent, Keystroke, MouseButton, MouseDownEvent, Pixels, Point,
    ScrollStrategy, SharedString, UniformListScrollHandle, Window, anchored, deferred, div, img,
    prelude::*, px, rgb, rgba, uniform_list,
};

use crate::sink::ChannelSink;
use crate::text_input::TextInput;

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
#[derive(Clone, Copy)]
enum MenuAction {
    Open,
    Copy,
    Cut,
    Paste,
    Rename,
    Delete,
    NewFolder,
    NewFile,
    Refresh,
}

/// ペイン間 D&D のペイロード (ドラッグ中のファイルパス一覧)。
pub(crate) struct DraggedFiles {
    paths: Vec<String>,
}

/// ドラッグ中にカーソル横へ出すプレビュー。
struct DragPreview {
    text: SharedString,
}

impl Render for DragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(rgb(0x2d4661))
            .border_1()
            .border_color(rgb(0x5aa9e6))
            .text_color(rgb(0xffffff))
            .text_size(px(12.0))
            .child(self.text.clone())
    }
}

/// 右クリックメニューの状態。
struct MenuState {
    /// ウィンドウ座標 (anchored で配置)。
    position: Point<Pixels>,
    /// 行上で開いたか (false = 背景)。
    on_row: bool,
    /// 開いた時点でクリップボードに貼り付け可能なファイルがあるか。
    can_paste: bool,
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

    /// app 側 (ペイン切替) からも focus できるよう crate 公開。
    pub(crate) focus_handle: FocusHandle,
    focused_once: bool,
    /// watcher バースト対策の reload デバウンスフラグ。
    reload_pending: bool,

    /// リネーム / 新規作成の入力モーダル。
    modal: Option<ModalState>,
    /// 右クリックメニュー。
    context_menu: Option<MenuState>,

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
            focus_handle: cx.focus_handle(),
            focused_once: false,
            reload_pending: false,
            modal: None,
            context_menu: None,
            watcher: Arc::new(WatcherCore::default()),
            sink,
            watched: None,
            jobs: Arc::new(JobRegistry::default()),
            next_job_id: 1,
            job_status: None,
        };
        this.open(path, cx, true);
        this
    }

    /// 表示フォルダを切り替える: 旧 watch を外し、新 watch を張り、再読込。
    fn open(&mut self, path: PathBuf, cx: &mut Context<Self>, reset_view: bool) {
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
                self.status = format!("{}  —  {} 項目", self.cur_path.display(), n).into();
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
                SortCol::Size => a.size.cmp(&b.size),
                SortCol::Type => a
                    .ext
                    .cmp(&b.ext)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            };
            if asc { ord } else { ord.reverse() }
        });
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
            self.open(parent, cx, true);
        }
    }

    /// 外部 (ワークスペースツリー等) からフォルダを開く。
    pub fn open_dir(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.open(path, cx, true);
    }

    /// タブ見出し用の名前 (表示中フォルダ名。ルートは表示パスそのまま)。
    pub fn title(&self) -> String {
        self.cur_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| self.cur_path.display().to_string())
    }

    pub fn cur_path(&self) -> &Path {
        &self.cur_path
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
            self.open(path, cx, true);
        } else {
            self.open_in_shell(path, cx);
        }
    }

    fn open_in_shell(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Err(e) = shell::open_with_shell(path.to_string_lossy().to_string()) {
            self.status = format!("開けません: {e}").into();
            cx.notify();
        }
    }

    /// 選択中 (複数可) の項目をごみ箱へ。watcher でも更新されるが即時反映のため明示 reload。
    fn delete_selected(&mut self, cx: &mut Context<Self>) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let n = paths.len();
        match file_ops::delete_to_trash(paths) {
            Ok(()) => {
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

        // Ctrl 系: コピー / 切り取り / 貼り付け
        if ks.modifiers.control {
            match ks.key.as_str() {
                "c" => {
                    self.clipboard_copy("copy", cx);
                    return;
                }
                "x" => {
                    self.clipboard_copy("cut", cx);
                    return;
                }
                "v" => {
                    self.paste(cx);
                    return;
                }
                "a" => {
                    self.select_all(cx);
                    return;
                }
                "tab" => {
                    // Ctrl+Tab = 次のタブ / Ctrl+Shift+Tab = 前のタブ
                    cx.emit(PaneEvent::SwitchTab(if ks.modifiers.shift { -1 } else { 1 }));
                    return;
                }
                _ => {}
            }
        }

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
            "enter" => self.activate_selected(cx),
            "backspace" => self.go_up(cx),
            "delete" => self.delete_selected(cx),
            "f5" => self.reload(cx, false),
            "f6" => cx.emit(PaneEvent::FocusNextPane),
            "f2" => self.start_rename(window, cx),
            "f7" => self.open_modal(ModalKind::NewFolder, String::new(), 0..0, window, cx),
            "f8" => self.open_modal(ModalKind::NewFile, String::new(), 0..0, window, cx),
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
                    file_ops::rename_path_no_overwrite(
                        &self.cur_path.join(original),
                        &self.cur_path.join(&text),
                    )
                    .map_err(|e| e.to_string())
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

    // ── 右クリックメニュー ─────────────────────────────────────────

    fn open_menu(&mut self, position: Point<Pixels>, on_row: bool, cx: &mut Context<Self>) {
        let can_paste = win_clipboard::clipboard_read_paths()
            .ok()
            .flatten()
            .map(|c| !c.paths.is_empty())
            .unwrap_or(false);
        self.context_menu = Some(MenuState {
            position,
            on_row,
            can_paste,
        });
        cx.notify();
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
            MenuAction::NewFile => {
                self.open_modal(ModalKind::NewFile, String::new(), 0..0, window, cx)
            }
            MenuAction::Refresh => self.reload(cx, false),
        }
        cx.notify();
    }

    fn menu_item(
        &self,
        label: &'static str,
        shortcut: &'static str,
        enabled: bool,
        action: MenuAction,
        cx: &Context<Self>,
    ) -> AnyElement {
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
            .rounded_sm();
        if enabled {
            row.cursor_pointer()
                .hover(|s| s.bg(rgb(0x3a5a7a)))
                .on_click(cx.listener(move |this, _e, w, cx| this.menu_action(action, w, cx)))
                .child(div().child(label))
                .child(div().text_color(rgb(0x8a8a8a)).child(shortcut))
                .into_any_element()
        } else {
            row.text_color(rgb(0x666666))
                .child(div().child(label))
                .child(div().text_color(rgb(0x555555)).child(shortcut))
                .into_any_element()
        }
    }

    /// 右クリックメニューのオーバーレイ (開いていなければ None)。
    fn render_context_menu(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let m = self.context_menu.as_ref()?;
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
        items.push(self.menu_item("新しいファイル", "F8", true, MenuAction::NewFile, cx));

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
                                    .rounded_md()
                                    .bg(rgb(0x2a2a2a))
                                    .border_1()
                                    .border_color(rgb(0x4a4a4a))
                                    .text_color(rgb(0xdddddd))
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
                .bg(rgba(0x000000aa))
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
                        .rounded_md()
                        .bg(rgb(0x2a2a2a))
                        .border_1()
                        .border_color(rgb(0x5aa9e6))
                        .text_color(rgb(0xe0e0e0))
                        .child(title)
                        .child(m.input.clone())
                        .child(
                            div()
                                .text_color(rgb(0x8a8a8a))
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
                    self.open(path, cx, true);
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

    /// ペイン間 / 外部からのドロップ。同一ボリューム=移動 / 異なる=コピー (エクスプローラ準拠)。
    fn drop_paths(&mut self, paths: Vec<String>, cx: &mut Context<Self>) {
        let items = build_job_items(&paths, &self.cur_path);
        if items.is_empty() {
            return;
        }
        let src_vol = path_util::volume_key(Path::new(&items[0].from));
        let dst_vol = path_util::volume_key(&self.cur_path);
        let is_move = src_vol.is_some() && src_vol == dst_vol;
        self.run_transfer(items, is_move, cx);
    }

    /// copy/move ジョブを別スレッドで開始する (進捗は sink 経由で届く)。
    fn run_transfer(&mut self, items: Vec<JobItem>, is_move: bool, cx: &mut Context<Self>) {
        let job_id = self.next_job_id;
        self.next_job_id += 1;
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
        cx.notify();
    }

    /// 別スレッド由来の domain イベント受信 (UI スレッド上で実行)。
    fn on_domain_event(&mut self, event: &str, payload: serde_json::Value, cx: &mut Context<Self>) {
        match event {
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
            "fs:job:done" => {
                let ok = payload.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                let canceled = payload
                    .get("canceled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.job_status = None;
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
            rgb(0x33506e)
        } else if is_selected {
            rgb(0x2d4661)
        } else if is_cursor {
            rgb(0x2a3340)
        } else if ix % 2 == 0 {
            rgb(0x1e1e1e)
        } else {
            rgb(0x232323)
        };

        // アイコン: 取得できれば実アイコン、失敗時は種別アクセントの代替。
        let icon_el: AnyElement = match self.row_icons.get(ix).cloned().flatten() {
            Some(handle) => img(handle).w(px(16.0)).h(px(16.0)).into_any_element(),
            None => {
                let accent = if is_dir { rgb(0x5aa9e6) } else { rgb(0x707070) };
                div()
                    .w(px(6.0))
                    .h(px(14.0))
                    .rounded_sm()
                    .bg(accent)
                    .into_any_element()
            }
        };

        div()
            .id(ix)
            .flex()
            .flex_row()
            .items_center()
            .h(px(24.0))
            .px_2()
            .gap_2()
            .bg(row_bg)
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0x33404d)))
            .on_click(cx.listener(move |this, event, window, cx| {
                this.on_row_click(ix, event, window, cx);
            }))
            // 右クリック: この行を対象にメニューを開く (背景メニューへは伝播させない)。
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                    this.on_row_right_click(ix, ev.position, window, cx);
                    cx.stop_propagation();
                }),
            )
            // ドラッグ元: 選択内の行なら選択全体、選択外なら単一をドラッグ。
            .on_drag(
                DraggedFiles { paths: drag_paths },
                |files, _pos, _w, cx| {
                    let text = if files.paths.len() == 1 {
                        Path::new(&files.paths[0])
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default()
                    } else {
                        format!("{} 個の項目", files.paths.len())
                    };
                    let text = SharedString::from(text);
                    cx.new(|_| DragPreview { text })
                },
            )
            .child(
                div()
                    .w(px(16.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon_el),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_color(rgb(0xe0e0e0))
                    .child(name),
            )
            .child(
                div()
                    .w(px(96.0))
                    .text_right()
                    .text_color(rgb(0x9a9a9a))
                    .child(size_text),
            )
            .child(
                div()
                    .w(px(96.0))
                    .text_color(rgb(0x9a9a9a))
                    .child(kind_text),
            )
            .into_any_element()
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
            .hover(|s| s.text_color(rgb(0xcccccc)))
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
        let count = self.entries.len();

        let name_hdr = self.header_cell("名前", SortCol::Name, cx);
        let size_hdr = self.header_cell("サイズ", SortCol::Size, cx);
        let type_hdr = self.header_cell("種類", SortCol::Type, cx);

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
            // ドロップ受け入れ: ペイン間 D&D とエクスプローラ等の外部 D&D。
            // ドロップ先はこのペインの表示中フォルダ (ADR 0009)。
            .drag_over::<DraggedFiles>(|style, _, _, _| style.bg(rgb(0x1f2c3a)))
            .on_drop(cx.listener(|this, files: &DraggedFiles, _w, cx| {
                this.drop_paths(files.paths.clone(), cx);
            }))
            .drag_over::<ExternalPaths>(|style, _, _, _| style.bg(rgb(0x1f2c3a)))
            .on_drop(cx.listener(|this, files: &ExternalPaths, _w, cx| {
                let paths: Vec<String> = files
                    .paths()
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();
                this.drop_paths(paths, cx);
            }))
            .flex()
            .flex_col()
            .size_full()
            .relative()
            .bg(rgb(0x1a1a1a))
            .text_color(rgb(0xdddddd))
            // 上部パスバー
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .h(px(34.0))
                    .bg(rgb(0x252525))
                    .child(
                        div()
                            .id("up")
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(0x3a3a3a))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x4a4a4a)))
                            .child("↑ 上へ")
                            .on_click(cx.listener(|this, _e, _w, cx| this.go_up(cx))),
                    )
                    .child(div().flex_1().overflow_hidden().child(path_text))
                    // ペイン分割 / 閉じる
                    .child(
                        div()
                            .id("split-row")
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(0x3a3a3a))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x4a4a4a)))
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
                            .rounded_md()
                            .bg(rgb(0x3a3a3a))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x4a4a4a)))
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
                            .rounded_md()
                            .bg(rgb(0x3a3a3a))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x5a3a3a)))
                            .child("×")
                            .on_click(cx.listener(|_t, _e, _w, cx| {
                                cx.emit(PaneEvent::CloseRequested)
                            })),
                    ),
            )
            // 列見出し
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .h(px(22.0))
                    .bg(rgb(0x202020))
                    .text_color(rgb(0x888888))
                    .child(div().w(px(16.0)))
                    .child(div().flex_1().child(name_hdr))
                    .child(div().w(px(96.0)).flex().justify_end().child(size_hdr))
                    .child(div().w(px(96.0)).child(type_hdr)),
            )
            // 一覧 (仮想化)
            .child(
                div().flex_1().overflow_hidden().child(
                    uniform_list(
                        "file-list",
                        count,
                        cx.processor(|this, range: Range<usize>, _w, cx| {
                            range.map(|ix| this.render_row(ix, cx)).collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(&self.scroll)
                    .size_full(),
                ),
            )
            // フッタ (ステータス + 選択数)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .h(px(24.0))
                    .bg(rgb(0x252525))
                    .text_color(rgb(0x9a9a9a))
                    .child(div().flex_1().overflow_hidden().child(status))
                    .child(sel_text),
            )
            // 入力モーダル (開いている時のみ)
            .children(self.render_modal(cx))
            // 右クリックメニュー (開いている時のみ)
            .children(self.render_context_menu(cx))
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

/// メニューの区切り線。
fn menu_sep() -> AnyElement {
    div()
        .h(px(1.0))
        .mx_1()
        .my_1()
        .bg(rgb(0x3f3f3f))
        .into_any_element()
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
