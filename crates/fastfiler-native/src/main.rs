// Phase 3 minimal explorer — floem 単独 (Tauri / WebView2 なし) で動く最小ファイラ。
//
// 機能:
//   - パスバー (text_input + Open)
//   - 戻る / 進む / 親へ ボタン (履歴スタック)
//   - virtual_stack によるファイル一覧 (1M件耐性)
//   - フォルダのダブルクリックで階層降下
//   - ドライブサイドバー (Windows のみ・C: 〜 Z: を実存チェック)
//   - WatcherCore による fs-change カウント表示
//   - ステータスバー (件数 / ロード時間 / fs-change 件数)
//
// 非機能 (Phase 3+ 以降):
//   - アイコン / サムネ / D&D / シェル統合 / 設定 / プラグイン / プレビュー / 検索
//
// fastfiler-domain (Tauri 非依存の純粋 Rust ロジック) を直接利用する。

use std::path::{Path, PathBuf};
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
    button, container, h_stack, label, scroll, text, text_input, v_stack, virtual_stack,
    Decorators, VirtualDirection, VirtualItemSize,
};
use parking_lot::Mutex;

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

#[derive(Clone, Copy, Debug)]
struct Stats {
    load_ms: f64,
    count: usize,
}

/// fs-change イベントを受け取る簡易 sink。
struct CounterSink(Mutex<u32>);
impl EventSink for CounterSink {
    fn emit_json(&self, _event: &str, _payload: serde_json::Value) {
        *self.0.lock() += 1;
    }
}

/// 戻る/進むの履歴スタック。
#[derive(Default, Clone)]
struct History {
    back: im::Vector<PathBuf>,
    forward: im::Vector<PathBuf>,
}

fn app_view() -> impl IntoView {
    let start = initial_path();
    let initial_rows = read_folder(&start).unwrap_or_default();
    let initial_count = initial_rows.len();

    let cur_path: RwSignal<PathBuf> = RwSignal::new(start.clone());
    let path_input: RwSignal<String> =
        RwSignal::new(start.to_string_lossy().into_owned());
    let rows: RwSignal<im::Vector<FileRow>> = RwSignal::new(initial_rows);
    let stats = RwSignal::new(Stats { load_ms: 0.0, count: initial_count });
    let selected: RwSignal<Option<usize>> = RwSignal::new(None);
    let status_msg: RwSignal<String> = RwSignal::new(String::from("ready"));
    let history: RwSignal<History> = RwSignal::new(History::default());

    let watcher = Arc::new(WatcherCore::default());
    let sink: Arc<CounterSink> = Arc::new(CounterSink(Mutex::new(0)));
    let watched: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    // 共通ナビゲーション。push_history=true のときは現在パスを back に積む。
    let navigate = {
        let watcher = watcher.clone();
        let sink = sink.clone();
        let watched = watched.clone();
        move |target: PathBuf, push_history: bool| {
            if !target.is_dir() {
                status_msg.set(format!("not a directory: {}", target.display()));
                return;
            }
            let t = Instant::now();
            match read_folder(&target) {
                Ok(v) => {
                    let ms = t.elapsed().as_secs_f64() * 1000.0;
                    let len = v.len();
                    if push_history {
                        let prev = cur_path.get();
                        history.update(|h| {
                            h.back.push_back(prev);
                            h.forward.clear();
                        });
                    }
                    cur_path.set(target.clone());
                    path_input.set(target.to_string_lossy().into_owned());
                    rows.set(v);
                    selected.set(None);
                    stats.set(Stats { load_ms: ms, count: len });
                    status_msg.set(String::from("ok"));

                    let s = target.to_string_lossy().into_owned();
                    let mut wp = watched.lock();
                    if let Some(old) = wp.as_ref() {
                        watcher.unwatch(old);
                    }
                    *wp = Some(s.clone());
                    *sink.0.lock() = 0;
                    let sd: Arc<dyn EventSink> = sink.clone();
                    let _ = watcher.watch_with_sink(s, sd);
                }
                Err(e) => status_msg.set(format!("read failed: {}", e)),
            }
        }
    };

    // 各操作で navigate を呼べるよう Rc/Box でなく Arc<Fn> にする
    let navigate = Arc::new(navigate);

    let do_open = {
        let navigate = navigate.clone();
        move || {
            let s = path_input.get();
            let p = PathBuf::from(s.trim());
            navigate(p, true);
        }
    };

    let do_up = {
        let navigate = navigate.clone();
        move || {
            let cur = cur_path.get();
            if let Some(parent) = cur.parent() {
                navigate(parent.to_path_buf(), true);
            }
        }
    };

    let do_back = {
        let navigate = navigate.clone();
        move || {
            let mut h = history.get();
            if let Some(prev) = h.back.pop_back() {
                let cur = cur_path.get();
                h.forward.push_back(cur);
                history.set(h);
                navigate(prev, false);
            }
        }
    };

    let do_forward = {
        let navigate = navigate.clone();
        move || {
            let mut h = history.get();
            if let Some(next) = h.forward.pop_back() {
                let cur = cur_path.get();
                h.back.push_back(cur);
                history.set(h);
                navigate(next, false);
            }
        }
    };

    let do_reload = {
        let navigate = navigate.clone();
        move || {
            let cur = cur_path.get();
            navigate(cur, false);
        }
    };

    let toolbar = h_stack((
        button("←").action(do_back),
        button("→").action(do_forward),
        button("↑").action(do_up),
        button("⟳").action(do_reload),
        text_input(path_input).style(|s| {
            s.flex_grow(1.0)
                .padding(4)
                .border(1)
                .border_color(Color::rgb8(120, 120, 120))
        }),
        button("Open").action(do_open),
    ))
    .style(|s| s.gap(6).padding(6).items_center());

    // サイドバー (ドライブ一覧)
    let drives = list_drives();
    let sidebar_navigate = navigate.clone();
    let sidebar_items: Vec<_> = drives
        .into_iter()
        .map(|d| {
            let nav = sidebar_navigate.clone();
            let d_label = d.clone();
            label(move || d_label.clone())
                .style(|s| {
                    s.padding(6)
                        .cursor(CursorStyle::Pointer)
                        .color(Color::rgb8(220, 220, 220))
                })
                .on_click_stop(move |_| {
                    nav(PathBuf::from(d.clone()), true);
                })
                .into_any()
        })
        .collect();
    let sidebar = scroll(
        v_stack((
            label(|| String::from("Drives"))
                .style(|s| s.padding(6).font_bold().color(Color::rgb8(180, 180, 180))),
            container(floem::views::stack_from_iter(sidebar_items))
                .style(|s| s.flex_col()),
        ))
        .style(|s| s.flex_col()),
    )
    .style(|s| {
        s.width(160)
            .height_full()
            .background(Color::rgb8(28, 28, 32))
            .border_right(1)
            .border_color(Color::rgb8(60, 60, 60))
    });

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
    let nav_for_dblclick = navigate.clone();

    let list = virtual_stack(
        VirtualDirection::Vertical,
        VirtualItemSize::Fixed(Box::new(move || row_height)),
        move || rows.get().enumerate(),
        move |(idx, _)| *idx,
        move |(idx, row): (usize, FileRow)| {
            let is_dir = row.is_dir;
            let bg_idx = idx;
            let name_for_open = row.name.clone();
            let nav = nav_for_dblclick.clone();
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
                    nav(target, true);
                }
            })
        },
    )
    .style(|s| s.flex_col().width_full());

    let scrollable = scroll(list).style(|s| s.width_full().flex_grow(1.0));

    let status = label({
        let sink = sink.clone();
        move || {
            let st = stats.get();
            let sel = selected.get();
            let cnt = sink.0.lock();
            let msg = status_msg.get();
            format!(
                "items: {}   load: {:.2} ms   selected: {:?}   fs-change: {}   {}",
                st.count, st.load_ms, sel, *cnt, msg
            )
        }
    })
    .style(|s| {
        s.height(22)
            .padding_horiz(8)
            .items_center()
            .background(Color::rgb8(20, 20, 24))
            .border_top(1)
            .border_color(Color::rgb8(60, 60, 60))
    });

    let main_area = v_stack((header, scrollable))
        .style(|s| s.flex_col().flex_grow(1.0).height_full());

    let body = h_stack((sidebar, main_area)).style(|s| s.flex_grow(1.0).width_full());

    v_stack((toolbar, body, status))
        .style(|s| {
            s.size_full()
                .background(Color::rgb8(24, 24, 28))
                .color(Color::rgb8(220, 220, 220))
                .font_size(13.0)
        })
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

fn main() {
    floem::launch(app_view);
}
