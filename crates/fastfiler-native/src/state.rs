//! アプリの状態オブジェクト群。
//!
//! ここに置くもの:
//!   - `CounterSink` (ファイル監視の通知カウント)
//!   - `ModalKind` (新規/リネームのモーダル種別)
//!   - `DragState` (内部 D&D 状態)
//!   - `SplitterTarget` (どのスプリッタをドラッグ中か)
//!   - `PaneState` (1 タブ = 1 ペインの全シグナル + アクション)
//!   - `Tab` (1 つのタブ = 2D ペインレイアウト)
//!   - `AppState` (タブ集合 + 設定 + D&D / スプリッタ等のグローバル状態)
//!
//! 値は `RwSignal` / `Arc` で Clone 可能なので、ビュー間で安全に共有できる。

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use fastfiler_domain::events::EventSink;
use fastfiler_domain::file_ops as fops;
use fastfiler_domain::watcher::WatcherCore;
use fastfiler_domain::win_clipboard as wcb;
use floem::kurbo::{Point, Rect};
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use parking_lot::Mutex;

use crate::fs_model::{
    initial_path, pretty_title, read_folder, sort_rows, unique_dest, FileRow, History, SortKey,
    Stats,
};
use crate::settings::AppSettings;

// ────────────────────────────────────────────────────────────────
// CounterSink
// ────────────────────────────────────────────────────────────────

pub struct CounterSink {
    pub counter: Mutex<u32>,
    pub tx: crossbeam_channel::Sender<()>,
}
impl EventSink for CounterSink {
    fn emit_json(&self, _event: &str, _payload: serde_json::Value) {
        *self.counter.lock() += 1;
        let _ = self.tx.try_send(());
    }
}

// ────────────────────────────────────────────────────────────────
// ModalKind / SplitterTarget / DragState
// ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum ModalKind {
    None,
    NewFolder,
    NewFile,
    /// 元の名前 (リネーム対象)
    Rename(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitterTarget {
    Tabs,
    Tree,
}

#[derive(Clone, Debug)]
pub struct DragState {
    pub source_pane: u64,
    pub paths: Vec<PathBuf>,
    pub start_window: Option<Point>,
    pub current_window: Point,
    pub active: bool,
}

// ────────────────────────────────────────────────────────────────
// PaneState
// ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PaneState {
    pub id: u64,
    pub title: RwSignal<String>,
    pub cur_path: RwSignal<PathBuf>,
    pub path_input: RwSignal<String>,
    pub rows: RwSignal<im::Vector<FileRow>>,
    pub stats: RwSignal<Stats>,
    pub selected: RwSignal<im::OrdSet<usize>>,
    /// 最後にクリックした行 (Shift+Click のアンカー / キーボード操作の起点)
    pub anchor: RwSignal<Option<usize>>,
    pub status_msg: RwSignal<String>,
    pub history: RwSignal<History>,
    pub watcher: Arc<WatcherCore>,
    pub sink: Arc<CounterSink>,
    /// 監視スレッドからのイベント受信用 (UI 側で signal 化)
    pub fs_event_signal: floem::reactive::ReadSignal<Option<()>>,
    pub watched: Arc<Mutex<Option<String>>>,
    pub fs_change_tick: RwSignal<u32>,
    pub show_hidden: RwSignal<bool>,
    /// モーダル種別 (新規フォルダ / リネーム)
    pub modal_kind: RwSignal<ModalKind>,
    pub modal_input: RwSignal<String>,
    pub sort_key: RwSignal<SortKey>,
    pub sort_desc: RwSignal<bool>,
}

static NEXT_PANE_ID: AtomicU64 = AtomicU64::new(1);

impl PaneState {
    pub fn new(start: PathBuf, show_hidden: RwSignal<bool>) -> Self {
        let mut initial_rows = read_folder(&start, show_hidden.get()).unwrap_or_default();
        sort_rows(&mut initial_rows, SortKey::Name, false);
        let initial_count = initial_rows.len();
        let (fs_tx, fs_rx) = crossbeam_channel::unbounded::<()>();
        let fs_signal = floem::ext_event::create_signal_from_channel(fs_rx);
        Self {
            id: NEXT_PANE_ID.fetch_add(1, Ordering::Relaxed),
            title: RwSignal::new(pretty_title(&start)),
            cur_path: RwSignal::new(start.clone()),
            path_input: RwSignal::new(start.to_string_lossy().into_owned()),
            rows: RwSignal::new(initial_rows),
            stats: RwSignal::new(Stats {
                load_ms: 0.0,
                count: initial_count,
            }),
            selected: RwSignal::new(im::OrdSet::new()),
            anchor: RwSignal::new(None),
            status_msg: RwSignal::new(String::from("ready")),
            history: RwSignal::new(History::default()),
            watcher: Arc::new(WatcherCore::default()),
            sink: Arc::new(CounterSink {
                counter: Mutex::new(0),
                tx: fs_tx,
            }),
            fs_event_signal: fs_signal,
            watched: Arc::new(Mutex::new(None)),
            fs_change_tick: RwSignal::new(0),
            show_hidden,
            modal_kind: RwSignal::new(ModalKind::None),
            modal_input: RwSignal::new(String::new()),
            sort_key: RwSignal::new(SortKey::Name),
            sort_desc: RwSignal::new(false),
        }
    }

    /// 別フォルダへナビゲーションする。push_history=true で現在パスを back に積む。
    pub fn navigate(&self, target: PathBuf, push_history: bool) {
        if !target.is_dir() {
            self.status_msg
                .set(format!("not a directory: {}", target.display()));
            return;
        }
        let t = Instant::now();
        match read_folder(&target, self.show_hidden.get_untracked()) {
            Ok(mut v) => {
                sort_rows(&mut v, self.sort_key.get_untracked(), self.sort_desc.get_untracked());
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                let len = v.len();
                let prev_path = self.cur_path.get_untracked();
                let same_path = prev_path == target;
                if push_history && !same_path {
                    self.history.update(|h| {
                        h.back.push_back(prev_path.clone());
                        h.forward.clear();
                    });
                }
                // 変化時のみ set してシグナル通知の連鎖を抑える
                if !same_path {
                    self.cur_path.set(target.clone());
                    let s = target.to_string_lossy().into_owned();
                    if self.path_input.with_untracked(|p| p != &s) {
                        self.path_input.set(s);
                    }
                    let title_new = pretty_title(&target);
                    if self.title.with_untracked(|p| p != &title_new) {
                        self.title.set(title_new);
                    }
                }
                self.rows.set(v);
                if !self.selected.with_untracked(|s| s.is_empty()) {
                    self.selected.set(im::OrdSet::new());
                }
                if self.anchor.get_untracked().is_some() {
                    self.anchor.set(None);
                }
                self.stats.set(Stats {
                    load_ms: ms,
                    count: len,
                });
                if self.status_msg.with_untracked(|m| m != "ok") {
                    self.status_msg.set(String::from("ok"));
                }

                let s = target.to_string_lossy().into_owned();
                let mut wp = self.watched.lock();
                let need_rewatch = wp.as_deref() != Some(s.as_str());
                if need_rewatch {
                    if let Some(old) = wp.as_ref() {
                        self.watcher.unwatch(old);
                    }
                    *wp = Some(s.clone());
                    *self.sink.counter.lock() = 0;
                    if self.fs_change_tick.get_untracked() != 0 {
                        self.fs_change_tick.set(0);
                    }
                    let sd: Arc<dyn EventSink> = self.sink.clone();
                    let _ = self.watcher.watch_with_sink(s, sd);
                }
            }
            Err(e) => self.status_msg.set(format!("read failed: {}", e)),
        }
    }

    pub fn back(&self) {
        let mut h = self.history.get();
        if let Some(prev) = h.back.pop_back() {
            let cur = self.cur_path.get();
            h.forward.push_back(cur);
            self.history.set(h);
            self.navigate(prev, false);
        }
    }
    pub fn forward(&self) {
        let mut h = self.history.get();
        if let Some(next) = h.forward.pop_back() {
            let cur = self.cur_path.get();
            h.back.push_back(cur);
            self.history.set(h);
            self.navigate(next, false);
        }
    }
    pub fn up(&self) {
        let cur = self.cur_path.get();
        if let Some(parent) = cur.parent() {
            self.navigate(parent.to_path_buf(), true);
        }
    }
    pub fn reload(&self) {
        let cur = self.cur_path.get_untracked();
        self.navigate(cur, false);
    }

    /// ファイル監視イベントによる軽量再読込。
    /// navigate と違い cur_path/title/path_input/history/watcher を触らず、
    /// rows と stats のみを差分検出して更新する。シグナル更新の連鎖を抑える。
    pub fn refresh_rows_only(&self) {
        let cur = self.cur_path.get_untracked();
        let show_hidden = self.show_hidden.get_untracked();
        let Ok(mut v) = read_folder(&cur, show_hidden) else { return; };
        sort_rows(&mut v, self.sort_key.get_untracked(), self.sort_desc.get_untracked());
        let new_len = v.len();
        // 簡易差分: 件数 + name 列が同じなら更新スキップ
        let same = self.rows.with_untracked(|r| {
            r.len() == new_len
                && r.iter().zip(v.iter()).all(|(a, b)| a.name == b.name && a.size == b.size && a.modified == b.modified)
        });
        if same {
            return;
        }
        // 削除等で行数が減った場合に備えて選択をクリア
        let cur_sel_max = self.selected.with_untracked(|s| s.iter().copied().max());
        if let Some(mx) = cur_sel_max {
            if mx >= new_len {
                self.selected.set(im::OrdSet::new());
                self.anchor.set(None);
            }
        }
        self.rows.set(v);
        self.stats.update(|s| s.count = new_len);
    }

    /// 選択行のフルパスを返す
    pub fn selected_paths(&self) -> Vec<PathBuf> {
        let rows = self.rows.get();
        let cur = self.cur_path.get();
        self.selected
            .get()
            .iter()
            .filter_map(|i| rows.get(*i).map(|r| cur.join(&r.name)))
            .collect()
    }

    /// 選択行が 1 件のときのみインデックスを返す
    pub fn single_selected(&self) -> Option<usize> {
        let s = self.selected.get();
        if s.len() == 1 {
            s.iter().next().copied()
        } else {
            None
        }
    }

    /// 行 idx をクリック (修飾キー対応)
    pub fn click_row(&self, idx: usize, ctrl: bool, shift: bool) {
        // 範囲外を即弾く (sort/reload 直後の古いインデックスでクラッシュさせない)
        let len = self.rows.with(|v| v.len());
        if idx >= len {
            return;
        }
        if shift {
            let anchor = self.anchor.get().unwrap_or(idx);
            let (lo, hi) = if anchor <= idx {
                (anchor, idx)
            } else {
                (idx, anchor)
            };
            let mut set = if ctrl {
                self.selected.get()
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
        let len = self.rows.with(|v| v.len());
        let mut set = im::OrdSet::new();
        for i in 0..len {
            set.insert(i);
        }
        self.selected.set(set);
    }

    /// 選択をゴミ箱へ送る
    pub fn delete_selected(&self) {
        let paths: Vec<String> = self
            .selected_paths()
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        if paths.is_empty() {
            return;
        }
        let n = paths.len();
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
        let name = self.rows.with(|v| v.get(idx).map(|r| r.name.clone()));
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
        let kind = self.modal_kind.get();
        let input = self.modal_input.get().trim().to_string();
        if input.is_empty() {
            self.close_modal();
            return;
        }
        let cur = self.cur_path.get();
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

    /// ソート列をクリック (同じ列なら方向トグル / 別列なら昇順)
    pub fn click_sort(&self, key: SortKey) {
        if self.sort_key.get() == key {
            self.sort_desc.update(|d| *d = !*d);
        } else {
            self.sort_key.set(key);
            self.sort_desc.set(false);
        }
        self.rows
            .update(|v| sort_rows(v, self.sort_key.get(), self.sort_desc.get()));
        self.selected.set(im::OrdSet::new());
        self.anchor.set(None);
    }

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
                self.status_msg.set(format!("クリップボード読込失敗: {}", e));
                return;
            }
        };
        let dst_dir = self.cur_path.get();
        let is_move = cb.op.eq_ignore_ascii_case("move");
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
            let res = if is_move {
                fops::move_path(src.clone(), dst.to_string_lossy().into_owned())
            } else {
                fops::copy_path(src.clone(), dst.to_string_lossy().into_owned())
            };
            match res {
                Ok(()) => ok += 1,
                Err(_) => err += 1,
            }
        }
        let label = if is_move { "移動" } else { "コピー" };
        self.status_msg
            .set(format!("{} 完了 OK={} / NG={}", label, ok, err));
        self.reload();
    }
}

// ────────────────────────────────────────────────────────────────
// Tab
// ────────────────────────────────────────────────────────────────

static NEXT_TAB_ID: AtomicU64 = AtomicU64::new(1);

/// 1 タブ = 複数フォルダペイン (左から 1..N)。
/// タブ名は panes\[0\] (primary) のフォルダ名から派生する。
#[derive(Clone)]
pub struct Tab {
    pub id: u64,
    /// 2D 列レイアウト: 外 = 左→右の列, 各列 = 上→下のペイン
    pub columns: RwSignal<im::Vector<RwSignal<im::Vector<PaneState>>>>,
    /// 現在フォーカスされているペイン id (split_active や close_pane の起点)
    pub active_pane: RwSignal<u64>,
}

impl Tab {
    pub fn new(start: PathBuf, show_hidden: RwSignal<bool>) -> Self {
        let p = PaneState::new(start, show_hidden);
        let pid = p.id;
        let col = RwSignal::new(im::vector![p]);
        Self {
            id: NEXT_TAB_ID.fetch_add(1, Ordering::Relaxed),
            columns: RwSignal::new(im::vector![col]),
            active_pane: RwSignal::new(pid),
        }
    }

    pub fn primary(&self) -> PaneState {
        self.columns.with(|cols| cols[0].with(|c| c[0].clone()))
    }

    /// 全ペインを上→下、左→右順にフラット化
    pub fn all_panes(&self) -> Vec<PaneState> {
        self.columns.with(|cols| {
            let mut out = Vec::new();
            for col in cols.iter() {
                col.with(|panes| {
                    for p in panes.iter() {
                        out.push(p.clone());
                    }
                });
            }
            out
        })
    }

    pub fn pane_count(&self) -> usize {
        self.columns
            .with(|cols| cols.iter().map(|c| c.with(|p| p.len())).sum())
    }

    /// 指定ペインの (列 index, 行 index) を返す
    pub fn locate(&self, pane_id: u64) -> Option<(usize, usize)> {
        self.columns.with(|cols| {
            for (ci, col) in cols.iter().enumerate() {
                if let Some(ri) = col.with(|panes| panes.iter().position(|p| p.id == pane_id)) {
                    return Some((ci, ri));
                }
            }
            None
        })
    }
}

// ────────────────────────────────────────────────────────────────
// AppState
// ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub tabs: RwSignal<im::Vector<Tab>>,
    /// active タブ id
    pub active: RwSignal<u64>,
    pub tab_cols: RwSignal<usize>,
    pub settings: AppSettings,
    pub settings_open: RwSignal<bool>,
    pub pane_rects: RwSignal<im::HashMap<u64, Rect>>,
    pub dragging: RwSignal<Option<DragState>>,
    /// スプリッタドラッグ中のターゲット (タブペイン / ツリーペイン 右端)
    pub splitter_drag: RwSignal<Option<SplitterTarget>>,
}

impl AppState {
    pub fn new(start: PathBuf) -> Self {
        let settings = AppSettings::new();

        // 起動時タブ復元: 保存された open_tabs があればそれを使う
        let saved_paths = settings.open_tabs.get_untracked();
        let mut tabs_vec: im::Vector<Tab> = im::Vector::new();
        for p in &saved_paths {
            let path = PathBuf::from(p);
            if path.exists() {
                tabs_vec.push_back(Tab::new(path, settings.show_hidden));
            }
        }
        if tabs_vec.is_empty() {
            tabs_vec.push_back(Tab::new(start, settings.show_hidden));
        }
        let id = tabs_vec.front().map(|t| t.id).unwrap_or(0);

        let initial_cols = settings
            .tab_columns
            .get_untracked()
            .parse::<usize>()
            .unwrap_or(1)
            .clamp(1, 4);
        Self {
            tabs: RwSignal::new(tabs_vec),
            active: RwSignal::new(id),
            tab_cols: RwSignal::new(initial_cols),
            settings,
            settings_open: RwSignal::new(false),
            pane_rects: RwSignal::new(im::HashMap::new()),
            dragging: RwSignal::new(None),
            splitter_drag: RwSignal::new(None),
        }
    }

    pub fn active_tab(&self) -> Option<Tab> {
        let id = self.active.get();
        self.tabs.get().iter().find(|t| t.id == id).cloned()
    }

    /// 互換用: アクティブタブの primary ペイン
    pub fn active_pane(&self) -> Option<PaneState> {
        self.active_tab().map(|t| t.primary())
    }

    pub fn add_tab(&self, start: PathBuf) {
        let tab = Tab::new(start, self.settings.show_hidden);
        let id = tab.id;
        self.tabs.update(|t| t.push_back(tab));
        self.active.set(id);
    }

    pub fn close_tab(&self, id: u64) {
        self.tabs.update(|t| {
            if let Some(idx) = t.iter().position(|x| x.id == id) {
                t.remove(idx);
            }
        });
        let remaining = self.tabs.get();
        if remaining.is_empty() {
            self.add_tab(initial_path());
        } else if !remaining.iter().any(|t| t.id == self.active.get()) {
            if let Some(last) = remaining.last() {
                self.active.set(last.id);
            }
        }
    }

    /// 指定 pane id を保持するペインを全タブから検索
    pub fn find_pane(&self, pane_id: u64) -> Option<PaneState> {
        self.tabs.with(|tabs| {
            for t in tabs.iter() {
                for p in t.all_panes() {
                    if p.id == pane_id {
                        return Some(p);
                    }
                }
            }
            None
        })
    }

    /// アクティブタブにペインを 1 つ追加 (最大 4)。
    /// vertical=false: 横分割 (アクティブペインの右側に新しい列を挿入、ペイン 1 つ)
    /// vertical=true:  縦分割 (アクティブペインを含む列に、その下にペイン追加)
    pub fn split_active(&self, vertical: bool) {
        if let Some(tab) = self.active_tab() {
            if tab.pane_count() >= 4 {
                return;
            }
            let active_id = tab.active_pane.get_untracked();
            let loc = tab.locate(active_id);
            let (col_idx, row_idx) = loc.unwrap_or((0, 0));
            let base = tab
                .all_panes()
                .into_iter()
                .find(|p| p.id == active_id)
                .map(|p| p.cur_path.get_untracked())
                .unwrap_or_else(|| tab.primary().cur_path.get_untracked());
            let show_hidden = self.settings.show_hidden;
            let new_pane = PaneState::new(base, show_hidden);
            let new_id = new_pane.id;

            if vertical {
                tab.columns.with(|cols| {
                    if let Some(col) = cols.get(col_idx) {
                        col.update(|panes| {
                            panes.insert(row_idx + 1, new_pane);
                        });
                    }
                });
            } else {
                let new_col = RwSignal::new(im::vector![new_pane]);
                tab.columns.update(|cols| {
                    cols.insert(col_idx + 1, new_col);
                });
            }
            tab.active_pane.set(new_id);
        }
    }

    /// 指定 pane を削除 (最後の 1 ペインは削除不可)
    pub fn close_pane(&self, pane_id: u64) {
        self.tabs.with(|tabs| {
            for t in tabs.iter() {
                if t.pane_count() <= 1 {
                    continue;
                }
                let loc = t.locate(pane_id);
                if let Some((ci, ri)) = loc {
                    let mut empty_col = false;
                    t.columns.with(|cols| {
                        if let Some(col) = cols.get(ci) {
                            col.update(|panes| {
                                panes.remove(ri);
                            });
                            empty_col = col.with(|p| p.is_empty());
                        }
                    });
                    if empty_col {
                        t.columns.update(|cols| {
                            cols.remove(ci);
                        });
                    }
                    // 残ったペインから新しいアクティブを選ぶ
                    if let Some(first) = t.all_panes().first() {
                        t.active_pane.set(first.id);
                    }
                    return;
                }
            }
        });
    }
}
