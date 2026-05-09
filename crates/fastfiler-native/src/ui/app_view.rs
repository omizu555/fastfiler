// app_view + persist_window_state — アプリ全体レイアウト

use floem::event::{Event, EventListener};
use floem::prelude::*;
use floem::reactive::{SignalGet, SignalUpdate, SignalWith};
use floem::views::{container, dyn_container, h_stack, label, v_stack, Decorators};

use crate::fs_model::initial_path;
use crate::settings::{settings_view, AppSettings};
use crate::state::{AppState, PaneState, SplitterTarget};
use crate::theme;
use crate::ui::footer::footer_bar;
use crate::ui::pane::pane_view;
use crate::ui::splitter::splitter;
use crate::ui::tabs::tabs_panel;
use crate::ui::tree::tree_pane;

pub fn persist_window_state(settings: &AppSettings) {
    if let Err(e) = settings.save() {
        eprintln!("[settings] window-state save error: {}", e);
    }
}

pub fn app_view() -> impl IntoView {
    let app = AppState::new(initial_path());
    let settings_open = app.settings_open;
    let active = app.active;

    // 設定値変化時の自動保存 (タブ列数 / タブペイン幅 / ツリーペイン幅 / open_tabs)
    {
        let settings_for_save = app.settings.clone();
        let tab_columns_sig = app.settings.tab_columns;
        let tabs_width_sig = app.settings.tabs_width;
        let tree_width_sig = app.settings.tree_width;
        floem::reactive::create_effect(move |prev: Option<(String, String, String)>| {
            let cols = tab_columns_sig.get();
            let tw = tabs_width_sig.get();
            let trw = tree_width_sig.get();
            let cur = (cols, tw, trw);
            let changed = prev.as_ref().map_or(false, |p| p != &cur);
            if changed {
                if let Err(e) = settings_for_save.save() {
                    eprintln!("[settings] auto-save error: {}", e);
                }
            }
            cur
        });
    }
    // show_hidden 切替時に全 pane の rows を再読込 (dyn_container 再構築を避けるため独立 effect)
    {
        let app_for_hidden = app.clone();
        let show_hidden_sig = app.settings.show_hidden;
        floem::reactive::create_effect(move |prev: Option<bool>| {
            let cur = show_hidden_sig.get();
            if let Some(p) = prev {
                if p != cur {
                    let tabs_v = app_for_hidden.tabs.get_untracked();
                    for tab in tabs_v.iter() {
                        for pane in tab.all_panes() {
                            pane.refresh_rows_only();
                        }
                    }
                }
            }
            cur
        });
    }
    // タブ一覧 / 各タブの primary パス変化時に open_tabs を更新 + 保存
    {
        let app_for_tabs = app.clone();
        floem::reactive::create_effect(move |prev: Option<Vec<String>>| {
            let tabs_v = app_for_tabs.tabs.get();
            let paths: Vec<String> = tabs_v
                .iter()
                .map(|t| {
                    t.primary()
                        .cur_path
                        .get()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            let changed = prev.as_ref().map_or(true, |p| p != &paths);
            if changed {
                app_for_tabs.settings.open_tabs.set(paths.clone());
                if prev.is_some() {
                    if let Err(e) = app_for_tabs.settings.save() {
                        eprintln!("[settings] tabs auto-save error: {}", e);
                    }
                }
            }
            paths
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
                    let app_for_panes = app.clone();
                    let active_panes = dyn_container(
                        move || {
                            let id = active.get();
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

