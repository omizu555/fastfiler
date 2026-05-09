// Phase 3 step 3: 縦型タブ (N 列) + フォルダペインの GUI 実装。
//
// 構造:
//   App
//     ├─ Sidebar      (ドライブ一覧 — グローバル, 左端)
//     ├─ TabsPanel    (縦型タブを N 列で表示 + 列数セレクタ + [+])
//     └─ ActivePane   (1 タブ = 1 PaneState)
//           ├─ Toolbar (← → ↑ ⟳ パス入力 Open)
//           ├─ FileList (virtual_stack)
//           └─ Footer (status)
//
// タブはブラウザのような上部水平タブではなく、左側に縦に並ぶ。
// ユーザーは列数 (1〜4) を選択でき、タブを行優先で N 列に分割して表示する。
// PaneState は全フィールドが RwSignal/Arc で Clone 可能。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use fastfiler_domain::events::EventSink;
use fastfiler_domain::file_ops as fops;
use fastfiler_domain::fs as ffs;
use fastfiler_domain::watcher::WatcherCore;
use fastfiler_domain::win_clipboard as wcb;
use floem::event::{Event, EventListener};
use floem::keyboard::{Key, NamedKey};
use floem::kurbo::{Point, Rect};
use floem::menu::{Menu, MenuItem};
use floem::peniko::Color;
use floem::prelude::*;
use floem::style::CursorStyle;
use floem::views::{
    button, container, dyn_container, h_stack, label, scroll, text, text_input,
    v_stack, virtual_stack, Decorators, VirtualDirection, VirtualItemSize,
};
use parking_lot::Mutex;

mod settings;
use settings::{settings_view, AppSettings};

// ────────────────────────────────────────────────────────────────
// Data
// ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct FileRow {
    name: String,
    is_dir: bool,
    size: u64,
    modified: i64,
    size_text: String,
    mtime_text: String,
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 { format!("{} B", bytes) } else { format!("{:.1} {}", v, UNITS[u]) }
}

fn format_mtime(unix_secs: i64) -> String {
    if unix_secs <= 0 {
        return String::new();
    }
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(unix_secs, 0) {
        chrono::LocalResult::Single(dt) => format_dt(dt),
        chrono::LocalResult::Ambiguous(dt, _) => format_dt(dt),
        chrono::LocalResult::None => String::new(),
    }
}

fn format_dt(dt: chrono::DateTime<chrono::Local>) -> String {
    dt.format("%Y-%m-%d %H:%M").to_string()
}

fn read_folder(path: &Path, show_hidden: bool) -> Result<im::Vector<FileRow>, String> {
    let s = path.to_string_lossy().into_owned();
    let entries = ffs::list_dir(s).map_err(|e| e.to_string())?;
    let tmp: Vec<FileRow> = entries
        .into_iter()
        .filter(|e| show_hidden || !e.hidden)
        .map(|e| {
            let is_dir = e.kind == "dir";
            let size_text = if is_dir { String::from("<DIR>") } else { human_size(e.size) };
            let mtime_text = format_mtime(e.modified);
            FileRow {
                name: e.name,
                is_dir,
                size: e.size,
                modified: e.modified,
                size_text,
                mtime_text,
            }
        })
        .collect();
    Ok(tmp.into())
}

fn sort_rows(rows: &mut im::Vector<FileRow>, key: SortKey, desc: bool) {
    let mut tmp: Vec<FileRow> = rows.iter().cloned().collect();
    tmp.sort_by(|a, b| {
        // ディレクトリ優先 (常に上)
        match (a.is_dir, b.is_dir) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
        let ord = match key {
            SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortKey::Size => a.size.cmp(&b.size),
            SortKey::Modified => a.modified.cmp(&b.modified),
        };
        if desc { ord.reverse() } else { ord }
    });
    *rows = tmp.into();
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SortKey {
    Name,
    Size,
    Modified,
}

#[cfg(windows)]
fn list_drives() -> Vec<String> {
    let mut out = Vec::new();
    for c in b'C'..=b'Z' {
        let p = format!("{}:\\", c as char);
        if Path::new(&p).is_dir() {
            out.push(p);
        }
    }
    out
}
#[cfg(not(windows))]
fn list_drives() -> Vec<String> {
    vec![String::from("/")]
}

fn initial_path() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn pretty_title(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string_lossy().into_owned())
}

#[derive(Clone, Copy, Debug)]
struct Stats {
    load_ms: f64,
    count: usize,
}

#[derive(Default, Clone)]
struct History {
    back: im::Vector<PathBuf>,
    forward: im::Vector<PathBuf>,
}

struct CounterSink {
    counter: Mutex<u32>,
    tx: crossbeam_channel::Sender<()>,
}
impl EventSink for CounterSink {
    fn emit_json(&self, _event: &str, _payload: serde_json::Value) {
        *self.counter.lock() += 1;
        let _ = self.tx.try_send(());
    }
}

// ────────────────────────────────────────────────────────────────
// PaneState (1 タブ = 1 ペイン)
// ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
enum ModalKind {
    None,
    NewFolder,
    NewFile,
    /// 元の名前 (リネーム対象)
    Rename(String),
}

#[derive(Clone)]
struct PaneState {
    id: u64,
    title: RwSignal<String>,
    cur_path: RwSignal<PathBuf>,
    path_input: RwSignal<String>,
    rows: RwSignal<im::Vector<FileRow>>,
    stats: RwSignal<Stats>,
    selected: RwSignal<im::OrdSet<usize>>,
    /// 最後にクリックした行 (Shift+Click のアンカー / キーボード操作の起点)
    anchor: RwSignal<Option<usize>>,
    status_msg: RwSignal<String>,
    history: RwSignal<History>,
    watcher: Arc<WatcherCore>,
    sink: Arc<CounterSink>,
    /// 監視スレッドからのイベント受信用 (UI 側で signal 化)
    fs_event_signal: floem::reactive::ReadSignal<Option<()>>,
    watched: Arc<Mutex<Option<String>>>,
    fs_change_tick: RwSignal<u32>,
    show_hidden: RwSignal<bool>,
    /// モーダル種別 (新規フォルダ / リネーム)
    modal_kind: RwSignal<ModalKind>,
    modal_input: RwSignal<String>,
    sort_key: RwSignal<SortKey>,
    sort_desc: RwSignal<bool>,
}

static NEXT_PANE_ID: AtomicU64 = AtomicU64::new(1);

impl PaneState {
    fn new(start: PathBuf, show_hidden: RwSignal<bool>) -> Self {
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
            stats: RwSignal::new(Stats { load_ms: 0.0, count: initial_count }),
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
    fn navigate(&self, target: PathBuf, push_history: bool) {
        if !target.is_dir() {
            self.status_msg
                .set(format!("not a directory: {}", target.display()));
            return;
        }
        let t = Instant::now();
        match read_folder(&target, self.show_hidden.get()) {
            Ok(mut v) => {
                sort_rows(&mut v, self.sort_key.get(), self.sort_desc.get());
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                let len = v.len();
                if push_history {
                    let prev = self.cur_path.get();
                    self.history.update(|h| {
                        h.back.push_back(prev);
                        h.forward.clear();
                    });
                }
                self.cur_path.set(target.clone());
                self.path_input.set(target.to_string_lossy().into_owned());
                self.title.set(pretty_title(&target));
                self.rows.set(v);
                self.selected.set(im::OrdSet::new());
                self.anchor.set(None);
                self.stats.set(Stats { load_ms: ms, count: len });
                self.status_msg.set(String::from("ok"));

                let s = target.to_string_lossy().into_owned();
                let mut wp = self.watched.lock();
                if let Some(old) = wp.as_ref() {
                    self.watcher.unwatch(old);
                }
                *wp = Some(s.clone());
                *self.sink.counter.lock() = 0;
                self.fs_change_tick.set(0);
                let sd: Arc<dyn EventSink> = self.sink.clone();
                let _ = self.watcher.watch_with_sink(s, sd);
            }
            Err(e) => self.status_msg.set(format!("read failed: {}", e)),
        }
    }

    fn back(&self) {
        let mut h = self.history.get();
        if let Some(prev) = h.back.pop_back() {
            let cur = self.cur_path.get();
            h.forward.push_back(cur);
            self.history.set(h);
            self.navigate(prev, false);
        }
    }
    fn forward(&self) {
        let mut h = self.history.get();
        if let Some(next) = h.forward.pop_back() {
            let cur = self.cur_path.get();
            h.back.push_back(cur);
            self.history.set(h);
            self.navigate(next, false);
        }
    }
    fn up(&self) {
        let cur = self.cur_path.get();
        if let Some(parent) = cur.parent() {
            self.navigate(parent.to_path_buf(), true);
        }
    }
    fn reload(&self) {
        let cur = self.cur_path.get();
        self.navigate(cur, false);
    }

    /// 選択行のフルパスを返す
    fn selected_paths(&self) -> Vec<PathBuf> {
        let rows = self.rows.get();
        let cur = self.cur_path.get();
        self.selected
            .get()
            .iter()
            .filter_map(|i| rows.get(*i).map(|r| cur.join(&r.name)))
            .collect()
    }

    /// 選択行が 1 件のときのみインデックスを返す
    fn single_selected(&self) -> Option<usize> {
        let s = self.selected.get();
        if s.len() == 1 { s.iter().next().copied() } else { None }
    }

    /// 行 idx をクリック (修飾キー対応)
    fn click_row(&self, idx: usize, ctrl: bool, shift: bool) {
        if shift {
            let anchor = self.anchor.get().unwrap_or(idx);
            let (lo, hi) = if anchor <= idx { (anchor, idx) } else { (idx, anchor) };
            let mut set = if ctrl { self.selected.get() } else { im::OrdSet::new() };
            for i in lo..=hi {
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

    fn select_all(&self) {
        let len = self.rows.with(|v| v.len());
        let mut set = im::OrdSet::new();
        for i in 0..len {
            set.insert(i);
        }
        self.selected.set(set);
    }

    /// 選択をゴミ箱へ送る
    fn delete_selected(&self) {
        let paths: Vec<String> = self
            .selected_paths()
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        if paths.is_empty() {
            return;
        }
        let n = paths.len();
        match fops::delete_to_trash(paths) {
            Ok(()) => {
                self.status_msg.set(format!("ごみ箱へ送りました ({} 件)", n));
                self.reload();
            }
            Err(e) => self.status_msg.set(format!("削除失敗: {}", e)),
        }
    }

    fn open_new_folder_modal(&self) {
        self.modal_input.set(String::from("New Folder"));
        self.modal_kind.set(ModalKind::NewFolder);
    }

    fn open_new_file_modal(&self) {
        self.modal_input.set(String::from("new.txt"));
        self.modal_kind.set(ModalKind::NewFile);
    }

    fn open_rename_modal(&self) {
        let Some(idx) = self.single_selected() else {
            self.status_msg.set(String::from("リネームは 1 件のみ選択時"));
            return;
        };
        let name = self.rows.with(|v| v.get(idx).map(|r| r.name.clone()));
        if let Some(name) = name {
            self.modal_input.set(name.clone());
            self.modal_kind.set(ModalKind::Rename(name));
        }
    }

    fn close_modal(&self) {
        self.modal_kind.set(ModalKind::None);
        self.modal_input.set(String::new());
    }

    fn confirm_modal(&self) {
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
                        self.status_msg.set(format!("リネーム: {} → {}", orig, input));
                        self.close_modal();
                        self.reload();
                    }
                    Err(e) => self.status_msg.set(format!("リネーム失敗: {}", e)),
                }
            }
        }
    }

    /// ソート列をクリック (同じ列なら方向トグル / 別列なら昇順)
    fn click_sort(&self, key: SortKey) {
        if self.sort_key.get() == key {
            self.sort_desc.update(|d| *d = !*d);
        } else {
            self.sort_key.set(key);
            self.sort_desc.set(false);
        }
        self.rows.update(|v| sort_rows(v, self.sort_key.get(), self.sort_desc.get()));
        self.selected.set(im::OrdSet::new());
        self.anchor.set(None);
    }

    /// 選択行をクリップボードへ書き込み (op = "copy" or "move")
    fn clipboard_write(&self, op: &str) {
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
    fn clipboard_paste(&self) {
        let cb = match wcb::clipboard_read_paths() {
            Ok(Some(c)) => c,
            Ok(None) => {
                self.status_msg.set(String::from("クリップボードに項目がありません"));
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
            let Some(name) = name else { err += 1; continue };
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

/// 同名ファイルがあれば " (2)", " (3)"... を付与してユニークな宛先を返す。
fn unique_dest(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (base, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for n in 2..=9999u32 {
        let cand = dir.join(format!("{} ({}){}", base, n, ext));
        if !cand.exists() {
            return cand;
        }
    }
    p
}

// ────────────────────────────────────────────────────────────────
// AppState (タブ集合)
// ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct DragState {
    source_pane: u64,
    paths: Vec<PathBuf>,
    start_window: Option<Point>,
    current_window: Point,
    active: bool,
}

static NEXT_TAB_ID: AtomicU64 = AtomicU64::new(1);

/// 1 タブ = 複数フォルダペイン (左から 1..N)。
/// タブ名は panes[0] (primary) のフォルダ名から派生する。
#[derive(Clone)]
struct Tab {
    id: u64,
    panes: RwSignal<im::Vector<PaneState>>,
}

impl Tab {
    fn new(start: PathBuf, show_hidden: RwSignal<bool>) -> Self {
        let p = PaneState::new(start, show_hidden);
        Self {
            id: NEXT_TAB_ID.fetch_add(1, Ordering::Relaxed),
            panes: RwSignal::new(im::vector![p]),
        }
    }

    fn primary(&self) -> PaneState {
        self.panes.with(|v| v[0].clone())
    }

    /// 必要なら primary のパスを複製して n 個まで増やす。
    fn ensure_panes(&self, n: usize, show_hidden: RwSignal<bool>) {
        let cur_len = self.panes.with(|v| v.len());
        if cur_len >= n {
            return;
        }
        let base_path = self.primary().cur_path.get_untracked();
        self.panes.update(|v| {
            for _ in v.len()..n {
                v.push_back(PaneState::new(base_path.clone(), show_hidden));
            }
        });
    }
}

#[derive(Clone)]
struct AppState {
    tabs: RwSignal<im::Vector<Tab>>,
    /// active タブ id
    active: RwSignal<u64>,
    tab_cols: RwSignal<usize>,
    settings: AppSettings,
    settings_open: RwSignal<bool>,
    pane_rects: RwSignal<im::HashMap<u64, Rect>>,
    dragging: RwSignal<Option<DragState>>,
}

impl AppState {
    fn new(start: PathBuf) -> Self {
        let settings = AppSettings::new();
        let tab = Tab::new(start, settings.show_hidden);
        let id = tab.id;
        Self {
            tabs: RwSignal::new(im::vector![tab]),
            active: RwSignal::new(id),
            tab_cols: RwSignal::new(1),
            settings,
            settings_open: RwSignal::new(false),
            pane_rects: RwSignal::new(im::HashMap::new()),
            dragging: RwSignal::new(None),
        }
    }

    fn active_tab(&self) -> Option<Tab> {
        let id = self.active.get();
        self.tabs.get().iter().find(|t| t.id == id).cloned()
    }

    /// 互換用: アクティブタブの primary ペイン
    fn active_pane(&self) -> Option<PaneState> {
        self.active_tab().map(|t| t.primary())
    }

    fn add_tab(&self, start: PathBuf) {
        let tab = Tab::new(start, self.settings.show_hidden);
        let id = tab.id;
        self.tabs.update(|t| t.push_back(tab));
        self.active.set(id);
    }

    fn close_tab(&self, id: u64) {
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
    fn find_pane(&self, pane_id: u64) -> Option<PaneState> {
        self.tabs.with(|tabs| {
            for t in tabs.iter() {
                let found = t.panes.with(|ps| ps.iter().find(|p| p.id == pane_id).cloned());
                if let Some(p) = found {
                    return Some(p);
                }
            }
            None
        })
    }

    /// アクティブタブにペインを 1 つ追加 (最大 4)
    fn split_active(&self) {
        if let Some(tab) = self.active_tab() {
            let cur = tab.panes.with(|v| v.len());
            if cur >= 4 {
                return;
            }
            let base = tab.primary().cur_path.get_untracked();
            let show_hidden = self.settings.show_hidden;
            tab.panes.update(|v| {
                v.push_back(PaneState::new(base.clone(), show_hidden));
            });
        }
    }

    /// 指定 pane を含むタブからその pane を削除 (primary は削除不可)
    fn close_pane(&self, pane_id: u64) {
        self.tabs.with(|tabs| {
            for t in tabs.iter() {
                let pos = t.panes.with(|v| v.iter().position(|p| p.id == pane_id));
                if let Some(idx) = pos {
                    if idx == 0 {
                        return;
                    }
                    t.panes.update(|v| {
                        v.remove(idx);
                    });
                    return;
                }
            }
        });
    }
}

// ────────────────────────────────────────────────────────────────
// Views
// ────────────────────────────────────────────────────────────────

fn tab_button(app: AppState, tab: Tab) -> impl IntoView {
    let id = tab.id;
    let panes_sig = tab.panes;
    let active = app.active;

    let title_label = label(move || {
        // primary ペインの title を反応的に取得
        panes_sig.with(|v| {
            v.head()
                .map(|p| {
                    let t = p.title.get();
                    if t.is_empty() { String::from("(root)") } else { t }
                })
                .unwrap_or_else(|| String::from("(empty)"))
        })
    })
    .style(|s| s.flex_grow(1.0).padding_horiz(8));

    let close_btn = label(|| String::from("×"))
        .style(|s| {
            s.padding_horiz(8)
                .color(Color::rgb8(200, 200, 200))
                .cursor(CursorStyle::Pointer)
        })
        .on_click_stop({
            let app = app.clone();
            move |_| app.close_tab(id)
        });

    h_stack((title_label, close_btn))
        .style(move |s| {
            let is_active = active.get() == id;
            let bg = if is_active { Color::rgb8(58, 96, 158) } else { Color::rgb8(34, 34, 38) };
            s.height(28)
                .width_full()
                .items_center()
                .background(bg)
                .border(1)
                .border_color(Color::rgb8(60, 60, 60))
                .cursor(CursorStyle::Pointer)
        })
        .on_click_stop(move |_| active.set(id))
}

/// 列数セレクタ (1 / 2 / 3 / 4)
fn cols_selector(app: AppState) -> impl IntoView {
    let cols = app.tab_cols;
    let make_btn = move |n: usize| {
        let cols = cols;
        label(move || format!("{}", n))
            .style(move |s| {
                let active = cols.get() == n;
                let bg = if active { Color::rgb8(58, 96, 158) } else { Color::rgb8(40, 40, 44) };
                s.width(22)
                    .height(22)
                    .items_center()
                    .padding_horiz(4)
                    .background(bg)
                    .border(1)
                    .border_color(Color::rgb8(60, 60, 60))
                    .cursor(CursorStyle::Pointer)
                    .color(Color::rgb8(220, 220, 220))
            })
            .on_click_stop(move |_| cols.set(n))
    };
    h_stack((
        label(|| String::from("Cols:")).style(|s| s.padding_horiz(4).color(Color::rgb8(180, 180, 180))),
        make_btn(1),
        make_btn(2),
        make_btn(3),
        make_btn(4),
    ))
    .style(|s| s.gap(2).items_center().padding(4))
}

/// タブを N 列に行優先でチャンク分割した縦型タブパネル。
fn tabs_panel(app: AppState) -> impl IntoView {
    let tabs_sig = app.tabs;
    let cols_sig = app.tab_cols;
    let app_for_add = app.clone();

    let plus = label(|| String::from("+ New Tab"))
        .style(|s| {
            s.height(26)
                .width_full()
                .items_center()
                .padding_horiz(8)
                .color(Color::rgb8(180, 220, 180))
                .cursor(CursorStyle::Pointer)
                .background(Color::rgb8(34, 34, 38))
                .border(1)
                .border_color(Color::rgb8(60, 60, 60))
        })
        .on_click_stop(move |_| app_for_add.add_tab(initial_path()));

    let app_for_grid = app.clone();
    let grid = dyn_container(
        move || (tabs_sig.get(), cols_sig.get().max(1)),
        move |(tabs, cols)| {
            let app = app_for_grid.clone();
            let total = tabs.len();
            let per_col = if total == 0 { 0 } else { (total + cols - 1) / cols };
            let mut columns: Vec<floem::AnyView> = Vec::with_capacity(cols);
            for c in 0..cols {
                let start = c * per_col;
                let end = ((c + 1) * per_col).min(total);
                let mut col_items: Vec<floem::AnyView> = Vec::new();
                if start < end {
                    for t in tabs.iter().skip(start).take(end - start) {
                        col_items.push(tab_button(app.clone(), t.clone()).into_any());
                    }
                }
                let col_view = floem::views::stack_from_iter(col_items)
                    .style(|s| s.flex_col().flex_grow(1.0).gap(2));
                columns.push(container(col_view).style(|s| s.flex_grow(1.0)).into_any());
            }
            floem::views::stack_from_iter(columns)
                .style(|s| s.flex_row().gap(2).width_full())
                .into_any()
        },
    )
    .style(|s| s.flex_col().width_full());

    let header = h_stack((
        label(|| String::from("Tabs")).style(|s| s.padding(6).font_bold().flex_grow(1.0).color(Color::rgb8(200, 200, 200))),
        cols_selector(app.clone()),
    ))
    .style(|s| s.items_center().gap(4).padding(2));

    // Drives セクション (TabsPanel 内に配置)
    let drives_items: Vec<floem::AnyView> = list_drives()
        .into_iter()
        .map(|d| {
            let app = app.clone();
            let d_label = d.clone();
            label(move || d_label.clone())
                .style(|s| {
                    s.height(24)
                        .padding_horiz(8)
                        .items_center()
                        .cursor(CursorStyle::Pointer)
                        .color(Color::rgb8(220, 220, 220))
                })
                .on_click_stop(move |_| {
                    if let Some(p) = app.active_pane() {
                        p.navigate(PathBuf::from(d.clone()), true);
                    }
                })
                .into_any()
        })
        .collect();
    let drives_section = v_stack((
        label(|| String::from("Drives"))
            .style(|s| s.padding_horiz(6).padding_vert(4).font_bold().color(Color::rgb8(180, 180, 180))),
        floem::views::stack_from_iter(drives_items).style(|s| s.flex_col()),
    ))
    .style(|s| {
        s.flex_col()
            .border_bottom(1)
            .border_color(Color::rgb8(60, 60, 60))
    });

    let body = v_stack((header, drives_section, plus, scroll(grid).style(|s| s.flex_grow(1.0).width_full())))
        .style(|s| s.flex_col().size_full().gap(4).padding(4));

    container(body).style(|s| {
        s.width(220)
            .height_full()
            .background(Color::rgb8(28, 28, 32))
            .border_right(1)
            .border_color(Color::rgb8(60, 60, 60))
    })
}

fn pane_view(pane: PaneState, app: AppState) -> impl IntoView {
    let cur_path = pane.cur_path;
    let path_input = pane.path_input;
    let rows = pane.rows;
    let stats = pane.stats;
    let selected = pane.selected;
    let anchor = pane.anchor;
    let status_msg = pane.status_msg;
    let modal_kind = pane.modal_kind;
    let modal_input = pane.modal_input;
    let sink = pane.sink.clone();
    let fs_event_signal = pane.fs_event_signal;
    let fs_change_tick = pane.fs_change_tick;

    // ファイル監視 → 自動 reload (デバウンスなしの素朴版)
    let pane_for_fs = pane.clone();
    floem::reactive::create_effect(move |_| {
        // signal を track して変化時に reload
        if fs_event_signal.get().is_some() {
            fs_change_tick.update(|n| *n = n.wrapping_add(1));
            pane_for_fs.reload();
        }
    });

    let pane_for_open = pane.clone();
    let pane_for_back = pane.clone();
    let pane_for_forward = pane.clone();
    let pane_for_up = pane.clone();
    let pane_for_reload = pane.clone();
    let pane_for_dblclick = pane.clone();
    let pane_for_addr_enter = pane.clone();
    let pane_for_newfolder = pane.clone();
    let pane_for_newfile = pane.clone();
    let pane_for_rename = pane.clone();
    let pane_for_delete = pane.clone();
    let pane_for_modal_ok = pane.clone();
    let pane_for_modal_cancel = pane.clone();
    let pane_for_modal_enter = pane.clone();
    let pane_for_keys = pane.clone();
    let pane_for_click = pane.clone();
    let pane_for_ctxmenu = pane.clone();
    let pane_for_sort_name = pane.clone();
    let pane_for_sort_size = pane.clone();
    let pane_for_sort_mtime = pane.clone();
    let sort_key_sig = pane.sort_key;
    let sort_desc_sig = pane.sort_desc;

    let app_for_split = app.clone();
    let app_for_close_pane = app.clone();
    let pane_id_for_close = pane.id;
    let toolbar = h_stack((
        button("←").action(move || pane_for_back.back()),
        button("→").action(move || pane_for_forward.forward()),
        button("↑").action(move || pane_for_up.up()),
        button("⟳").action(move || pane_for_reload.reload()),
        text_input(path_input)
            .style(|s| {
                s.flex_grow(1.0)
                    .padding(4)
                    .border(1)
                    .border_color(Color::rgb8(120, 120, 120))
            })
            .on_event_stop(EventListener::KeyDown, move |e| {
                if let Event::KeyDown(ke) = e {
                    if matches!(ke.key.logical_key, Key::Named(NamedKey::Enter)) {
                        let s = path_input.get();
                        let p = PathBuf::from(s.trim());
                        pane_for_addr_enter.navigate(p, true);
                    }
                }
            }),
        button("Open").action(move || {
            let s = path_input.get();
            let p = PathBuf::from(s.trim());
            pane_for_open.navigate(p, true);
        }),
        button("New Folder").action(move || pane_for_newfolder.open_new_folder_modal()),
        button("New File").action(move || pane_for_newfile.open_new_file_modal()),
        button("Rename").action(move || pane_for_rename.open_rename_modal()),
        button("Delete").action(move || pane_for_delete.delete_selected()),
        button("⊟+").action(move || app_for_split.split_active()),
        button("✕").action(move || app_for_close_pane.close_pane(pane_id_for_close)),
    ))
    .style(|s| s.gap(6).padding(6).items_center());

    // パンくずリスト (現在パスを「>」区切りでクリック可能セグメント表示)
    let pane_for_crumb = pane.clone();
    let breadcrumb = dyn_container(
        move || cur_path.get(),
        move |p: PathBuf| {
            let mut acc = PathBuf::new();
            let mut items: Vec<floem::AnyView> = Vec::new();
            let mut first = true;
            for comp in p.components() {
                let part = comp.as_os_str().to_string_lossy().into_owned();
                if part.is_empty() {
                    continue;
                }
                acc.push(comp);
                if !first {
                    items.push(
                        label(|| String::from("›"))
                            .style(|s| {
                                s.padding_horiz(4).color(Color::rgb8(140, 140, 140))
                            })
                            .into_any(),
                    );
                }
                first = false;
                let target = acc.clone();
                let pane_seg = pane_for_crumb.clone();
                let display = if part.ends_with('\\') || part.ends_with('/') {
                    part.trim_end_matches(|c| c == '\\' || c == '/').to_string()
                } else {
                    part
                };
                let display = if display.is_empty() { String::from("/") } else { display };
                items.push(
                    label(move || display.clone())
                        .style(|s| {
                            s.padding_horiz(4)
                                .padding_vert(2)
                                .cursor(CursorStyle::Pointer)
                                .color(Color::rgb8(180, 200, 230))
                        })
                        .on_click_stop(move |_| pane_seg.navigate(target.clone(), true))
                        .into_any(),
                );
            }
            container(
                floem::views::stack_from_iter(items)
                    .style(|s| s.flex_row().items_center().gap(0)),
            )
            .style(|s| s.padding_horiz(8).padding_vert(2))
            .into_any()
        },
    )
    .style(|s| {
        s.height(22)
            .width_full()
            .background(Color::rgb8(30, 30, 34))
            .border_bottom(1)
            .border_color(Color::rgb8(50, 50, 50))
    });

    let arrow = move |k: SortKey| -> String {
        if sort_key_sig.get() == k {
            if sort_desc_sig.get() { String::from(" ▼") } else { String::from(" ▲") }
        } else {
            String::new()
        }
    };

    let header = h_stack((
        text("#").style(|s| s.width(60).padding_horiz(6).font_bold()),
        label(move || format!("Name{}", arrow(SortKey::Name)))
            .style(|s| s.flex_grow(1.0).padding_horiz(6).font_bold().cursor(CursorStyle::Pointer))
            .on_click_stop(move |_| pane_for_sort_name.click_sort(SortKey::Name)),
        label(move || format!("Size{}", arrow(SortKey::Size)))
            .style(|s| s.width(110).padding_horiz(6).font_bold().cursor(CursorStyle::Pointer))
            .on_click_stop(move |_| pane_for_sort_size.click_sort(SortKey::Size)),
        label(move || format!("Modified{}", arrow(SortKey::Modified)))
            .style(|s| s.width(140).padding_horiz(6).font_bold().cursor(CursorStyle::Pointer))
            .on_click_stop(move |_| pane_for_sort_mtime.click_sort(SortKey::Modified)),
    ))
    .style(|s| {
        s.height(24)
            .border_bottom(1)
            .border_color(Color::rgb8(80, 80, 80))
            .background(Color::rgb8(40, 40, 44))
    });

    let row_height: f64 = 22.0;

    let app_for_rows = app.clone();
    let pane_for_rows = pane.clone();
    let list = virtual_stack(
        VirtualDirection::Vertical,
        VirtualItemSize::Fixed(Box::new(move || row_height)),
        move || rows.get().enumerate(),
        move |(idx, _)| *idx,
        move |(idx, row): (usize, FileRow)| {
            let is_dir = row.is_dir;
            let bg_idx = idx;
            let name_for_open = row.name.clone();
            let row_name_for_drag = row.name.clone();
            let pane_dbl = pane_for_dblclick.clone();
            let pane_clk = pane_for_click.clone();
            let pane_for_drag = pane_for_rows.clone();
            let app_for_drag = app_for_rows.clone();
            h_stack((
                text(format!("{}", idx)).style(|s| s.width(60).padding_horiz(6)),
                text(row.name).style(move |s| {
                    let s = s.flex_grow(1.0).padding_horiz(6);
                    if is_dir { s.color(Color::rgb8(120, 200, 255)) } else { s }
                }),
                text(row.size_text)
                    .style(|s| s.width(110).padding_horiz(6).color(Color::rgb8(180, 180, 180))),
                text(row.mtime_text)
                    .style(|s| s.width(140).padding_horiz(6).color(Color::rgb8(180, 180, 180))),
            ))
            .style(move |s| {
                let zebra = if bg_idx % 2 == 0 {
                    Color::rgb8(28, 28, 30)
                } else {
                    Color::rgb8(34, 34, 38)
                };
                let sel = selected.with(|s| s.contains(&bg_idx));
                let bg = if sel { Color::rgb8(58, 96, 158) } else { zebra };
                s.height(row_height)
                    .items_center()
                    .background(bg)
                    .cursor(CursorStyle::Pointer)
            })
            .on_event_cont(EventListener::PointerDown, move |e| {
                if let Event::PointerDown(p) = e {
                    if !p.button.is_primary() {
                        return;
                    }
                    let cur = pane_for_drag.cur_path.get_untracked();
                    let row_path = cur.join(&row_name_for_drag);
                    let in_sel =
                        pane_for_drag.selected.with_untracked(|s| s.contains(&bg_idx));
                    let paths: Vec<PathBuf> = if in_sel {
                        let sel = pane_for_drag.selected.get_untracked();
                        let rs = pane_for_drag.rows.get_untracked();
                        sel.iter()
                            .filter_map(|i| rs.get(*i).map(|r| cur.join(&r.name)))
                            .collect()
                    } else {
                        vec![row_path]
                    };
                    app_for_drag.dragging.set(Some(DragState {
                        source_pane: pane_for_drag.id,
                        paths,
                        start_window: None,
                        current_window: Point::ZERO,
                        active: false,
                    }));
                }
            })
            .on_click_stop(move |e| {
                let (ctrl, shift) = if let Event::PointerUp(p) = e {
                    (p.modifiers.control(), p.modifiers.shift())
                } else {
                    (false, false)
                };
                pane_clk.click_row(bg_idx, ctrl, shift);
            })
            .on_double_click_stop(move |_| {
                let cur = cur_path.get();
                let target = cur.join(&name_for_open);
                if is_dir {
                    pane_dbl.navigate(target, true);
                } else {
                    let _ = fastfiler_domain::shell::open_with_shell(
                        target.to_string_lossy().into_owned(),
                    );
                }
            })
            .context_menu({
                let pane_ctx = pane_for_ctxmenu.clone();
                move || {
                    let p_open = pane_ctx.clone();
                    let p_reveal = pane_ctx.clone();
                    let p_cut = pane_ctx.clone();
                    let p_copy = pane_ctx.clone();
                    let p_paste = pane_ctx.clone();
                    let p_rename = pane_ctx.clone();
                    let p_delete = pane_ctx.clone();
                    // 右クリックされた行が未選択なら単独選択にする
                    if !p_open.selected.with(|s| s.contains(&bg_idx)) {
                        p_open.click_row(bg_idx, false, false);
                    }
                    Menu::new("")
                        .entry(MenuItem::new("開く").action({
                            let p = p_open.clone();
                            move || {
                                let cur = p.cur_path.get();
                                let name =
                                    p.rows.with(|v| v.get(bg_idx).map(|r| r.name.clone()));
                                let isd =
                                    p.rows.with(|v| v.get(bg_idx).map(|r| r.is_dir).unwrap_or(false));
                                if let Some(n) = name {
                                    let target = cur.join(n);
                                    if isd {
                                        p.navigate(target, true);
                                    } else {
                                        let _ = fastfiler_domain::shell::open_with_shell(
                                            target.to_string_lossy().into_owned(),
                                        );
                                    }
                                }
                            }
                        }))
                        .entry(MenuItem::new("エクスプローラで表示").action(move || {
                            let cur = p_reveal.cur_path.get();
                            let name = p_reveal
                                .rows
                                .with(|v| v.get(bg_idx).map(|r| r.name.clone()));
                            if let Some(n) = name {
                                let target = cur.join(n);
                                let _ = fastfiler_domain::shell::reveal_in_explorer(
                                    target.to_string_lossy().into_owned(),
                                );
                            }
                        }))
                        .separator()
                        .entry(MenuItem::new("切り取り").action(move || p_cut.clipboard_write("move")))
                        .entry(MenuItem::new("コピー").action(move || p_copy.clipboard_write("copy")))
                        .entry(MenuItem::new("貼り付け").action(move || p_paste.clipboard_paste()))
                        .separator()
                        .entry(MenuItem::new("名前の変更").action(move || p_rename.open_rename_modal()))
                        .entry(MenuItem::new("削除").action(move || p_delete.delete_selected()))
                }
            })
        },
    )
    .style(|s| s.flex_col().width_full());

    let scrollable = scroll(list).style(|s| s.width_full().flex_grow(1.0));

    let status = label(move || {
        let st = stats.get();
        let sel_count = selected.with(|s| s.len());
        let cnt = sink.counter.lock();
        let msg = status_msg.get();
        format!(
            "items: {}   load: {:.2} ms   selected: {}   fs-change: {}   {}",
            st.count, st.load_ms, sel_count, *cnt, msg
        )
    })
    .style(|s| {
        s.height(22)
            .padding_horiz(8)
            .items_center()
            .background(Color::rgb8(20, 20, 24))
            .border_top(1)
            .border_color(Color::rgb8(60, 60, 60))
    });

    // モーダル (新規フォルダ / リネーム入力)
    let modal_bar = dyn_container(
        move || modal_kind.get(),
        move |kind| match kind {
            ModalKind::None => container(label(|| String::new())).style(|s| s.height(0)).into_any(),
            other => {
                let title = match &other {
                    ModalKind::NewFolder => "新規フォルダ名",
                    ModalKind::NewFile => "新規ファイル名",
                    ModalKind::Rename(_) => "新しい名前",
                    ModalKind::None => "",
                };
                let pane_ok = pane_for_modal_ok.clone();
                let pane_cancel = pane_for_modal_cancel.clone();
                let pane_enter = pane_for_modal_enter.clone();
                h_stack((
                    label(move || title.to_string())
                        .style(|s| s.padding_horiz(8).color(Color::rgb8(220, 220, 220))),
                    text_input(modal_input)
                        .style(|s| {
                            s.flex_grow(1.0)
                                .padding(4)
                                .border(1)
                                .border_color(Color::rgb8(120, 120, 120))
                        })
                        .on_event_stop(EventListener::KeyDown, move |e| {
                            if let Event::KeyDown(ke) = e {
                                match &ke.key.logical_key {
                                    Key::Named(NamedKey::Enter) => pane_enter.confirm_modal(),
                                    Key::Named(NamedKey::Escape) => pane_enter.close_modal(),
                                    _ => {}
                                }
                            }
                        }),
                    button("OK").action(move || pane_ok.confirm_modal()),
                    button("Cancel").action(move || pane_cancel.close_modal()),
                ))
                .style(|s| {
                    s.gap(6)
                        .padding(6)
                        .items_center()
                        .background(Color::rgb8(36, 36, 40))
                        .border_bottom(1)
                        .border_color(Color::rgb8(80, 80, 80))
                })
                .into_any()
            }
        },
    );

    let pane_for_xbuttons = pane.clone();
    let pane_id = pane.id;
    let app_for_rect = app.clone();
    let app_for_rect2 = app.clone();
    let app_for_move = app.clone();
    let app_for_up = app.clone();
    v_stack((toolbar, breadcrumb, modal_bar, header, scrollable, status))
        .style(|s| s.size_full().flex_col())
        .on_resize(move |rect| {
            app_for_rect.pane_rects.update(|m| {
                let cur = m.get(&pane_id).copied().unwrap_or(Rect::ZERO);
                let new_rect = Rect::from_origin_size(cur.origin(), rect.size());
                m.insert(pane_id, new_rect);
            });
        })
        .on_move(move |pt| {
            app_for_rect2.pane_rects.update(|m| {
                let cur = m.get(&pane_id).copied().unwrap_or(Rect::ZERO);
                let new_rect = Rect::from_origin_size(pt, cur.size());
                m.insert(pane_id, new_rect);
            });
        })
        .on_event_cont(EventListener::PointerMove, move |e| {
            if let Event::PointerMove(p) = e {
                let dragging = app_for_move.dragging.get_untracked();
                let Some(_) = dragging else { return };
                let pane_origin = app_for_move
                    .pane_rects
                    .with_untracked(|m| m.get(&pane_id).map(|r| r.origin()).unwrap_or(Point::ZERO));
                let win_pt = Point::new(pane_origin.x + p.pos.x, pane_origin.y + p.pos.y);
                app_for_move.dragging.update(|d| {
                    if let Some(ds) = d {
                        if ds.source_pane != pane_id {
                            return;
                        }
                        if ds.start_window.is_none() {
                            ds.start_window = Some(win_pt);
                        }
                        ds.current_window = win_pt;
                        if !ds.active {
                            let s = ds.start_window.unwrap();
                            let dx = win_pt.x - s.x;
                            let dy = win_pt.y - s.y;
                            if (dx * dx + dy * dy).sqrt() > 5.0 {
                                ds.active = true;
                            }
                        }
                    }
                });
            }
        })
        .on_event_cont(EventListener::PointerUp, move |e| {
            if let Event::PointerUp(p) = e {
                let drag_opt = app_for_up.dragging.get_untracked();
                let Some(ds) = drag_opt else { return };
                app_for_up.dragging.set(None);
                if !ds.active || ds.source_pane != pane_id {
                    return;
                }
                let pane_origin = app_for_up
                    .pane_rects
                    .with_untracked(|m| m.get(&pane_id).map(|r| r.origin()).unwrap_or(Point::ZERO));
                let win_pt = Point::new(pane_origin.x + p.pos.x, pane_origin.y + p.pos.y);
                let copy = p.modifiers.control();
                let target_id = app_for_up.pane_rects.with_untracked(|m| {
                    m.iter()
                        .find_map(|(id, r)| if r.contains(win_pt) { Some(*id) } else { None })
                });
                let Some(target_id) = target_id else { return };
                if target_id == ds.source_pane {
                    return;
                }
                let target_pane = app_for_up.find_pane(target_id);
                let Some(tp) = target_pane else { return };
                let dest_dir = tp.cur_path.get_untracked();
                let mut ok = 0u32;
                let mut err = 0u32;
                for src in &ds.paths {
                    let name = match src.file_name() {
                        Some(n) => n.to_string_lossy().into_owned(),
                        None => {
                            err += 1;
                            continue;
                        }
                    };
                    let dst = unique_dest(&dest_dir, &name);
                    let res = if copy {
                        fops::copy_path(
                            src.to_string_lossy().into_owned(),
                            dst.to_string_lossy().into_owned(),
                        )
                    } else {
                        fops::move_path(
                            src.to_string_lossy().into_owned(),
                            dst.to_string_lossy().into_owned(),
                        )
                    };
                    match res {
                        Ok(()) => ok += 1,
                        Err(_) => err += 1,
                    }
                }
                let label = if copy { "コピー" } else { "移動" };
                tp.status_msg
                    .set(format!("D&D {} OK={} / NG={}", label, ok, err));
                tp.reload();
                if let Some(sp) = app_for_up.find_pane(ds.source_pane) {
                    sp.reload();
                }
            }
        })
        .on_event_stop(EventListener::PointerDown, move |e| {
            if let Event::PointerDown(p) = e {
                if p.button.is_x1() {
                    pane_for_xbuttons.back();
                } else if p.button.is_x2() {
                    pane_for_xbuttons.forward();
                }
            }
        })
        .on_event_stop(EventListener::KeyDown, move |e| {
            if let Event::KeyDown(ke) = e {
                let mods = &ke.modifiers;
                let ctrl = mods.control();
                let shift = mods.shift();
                match &ke.key.logical_key {
                    Key::Named(NamedKey::Delete) => {
                        pane_for_keys.delete_selected();
                        return;
                    }
                    Key::Named(NamedKey::F2) => {
                        pane_for_keys.open_rename_modal();
                        return;
                    }
                    Key::Named(NamedKey::Escape) => {
                        pane_for_keys.close_modal();
                        return;
                    }
                    Key::Character(c) if ctrl && (c == "a" || c == "A") => {
                        pane_for_keys.select_all();
                        return;
                    }
                    Key::Character(c) if ctrl && (c == "c" || c == "C") => {
                        pane_for_keys.clipboard_write("copy");
                        return;
                    }
                    Key::Character(c) if ctrl && (c == "x" || c == "X") => {
                        pane_for_keys.clipboard_write("move");
                        return;
                    }
                    Key::Character(c) if ctrl && (c == "v" || c == "V") => {
                        pane_for_keys.clipboard_paste();
                        return;
                    }
                    _ => {}
                }
                let len = rows.with(|v| v.len());
                if len == 0 {
                    return;
                }
                let cur = anchor.get().unwrap_or(0);
                let next = match &ke.key.logical_key {
                    Key::Named(NamedKey::ArrowDown) => Some((cur + 1).min(len - 1)),
                    Key::Named(NamedKey::ArrowUp) => Some(cur.saturating_sub(1)),
                    Key::Named(NamedKey::PageDown) => Some((cur + 30).min(len - 1)),
                    Key::Named(NamedKey::PageUp) => Some(cur.saturating_sub(30)),
                    Key::Named(NamedKey::Home) => Some(0),
                    Key::Named(NamedKey::End) => Some(len - 1),
                    _ => None,
                };
                if let Some(n) = next {
                    pane_for_keys.click_row(n, false, shift);
                }
            }
        })
}

fn _sidebar_unused(app: AppState) -> impl IntoView {
    let _ = app;
    label(|| String::new())
}

// ────────────────────────────────────────────────────────────────
// TreePane (フォルダツリー)
// ────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct TreeNode {
    path: PathBuf,
    name: String,
    expanded: RwSignal<bool>,
    loaded: RwSignal<bool>,
    children: RwSignal<im::Vector<TreeNode>>,
}

impl TreeNode {
    fn new(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        Self {
            path,
            name,
            expanded: RwSignal::new(false),
            loaded: RwSignal::new(false),
            children: RwSignal::new(im::Vector::new()),
        }
    }

    /// 子フォルダを 1 階層だけロード (lazy)。
    fn load_children(&self) {
        if self.loaded.get() {
            return;
        }
        let s = self.path.to_string_lossy().into_owned();
        if let Ok(dirs) = ffs::list_dirs(s, Some(false)) {
            let mut tmp: Vec<TreeNode> = dirs
                .into_iter()
                .map(|e| TreeNode::new(self.path.join(&e.name)))
                .collect();
            tmp.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            self.children.set(tmp.into_iter().collect());
        }
        self.loaded.set(true);
    }
}

fn render_tree_node(app: AppState, node: TreeNode, depth: usize) -> floem::AnyView {
    let expanded = node.expanded;
    let children = node.children;
    let path_for_nav = node.path.clone();
    let name_text = node.name.clone();

    let app_for_click = app.clone();
    let node_for_toggle = node.clone();

    let arrow = label(move || {
        if expanded.get() { String::from("▼") } else { String::from("▶") }
    })
    .style(|s| {
        s.width(14)
            .padding_horiz(2)
            .color(Color::rgb8(180, 180, 180))
            .cursor(CursorStyle::Pointer)
    })
    .on_click_stop(move |_| {
        let was = expanded.get();
        if !was {
            node_for_toggle.load_children();
        }
        expanded.set(!was);
    });

    let name_lbl = label(move || name_text.clone())
        .style(|s| {
            s.flex_grow(1.0)
                .padding_horiz(4)
                .cursor(CursorStyle::Pointer)
                .color(Color::rgb8(220, 220, 220))
        })
        .on_click_stop(move |_| {
            if let Some(p) = app_for_click.active_pane() {
                p.navigate(path_for_nav.clone(), true);
            }
        });

    let indent = (depth as f32) * 14.0 + 4.0;
    let row = h_stack((arrow, name_lbl)).style(move |s| {
        s.height(22).items_center().padding_left(indent)
    });

    let app_for_kids = app.clone();
    let kids = dyn_container(
        move || (expanded.get(), children.get()),
        move |(open, kids)| {
            if !open {
                return container(label(|| String::new()))
                    .style(|s| s.height(0))
                    .into_any();
            }
            let app = app_for_kids.clone();
            let items: Vec<floem::AnyView> = kids
                .into_iter()
                .map(|c| render_tree_node(app.clone(), c, depth + 1))
                .collect();
            floem::views::stack_from_iter(items)
                .style(|s| s.flex_col())
                .into_any()
        },
    );

    v_stack((row, kids)).style(|s| s.flex_col()).into_any()
}

fn tree_pane(app: AppState) -> impl IntoView {
    let roots: im::Vector<TreeNode> = list_drives()
        .into_iter()
        .map(|d| TreeNode::new(PathBuf::from(d)))
        .collect();
    let roots_sig = RwSignal::new(roots);

    let app_for_render = app.clone();
    let tree = dyn_container(
        move || roots_sig.get(),
        move |roots| {
            let app = app_for_render.clone();
            let items: Vec<floem::AnyView> = roots
                .into_iter()
                .map(|n| render_tree_node(app.clone(), n, 0))
                .collect();
            floem::views::stack_from_iter(items)
                .style(|s| s.flex_col())
                .into_any()
        },
    );

    let header = label(|| String::from("Tree"))
        .style(|s| s.padding(6).font_bold().color(Color::rgb8(200, 200, 200)));

    let body = v_stack((header, scroll(tree).style(|s| s.flex_grow(1.0).width_full())))
        .style(|s| s.flex_col().size_full());

    container(body).style(|s| {
        s.width(240)
            .height_full()
            .background(Color::rgb8(28, 28, 32))
            .border_right(1)
            .border_color(Color::rgb8(60, 60, 60))
    })
}

fn footer_bar(app: AppState) -> impl IntoView {
    let settings_open = app.settings_open;
    let active = app.active;
    let tabs = app.tabs;

    // フッター右側ステータス: アクティブペインのアイテム数
    let status = label(move || {
        let id = active.get();
        let cnt = tabs
            .get()
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.primary().stats.get().count)
            .unwrap_or(0);
        format!("items: {}", cnt)
    })
    .style(|s| s.flex_grow(1.0).padding_horiz(8).color(Color::rgb8(180, 180, 180)));

    let gear = label(|| String::from("⚙ Settings"))
        .style(|s| {
            s.height(22)
                .padding_horiz(10)
                .items_center()
                .cursor(CursorStyle::Pointer)
                .color(Color::rgb8(220, 220, 220))
                .border_left(1)
                .border_color(Color::rgb8(60, 60, 60))
        })
        .on_click_stop(move |_| settings_open.set(true));

    h_stack((status, gear)).style(|s| {
        s.height(26)
            .width_full()
            .items_center()
            .background(Color::rgb8(20, 20, 24))
            .border_top(1)
            .border_color(Color::rgb8(60, 60, 60))
    })
}

fn app_view() -> impl IntoView {
    let app = AppState::new(initial_path());
    let settings_open = app.settings_open;
    let active = app.active;
    let tabs = app.tabs;

    let switcher = dyn_container(
        move || settings_open.get(),
        {
            let app = app.clone();
            move |open| {
                if open {
                    settings_view(app.settings.clone(), settings_open).into_any()
                } else {
                    let app = app.clone();
                    let tab_columns_sig = app.settings.tab_columns;
                    let app_for_panes = app.clone();
                    let show_hidden_sig = app.settings.show_hidden;
                    let active_panes = dyn_container(
                        move || {
                            let id = active.get();
                            let setting_cols = tab_columns_sig
                                .get()
                                .parse::<usize>()
                                .unwrap_or(1)
                                .clamp(1, 4);
                            let tabs_v = tabs.get();
                            let active_tab = tabs_v.iter().find(|t| t.id == id).cloned()
                                .or_else(|| tabs_v.iter().next().cloned());
                            let panes: Vec<PaneState> = if let Some(t) = active_tab {
                                t.ensure_panes(setting_cols, show_hidden_sig);
                                // panes シグナル全体に依存させる (split/close で再描画)
                                t.panes.with(|v| v.iter().cloned().collect())
                            } else {
                                Vec::new()
                            };
                            let cols = panes.len();
                            (panes, cols)
                        },
                        move |(panes, _cols)| {
                            if panes.is_empty() {
                                return label(|| String::from("(no tab)"))
                                    .style(|s| s.size_full().padding(20))
                                    .into_any();
                            }
                            let views: Vec<floem::AnyView> = panes
                                .into_iter()
                                .enumerate()
                                .map(|(i, p)| {
                                    let app_for_pv = app_for_panes.clone();
                                    let v = container(pane_view(p, app_for_pv)).style(move |s| {
                                        let s = s.flex_grow(1.0).min_width(0).flex_basis(0).height_full();
                                        if i > 0 {
                                            s.border_left(1).border_color(Color::rgb8(60, 60, 60))
                                        } else {
                                            s
                                        }
                                    });
                                    v.into_any()
                                })
                                .collect();
                            container(
                                floem::views::stack_from_iter(views)
                                    .style(|s| s.flex_row().size_full()),
                            )
                            .style(|s| s.size_full())
                            .into_any()
                        },
                    )
                    .style(|s| s.flex_grow(1.0).min_height(0).flex_col());
                    let main_row = h_stack((
                        tabs_panel(app.clone()),
                        tree_pane(app.clone()),
                        active_panes,
                    ))
                    .style(|s| s.flex_grow(1.0).min_height(0).width_full());
                    v_stack((main_row, footer_bar(app.clone())))
                        .style(|s| s.size_full().flex_col())
                        .into_any()
                }
            }
        },
    )
    .style(|s| s.size_full());

    container(switcher).style(|s| {
        s.size_full()
            .background(Color::rgb8(24, 24, 28))
            .color(Color::rgb8(220, 220, 220))
            .font_size(13.0)
    })
}

fn main() {
    floem::launch(app_view);
}
