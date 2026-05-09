// FastFiler 脱 Tauri PoC: floem の virtual_stack で FileList の描画性能を実測する。
//
// Phase 2C-3:
//   - 実フォルダ読み込みを `fastfiler_domain::fs::list_dir` に切り替え
//     (Tauri なしでもドメインロジックがそのまま使えることを実証)
//   - `WatcherCore::watch_with_sink` でディレクトリ監視を起動し、変更件数を
//     ステータスバーに表示。Reload ボタンで一覧を更新する
//
// 目的:
//  - 1k / 10k / 100k / 1M 件の合成データ + 実フォルダ読み込みでロード時間 (ms) を測る
//  - スクロール / 選択操作中の体感 (フレーム落ち) を確認する
//  - ドメイン crate (Tauri 非依存) を別 GUI から再利用できることを確認する
//
// 非目的: アイコン取得 / D&D / シェル統合 (Phase 後段)。

use std::path::PathBuf;
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
    button, h_stack, label, scroll, text, text_input, v_stack, virtual_stack, Decorators,
    VirtualDirection, VirtualItemSize,
};
use parking_lot::Mutex;

#[derive(Clone, Debug)]
struct FileRow {
    name: String,
    is_dir: bool,
    size_text: String,
}

impl FileRow {
    fn new(name: String, size: u64, is_dir: bool) -> Self {
        let size_text = if is_dir { String::from("<DIR>") } else { human_size(size) };
        Self { name, is_dir, size_text }
    }
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

fn make_synthetic(n: usize) -> im::Vector<FileRow> {
    let exts = ["txt", "rs", "png", "log", "md", "json", "xml", "csv"];
    (0..n)
        .map(|i| {
            let is_dir = i % 17 == 0;
            let name = if is_dir {
                format!("folder_{:07}", i)
            } else {
                format!("file_{:07}.{}", i, exts[i % exts.len()])
            };
            let size = if is_dir { 0 } else { (i as u64).wrapping_mul(2_654_435_761) % (50 * 1024 * 1024) };
            FileRow::new(name, size, is_dir)
        })
        .collect()
}

/// Phase 2C-3: read_folder は fastfiler-domain の list_dir を使う。
fn read_folder(path: &std::path::Path) -> Result<im::Vector<FileRow>, String> {
    let s = path.to_string_lossy().into_owned();
    let entries = ffs::list_dir(s).map_err(|e| e.to_string())?;
    let mut tmp: Vec<FileRow> = entries
        .into_iter()
        .map(|e| {
            let is_dir = e.kind == "dir";
            FileRow::new(e.name, e.size, is_dir)
        })
        .collect();
    tmp.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(tmp.into())
}

#[derive(Clone, Copy, Debug)]
struct Stats {
    load_ms: f64,
    count: usize,
}

/// fs-change イベントを受け取るシンプルな EventSink。
/// イベント数を atomic にカウントするだけ。
struct CounterSink {
    count: Mutex<u32>,
    last_path: Mutex<String>,
}

impl EventSink for CounterSink {
    fn emit_json(&self, _event: &str, payload: serde_json::Value) {
        *self.count.lock() += 1;
        if let Some(p) = payload.get("path").and_then(|v| v.as_str()) {
            *self.last_path.lock() = p.to_string();
        }
    }
}

fn app_view() -> impl IntoView {
    let rows: RwSignal<im::Vector<FileRow>> = RwSignal::new(make_synthetic(1_000));
    let stats = RwSignal::new(Stats { load_ms: 0.0, count: 1_000 });
    let selected: RwSignal<Option<usize>> = RwSignal::new(None);
    let path_input = RwSignal::new(String::new());
    let watch_msg: RwSignal<String> = RwSignal::new(String::new());

    let watcher = Arc::new(WatcherCore::default());
    let sink: Arc<CounterSink> = Arc::new(CounterSink {
        count: Mutex::new(0),
        last_path: Mutex::new(String::new()),
    });
    let watched_path: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let load_synth = move |n: usize| {
        let t = Instant::now();
        let v = make_synthetic(n);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let len = v.len();
        rows.set(v);
        selected.set(None);
        stats.set(Stats { load_ms: ms, count: len });
    };

    let load_real = {
        let watcher = watcher.clone();
        let sink = sink.clone();
        let watched_path = watched_path.clone();
        move |p: PathBuf| {
            let t = Instant::now();
            match read_folder(&p) {
                Ok(v) => {
                    let ms = t.elapsed().as_secs_f64() * 1000.0;
                    let len = v.len();
                    rows.set(v);
                    selected.set(None);
                    stats.set(Stats { load_ms: ms, count: len });

                    let path_str = p.to_string_lossy().into_owned();
                    let mut wp = watched_path.lock();
                    if let Some(old) = wp.as_ref() {
                        watcher.unwatch(old);
                    }
                    *wp = Some(path_str.clone());
                    *sink.count.lock() = 0;
                    *sink.last_path.lock() = String::new();
                    let sink_dyn: Arc<dyn EventSink> = sink.clone();
                    if let Err(e) = watcher.watch_with_sink(path_str, sink_dyn) {
                        watch_msg.set(format!("watch failed: {}", e));
                    } else {
                        watch_msg.set(String::from("watching"));
                    }
                }
                Err(e) => {
                    eprintln!("[poc] read_folder failed: {}", e);
                    watch_msg.set(format!("read failed: {}", e));
                }
            }
        }
    };

    let load_real_for_open = load_real.clone();
    let load_real_for_reload = load_real;

    let toolbar = h_stack((
        button("1k").action(move || load_synth(1_000)),
        button("10k").action(move || load_synth(10_000)),
        button("100k").action(move || load_synth(100_000)),
        button("1M").action(move || load_synth(1_000_000)),
        text_input(path_input)
            .placeholder("E:\\some\\folder")
            .style(|s| s.width(360).padding(4).border(1).border_color(Color::rgb8(120, 120, 120))),
        button("Open Folder").action(move || {
            let s = path_input.get();
            let p = PathBuf::from(s.trim());
            if p.is_dir() {
                load_real_for_open(p);
            } else {
                eprintln!("[poc] not a directory: {:?}", p);
            }
        }),
        button("Reload").action({
            let watched_path = watched_path.clone();
            move || {
                let cur = watched_path.lock().clone();
                if let Some(p) = cur {
                    load_real_for_reload(PathBuf::from(p));
                }
            }
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
                let zebra = if bg_idx % 2 == 0 { Color::rgb8(28, 28, 30) } else { Color::rgb8(34, 34, 38) };
                let sel = selected.get() == Some(bg_idx);
                let bg = if sel { Color::rgb8(58, 96, 158) } else { zebra };
                s.height(row_height).items_center().background(bg).cursor(CursorStyle::Pointer)
            })
            .on_click_stop(move |_| {
                selected.set(Some(bg_idx));
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
            let watched = sink.count.lock();
            let msg = watch_msg.get();
            format!(
                "items: {}   load: {:.2} ms   selected: {:?}   fs-change: {}   {}",
                st.count, st.load_ms, sel, *watched, msg
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

    v_stack((toolbar, header, scrollable, status))
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
