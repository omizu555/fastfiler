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
use fastfiler_domain::fs as ffs;
use fastfiler_domain::watcher::WatcherCore;
use floem::event::{Event, EventListener};
use floem::keyboard::{Key, NamedKey};
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
    size_text: String,
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

fn read_folder(path: &Path) -> Result<im::Vector<FileRow>, String> {
    let s = path.to_string_lossy().into_owned();
    let entries = ffs::list_dir(s).map_err(|e| e.to_string())?;
    let mut tmp: Vec<FileRow> = entries
        .into_iter()
        .map(|e| {
            let is_dir = e.kind == "dir";
            let size_text = if is_dir { String::from("<DIR>") } else { human_size(e.size) };
            FileRow { name: e.name, is_dir, size_text }
        })
        .collect();
    tmp.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(tmp.into())
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

struct CounterSink(Mutex<u32>);
impl EventSink for CounterSink {
    fn emit_json(&self, _event: &str, _payload: serde_json::Value) {
        *self.0.lock() += 1;
    }
}

// ────────────────────────────────────────────────────────────────
// PaneState (1 タブ = 1 ペイン)
// ────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct PaneState {
    id: u64,
    title: RwSignal<String>,
    cur_path: RwSignal<PathBuf>,
    path_input: RwSignal<String>,
    rows: RwSignal<im::Vector<FileRow>>,
    stats: RwSignal<Stats>,
    selected: RwSignal<Option<usize>>,
    status_msg: RwSignal<String>,
    history: RwSignal<History>,
    watcher: Arc<WatcherCore>,
    sink: Arc<CounterSink>,
    watched: Arc<Mutex<Option<String>>>,
    fs_change_tick: RwSignal<u32>,
}

static NEXT_PANE_ID: AtomicU64 = AtomicU64::new(1);

impl PaneState {
    fn new(start: PathBuf) -> Self {
        let initial_rows = read_folder(&start).unwrap_or_default();
        let initial_count = initial_rows.len();
        Self {
            id: NEXT_PANE_ID.fetch_add(1, Ordering::Relaxed),
            title: RwSignal::new(pretty_title(&start)),
            cur_path: RwSignal::new(start.clone()),
            path_input: RwSignal::new(start.to_string_lossy().into_owned()),
            rows: RwSignal::new(initial_rows),
            stats: RwSignal::new(Stats { load_ms: 0.0, count: initial_count }),
            selected: RwSignal::new(None),
            status_msg: RwSignal::new(String::from("ready")),
            history: RwSignal::new(History::default()),
            watcher: Arc::new(WatcherCore::default()),
            sink: Arc::new(CounterSink(Mutex::new(0))),
            watched: Arc::new(Mutex::new(None)),
            fs_change_tick: RwSignal::new(0),
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
        match read_folder(&target) {
            Ok(v) => {
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
                self.selected.set(None);
                self.stats.set(Stats { load_ms: ms, count: len });
                self.status_msg.set(String::from("ok"));

                let s = target.to_string_lossy().into_owned();
                let mut wp = self.watched.lock();
                if let Some(old) = wp.as_ref() {
                    self.watcher.unwatch(old);
                }
                *wp = Some(s.clone());
                *self.sink.0.lock() = 0;
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
}

// ────────────────────────────────────────────────────────────────
// AppState (タブ集合)
// ────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    tabs: RwSignal<im::Vector<PaneState>>,
    active: RwSignal<u64>,
    tab_cols: RwSignal<usize>,
    settings: AppSettings,
    settings_open: RwSignal<bool>,
}

impl AppState {
    fn new(start: PathBuf) -> Self {
        let pane = PaneState::new(start);
        let id = pane.id;
        Self {
            tabs: RwSignal::new(im::vector![pane]),
            active: RwSignal::new(id),
            tab_cols: RwSignal::new(1),
            settings: AppSettings::new(),
            settings_open: RwSignal::new(false),
        }
    }

    fn active_pane(&self) -> Option<PaneState> {
        let id = self.active.get();
        self.tabs.get().iter().find(|p| p.id == id).cloned()
    }

    fn add_tab(&self, start: PathBuf) {
        let pane = PaneState::new(start);
        let id = pane.id;
        self.tabs.update(|t| t.push_back(pane));
        self.active.set(id);
    }

    fn close_tab(&self, id: u64) {
        self.tabs.update(|t| {
            if let Some(idx) = t.iter().position(|p| p.id == id) {
                t.remove(idx);
            }
        });
        let remaining = self.tabs.get();
        if remaining.is_empty() {
            // 最後の 1 つは閉じない代わりに、初期パスで新規作成して維持する
            self.add_tab(initial_path());
        } else if !remaining.iter().any(|p| p.id == self.active.get()) {
            // アクティブが消えたら末尾を選ぶ
            if let Some(last) = remaining.last() {
                self.active.set(last.id);
            }
        }
    }
}

// ────────────────────────────────────────────────────────────────
// Views
// ────────────────────────────────────────────────────────────────

fn tab_button(app: AppState, pane: PaneState) -> impl IntoView {
    let id = pane.id;
    let title = pane.title;
    let active = app.active;

    let title_label = label(move || {
        let t = title.get();
        if t.is_empty() { String::from("(root)") } else { t }
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
                    for p in tabs.iter().skip(start).take(end - start) {
                        col_items.push(tab_button(app.clone(), p.clone()).into_any());
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

    let settings_open = app.settings_open;
    let header = h_stack((
        label(|| String::from("Tabs")).style(|s| s.padding(6).font_bold().flex_grow(1.0).color(Color::rgb8(200, 200, 200))),
        cols_selector(app.clone()),
        label(|| String::from("⚙"))
            .style(|s| {
                s.width(28)
                    .height(24)
                    .items_center()
                    .padding_horiz(4)
                    .color(Color::rgb8(220, 220, 220))
                    .cursor(CursorStyle::Pointer)
                    .background(Color::rgb8(40, 40, 44))
                    .border(1)
                    .border_color(Color::rgb8(60, 60, 60))
            })
            .on_click_stop(move |_| settings_open.set(true)),
    ))
    .style(|s| s.items_center().gap(4).padding(2));

    let body = v_stack((header, plus, scroll(grid).style(|s| s.flex_grow(1.0).width_full())))
        .style(|s| s.flex_col().size_full().gap(4).padding(4));

    container(body).style(|s| {
        s.width(220)
            .height_full()
            .background(Color::rgb8(28, 28, 32))
            .border_right(1)
            .border_color(Color::rgb8(60, 60, 60))
    })
}

fn pane_view(pane: PaneState) -> impl IntoView {
    let cur_path = pane.cur_path;
    let path_input = pane.path_input;
    let rows = pane.rows;
    let stats = pane.stats;
    let selected = pane.selected;
    let status_msg = pane.status_msg;
    let sink = pane.sink.clone();

    let pane_for_open = pane.clone();
    let pane_for_back = pane.clone();
    let pane_for_forward = pane.clone();
    let pane_for_up = pane.clone();
    let pane_for_reload = pane.clone();
    let pane_for_dblclick = pane.clone();

    let toolbar = h_stack((
        button("←").action(move || pane_for_back.back()),
        button("→").action(move || pane_for_forward.forward()),
        button("↑").action(move || pane_for_up.up()),
        button("⟳").action(move || pane_for_reload.reload()),
        text_input(path_input).style(|s| {
            s.flex_grow(1.0)
                .padding(4)
                .border(1)
                .border_color(Color::rgb8(120, 120, 120))
        }),
        button("Open").action(move || {
            let s = path_input.get();
            let p = PathBuf::from(s.trim());
            pane_for_open.navigate(p, true);
        }),
    ))
    .style(|s| s.gap(6).padding(6).items_center());

    let header = h_stack((
        text("#").style(|s| s.width(70).padding_horiz(6).font_bold()),
        text("Name").style(|s| s.flex_grow(1.0).padding_horiz(6).font_bold()),
        text("Size").style(|s| s.width(110).padding_horiz(6).font_bold()),
    ))
    .style(|s| {
        s.height(24)
            .border_bottom(1)
            .border_color(Color::rgb8(80, 80, 80))
            .background(Color::rgb8(40, 40, 44))
    });

    let row_height: f64 = 22.0;

    let list = virtual_stack(
        VirtualDirection::Vertical,
        VirtualItemSize::Fixed(Box::new(move || row_height)),
        move || rows.get().enumerate(),
        move |(idx, _)| *idx,
        move |(idx, row): (usize, FileRow)| {
            let is_dir = row.is_dir;
            let bg_idx = idx;
            let name_for_open = row.name.clone();
            let pane = pane_for_dblclick.clone();
            h_stack((
                text(format!("{}", idx)).style(|s| s.width(70).padding_horiz(6)),
                text(row.name).style(move |s| {
                    let s = s.flex_grow(1.0).padding_horiz(6);
                    if is_dir { s.color(Color::rgb8(120, 200, 255)) } else { s }
                }),
                text(row.size_text)
                    .style(|s| s.width(110).padding_horiz(6).color(Color::rgb8(180, 180, 180))),
            ))
            .style(move |s| {
                let zebra = if bg_idx % 2 == 0 {
                    Color::rgb8(28, 28, 30)
                } else {
                    Color::rgb8(34, 34, 38)
                };
                let sel = selected.get() == Some(bg_idx);
                let bg = if sel { Color::rgb8(58, 96, 158) } else { zebra };
                s.height(row_height)
                    .items_center()
                    .background(bg)
                    .cursor(CursorStyle::Pointer)
            })
            .on_click_stop(move |_| {
                selected.set(Some(bg_idx));
            })
            .on_double_click_stop(move |_| {
                if is_dir {
                    let cur = cur_path.get();
                    let target = cur.join(&name_for_open);
                    pane.navigate(target, true);
                }
            })
        },
    )
    .style(|s| s.flex_col().width_full());

    let scrollable = scroll(list).style(|s| s.width_full().flex_grow(1.0));

    let status = label(move || {
        let st = stats.get();
        let sel = selected.get();
        let cnt = sink.0.lock();
        let msg = status_msg.get();
        format!(
            "items: {}   load: {:.2} ms   selected: {:?}   fs-change: {}   {}",
            st.count, st.load_ms, sel, *cnt, msg
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

    v_stack((toolbar, header, scrollable, status))
        .style(|s| s.size_full().flex_col())
        .on_event_stop(EventListener::KeyDown, move |e| {
            if let Event::KeyDown(ke) = e {
                let len = rows.with(|v| v.len());
                if len == 0 {
                    return;
                }
                let cur = selected.get().unwrap_or(0);
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
                    selected.set(Some(n));
                }
            }
        })
}

fn sidebar(app: AppState) -> impl IntoView {
    let drives = list_drives();
    let items: Vec<_> = drives
        .into_iter()
        .map(|d| {
            let app = app.clone();
            let d_label = d.clone();
            label(move || d_label.clone())
                .style(|s| {
                    s.padding(6)
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

    scroll(
        v_stack((
            label(|| String::from("Drives"))
                .style(|s| s.padding(6).font_bold().color(Color::rgb8(180, 180, 180))),
            container(floem::views::stack_from_iter(items)).style(|s| s.flex_col()),
        ))
        .style(|s| s.flex_col()),
    )
    .style(|s| {
        s.width(160)
            .height_full()
            .background(Color::rgb8(28, 28, 32))
            .border_right(1)
            .border_color(Color::rgb8(60, 60, 60))
    })
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
                    let active_pane = dyn_container(
                        move || {
                            let id = active.get();
                            tabs.get().iter().find(|p| p.id == id).cloned()
                        },
                        move |maybe_pane| match maybe_pane {
                            Some(p) => container(pane_view(p)).style(|s| s.size_full()).into_any(),
                            None => label(|| String::from("(no tab)"))
                                .style(|s| s.size_full().padding(20))
                                .into_any(),
                        },
                    )
                    .style(|s| s.size_full().flex_col().flex_grow(1.0));
                    h_stack((
                        sidebar(app.clone()),
                        tabs_panel(app.clone()),
                        tree_pane(app.clone()),
                        active_pane,
                    ))
                    .style(|s| s.size_full().flex_grow(1.0))
                    .into_any()
                }
            }
        },
    );

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
