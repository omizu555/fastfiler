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

use floem::event::{Event, EventListener};
use floem::prelude::*;
use floem::reactive::{SignalGet, SignalUpdate, SignalWith};
use floem::views::{container, dyn_container, h_stack, label, v_stack, Decorators};

mod fs_model;
mod settings;
mod state;
mod theme;
mod ui;
use fs_model::initial_path;
use settings::{settings_view, AppSettings};
use state::{AppState, PaneState, SplitterTarget};
use ui::footer::footer_bar;
use ui::pane::pane_view;
use ui::splitter::splitter;
use ui::tabs::tabs_panel;
use ui::tree::tree_pane;

// ────────────────────────────────────────────────────────────────
// (値型・状態は fs_model / state モジュールへ移動済)
// ────────────────────────────────────────────────────────────────

// ────────────────────────────────────────────────────────────────
// Views
// ────────────────────────────────────────────────────────────────

fn app_view() -> impl IntoView {
    let app = AppState::new(initial_path());
    let settings_open = app.settings_open;
    let active = app.active;

    // 設定値変化時の自動保存 (タブ列数 / タブペイン幅 / ツリーペイン幅 / open_tabs)
    {
        let settings_for_save = app.settings.clone();
        let tab_columns_sig = app.settings.tab_columns;
        let tabs_width_sig = app.settings.tabs_width;
        let tree_width_sig = app.settings.tree_width;
        floem::reactive::create_effect(move |prev: Option<()>| {
            // 全シグナルを track
            let _ = tab_columns_sig.get();
            let _ = tabs_width_sig.get();
            let _ = tree_width_sig.get();
            // 初回 (購読登録のための実行) は保存スキップ
            if prev.is_none() {
                return;
            }
            if let Err(e) = settings_for_save.save() {
                eprintln!("[settings] auto-save error: {}", e);
            }
        });
    }
    // タブ一覧 / 各タブの primary パス変化時に open_tabs を更新 + 保存
    {
        let app_for_tabs = app.clone();
        floem::reactive::create_effect(move |prev: Option<()>| {
            let tabs_v = app_for_tabs.tabs.get();
            let mut paths: Vec<String> = Vec::with_capacity(tabs_v.len());
            for t in tabs_v.iter() {
                let p = t.primary().cur_path.get();
                paths.push(p.to_string_lossy().into_owned());
            }
            app_for_tabs.settings.open_tabs.set(paths);
            if prev.is_none() {
                return;
            }
            if let Err(e) = app_for_tabs.settings.save() {
                eprintln!("[settings] tabs auto-save error: {}", e);
            }
        });
    }
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
                            let _setting_cols = tab_columns_sig
                                .get()
                                .parse::<usize>()
                                .unwrap_or(1)
                                .clamp(1, 4);
                            let _ = show_hidden_sig.get();
                            let tabs_v = tabs.get();
                            let active_tab = tabs_v.iter().find(|t| t.id == id).cloned()
                                .or_else(|| tabs_v.iter().next().cloned());
                            // 各列の (col_index, Vec<PaneState>) を集める
                            let layout: Vec<Vec<PaneState>> = if let Some(t) = active_tab {
                                t.columns.with(|cols| {
                                    cols.iter()
                                        .map(|col| col.with(|panes| panes.iter().cloned().collect()))
                                        .collect()
                                })
                            } else {
                                Vec::new()
                            };
                            layout
                        },
                        move |layout: Vec<Vec<PaneState>>| {
                            if layout.is_empty() || layout.iter().all(|c| c.is_empty()) {
                                return label(|| String::from("(no tab)"))
                                    .style(|s| s.size_full().padding(20))
                                    .into_any();
                            }
                            let col_count = layout.len();
                            let columns_views: Vec<floem::AnyView> = layout
                                .into_iter()
                                .enumerate()
                                .map(|(ci, panes)| {
                                    let app_for_col = app_for_panes.clone();
                                    let row_count = panes.len();
                                    let pane_views: Vec<floem::AnyView> = panes
                                        .into_iter()
                                        .enumerate()
                                        .map(|(ri, p)| {
                                            let app_for_pv = app_for_col.clone();
                                            container(pane_view(p, app_for_pv))
                                                .style(move |s| {
                                                    let s = s
                                                        .flex_grow(1.0)
                                                        .min_height(0)
                                                        .flex_basis(0)
                                                        .width_full();
                                                    if ri > 0 && row_count > 1 {
                                                        s.border_top(1)
                                                            .border_color(theme::border_default())
                                                    } else {
                                                        s
                                                    }
                                                })
                                                .into_any()
                                        })
                                        .collect();
                                    let col_view = floem::views::stack_from_iter(pane_views)
                                        .style(|s| s.flex_col().size_full());
                                    container(col_view)
                                        .style(move |s| {
                                            let s = s
                                                .flex_grow(1.0)
                                                .min_width(0)
                                                .flex_basis(0)
                                                .height_full();
                                            if ci > 0 && col_count > 1 {
                                                s.border_left(1)
                                                    .border_color(theme::border_default())
                                            } else {
                                                s
                                            }
                                        })
                                        .into_any()
                                })
                                .collect();
                            container(
                                floem::views::stack_from_iter(columns_views)
                                    .style(|s| s.flex_row().size_full()),
                            )
                            .style(|s| s.size_full())
                            .into_any()
                        },
                    )
                    .style(|s| s.flex_grow(1.0).min_height(0).flex_col());
                    let main_row = h_stack((
                        tabs_panel(app.clone()),
                        splitter(app.clone(), SplitterTarget::Tabs),
                        tree_pane(app.clone()),
                        splitter(app.clone(), SplitterTarget::Tree),
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

    container(switcher)
        .style(|s| {
            s.size_full()
                .background(theme::bg_root())
                .color(theme::text_normal())
                .font_size(13.0)
        })
        .on_event_cont(EventListener::PointerMove, {
            let app_for_split = app.clone();
            move |e| {
                if let Event::PointerMove(p) = e {
                    let target = app_for_split.splitter_drag.get_untracked();
                    if let Some(target) = target {
                        let x = p.pos.x as f32;
                        match target {
                            SplitterTarget::Tabs => {
                                let w = x.clamp(120.0, 600.0);
                                app_for_split.settings.tabs_width.set(format!("{:.0}", w));
                            }
                            SplitterTarget::Tree => {
                                let tabs_w = app_for_split
                                    .settings
                                    .tabs_width
                                    .get_untracked()
                                    .parse::<f32>()
                                    .unwrap_or(220.0);
                                // 4px 程度のスプリッタ自身の幅も考慮 (≒5)
                                let w = (x - tabs_w - 5.0).clamp(120.0, 600.0);
                                app_for_split.settings.tree_width.set(format!("{:.0}", w));
                            }
                        }
                    }
                }
            }
        })
        .on_event_cont(EventListener::PointerUp, {
            let app_for_split = app.clone();
            move |_| {
                if app_for_split.splitter_drag.get_untracked().is_some() {
                    app_for_split.splitter_drag.set(None);
                }
            }
        })
        .on_event_stop(EventListener::WindowResized, {
            let settings = app.settings.clone();
            move |e| {
                if let Event::WindowResized(sz) = e {
                    settings.window_w.set(Some(sz.width.max(0.0) as u32));
                    settings.window_h.set(Some(sz.height.max(0.0) as u32));
                    persist_window_state(&settings);
                }
            }
        })
        .on_event_stop(EventListener::WindowMoved, {
            let settings = app.settings.clone();
            move |e| {
                if let Event::WindowMoved(p) = e {
                    settings.window_x.set(Some(p.x as i32));
                    settings.window_y.set(Some(p.y as i32));
                    persist_window_state(&settings);
                }
            }
        })
        .on_event_stop(EventListener::WindowMaximizeChanged, {
            let settings = app.settings.clone();
            move |e| {
                if let Event::WindowMaximizeChanged(m) = e {
                    settings.window_maximized.set(*m);
                    persist_window_state(&settings);
                }
            }
        })
}


fn persist_window_state(settings: &AppSettings) {
    if let Err(e) = settings.save() {
        eprintln!("[settings] window-state save error: {}", e);
    }
}


fn main() {
    use floem::kurbo::{Point as KPoint, Size as KSize};
    use floem::window::WindowConfig;
    use settings::PersistedSettings;

    let p = PersistedSettings::load_or_default();
    let mut cfg = WindowConfig::default().title("FastFiler");
    if let (Some(w), Some(h)) = (p.window_w, p.window_h) {
        if w >= 200 && h >= 150 {
            cfg = cfg.size(KSize::new(w as f64, h as f64));
        }
    }
    if let (Some(x), Some(y)) = (p.window_x, p.window_y) {
        cfg = cfg.position(KPoint::new(x as f64, y as f64));
    }

    floem::Application::new()
        .window(move |_| app_view(), Some(cfg))
        .run();
}
