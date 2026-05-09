// app_view + persist_window_state — アプリ全体レイアウト

use floem::event::{Event, EventListener, EventPropagation};
use floem::prelude::*;
use floem::reactive::{SignalGet, SignalUpdate};
use floem::views::{container, dyn_container, label, v_stack, Decorators};

use crate::fs_model::initial_path;
use crate::settings::{settings_view, AppSettings};
use crate::state::{AppState, SplitDir, SplitNode, SplitterTarget};
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

/// SplitNode を再帰的に描画 (BSP)
fn render_split_node(node: SplitNode, app: AppState) -> floem::AnyView {
    match node {
        SplitNode::Leaf(p) => container(pane_view(p, app))
            .style(|s| s.size_full().min_width(0).min_height(0))
            .into_any(),
        SplitNode::Split { dir, children } => {
            let count = children.len();
            let views: Vec<floem::AnyView> = children
                .into_iter()
                .enumerate()
                .map(|(i, child)| {
                    let app = app.clone();
                    let child_view = render_split_node(child, app);
                    container(child_view)
                        .style(move |s| {
                            let s = s
                                .flex_grow(1.0)
                                .flex_basis(0)
                                .min_width(0)
                                .min_height(0);
                            if i > 0 && count > 1 {
                                match dir {
                                    SplitDir::Horizontal => {
                                        s.border_left(1).border_color(theme::border_default())
                                    }
                                    SplitDir::Vertical => {
                                        s.border_top(1).border_color(theme::border_default())
                                    }
                                }
                            } else {
                                s
                            }
                        })
                        .into_any()
                })
                .collect();
            let stack = floem::views::stack_from_iter(views).style(move |s| match dir {
                SplitDir::Horizontal => s.flex_row().size_full(),
                SplitDir::Vertical => s.flex_col().size_full(),
            });
            container(stack).style(|s| s.size_full()).into_any()
        }
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
    // タブ一覧 / 各タブの primary パス / BSP 構造変化時に open_tabs と tab_layouts を更新 + 保存
    {
        let app_for_tabs = app.clone();
        floem::reactive::create_effect(move |prev: Option<(Vec<String>, Vec<String>)>| {
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
            let layouts: Vec<String> = tabs_v
                .iter()
                .map(|t| {
                    // すべての leaf の cur_path を track して、副ペインの navigate でも保存が走るように
                    for pane in t.all_panes() {
                        let _ = pane.cur_path.get();
                    }
                    let saved = t.root.with(|r| r.to_saved());
                    serde_json::to_string(&saved).unwrap_or_default()
                })
                .collect();
            let cur = (paths, layouts);
            let changed = prev.as_ref().map_or(true, |p| p != &cur);
            if changed {
                app_for_tabs.settings.open_tabs.set(cur.0.clone());
                app_for_tabs.settings.tab_layouts.set(cur.1.clone());
                if prev.is_some() {
                    if let Err(e) = app_for_tabs.settings.save() {
                        eprintln!("[settings] tabs auto-save error: {}", e);
                    }
                }
            }
            cur
        });
    }
    let tabs = app.tabs;

    // テーマ/プリセット/アクセント変更時に theme_rev をインクリメントして全 UI を再構築する。
    // theme.rs の関数はクロージャ評価のたびに最新値を返すので、再構築すれば即時反映される。
    {
        let app_for_theme = app.clone();
        let theme_sig = app.settings.theme;
        let preset_sig = app.settings.theme_preset;
        let accent_sig = app.settings.accent_color;
        let icon_set_sig = app.settings.icon_set;
        let icon_pack_sig = app.settings.icon_pack;
        floem::reactive::create_effect(move |prev: Option<(String, String, String, String, String)>| {
            let cur = (
                theme_sig.get(),
                preset_sig.get(),
                accent_sig.get(),
                icon_set_sig.get(),
                icon_pack_sig.get(),
            );
            if let Some(p) = prev.as_ref() {
                if p != &cur {
                    crate::theme::set_mode_from_str(&cur.0);
                    crate::theme::set_preset_from_str(&cur.1);
                    crate::theme::set_accent_from_str(&cur.2);
                    app_for_theme.theme_rev.update(|v| *v = v.wrapping_add(1));
                }
            }
            cur
        });
    }

    let theme_rev = app.theme_rev;
    let switcher = dyn_container(
        move || (settings_open.get(), theme_rev.get()),
        {
            let app = app.clone();
            move |(open, _rev)| {
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
                            active_tab.map(|t| t.root.get())
                        },
                        move |root: Option<SplitNode>| {
                            let app = app_for_panes.clone();
                            match root {
                                None => label(|| String::from("(no tab)"))
                                    .style(|s| s.size_full().padding(20))
                                    .into_any(),
                                Some(node) => render_split_node(node, app).into_any(),
                            }
                        },
                    )
                    .style(|s| s.flex_grow(1.0).min_height(0).min_width(0).flex_col());
                    let app_for_layout = app.clone();
                    let active_panes_view = active_panes.into_any();
                    let layout_sig = app.settings.workspace_layout;
                    let dock_tabs_sig = app.settings.panel_dock_tabs;
                    let dock_tree_sig = app.settings.panel_dock_tree;
                    let active_panes_holder = std::rc::Rc::new(std::cell::RefCell::new(Some(active_panes_view)));
                    let main_row = dyn_container(
                        move || {
                            let layout = layout_sig.get();
                            let dt = dock_tabs_sig.get();
                            let dr = dock_tree_sig.get();
                            (layout, dt, dr)
                        },
                        move |(layout, dock_tabs, dock_tree)| {
                            let app = app_for_layout.clone();
                            let tabs_hidden = layout == "tabsHidden" || dock_tabs == "hidden";
                            let tree_hidden = dock_tree == "hidden";
                            let tabs_right = dock_tabs == "right" || layout == "tabsRight";
                            let tree_right = dock_tree == "right";

                            let mut items: Vec<floem::AnyView> = Vec::new();
                            if !tabs_hidden && !tabs_right {
                                items.push(tabs_panel(app.clone()).into_any());
                                items.push(splitter(app.clone(), SplitterTarget::Tabs).into_any());
                            }
                            if !tree_hidden && !tree_right {
                                items.push(tree_pane(app.clone()).into_any());
                                items.push(splitter(app.clone(), SplitterTarget::Tree).into_any());
                            }
                            // 中央 active_panes (ホルダから取り出し、再構築の都度新規生成)
                            let center = active_panes_holder
                                .borrow_mut()
                                .take()
                                .unwrap_or_else(|| {
                                    label(|| String::from("(no center)"))
                                        .style(|s| s.size_full())
                                        .into_any()
                                });
                            items.push(center);
                            if !tree_hidden && tree_right {
                                items.push(splitter(app.clone(), SplitterTarget::Tree).into_any());
                                items.push(tree_pane(app.clone()).into_any());
                            }
                            if !tabs_hidden && tabs_right {
                                items.push(splitter(app.clone(), SplitterTarget::Tabs).into_any());
                                items.push(tabs_panel(app.clone()).into_any());
                            }
                            floem::views::stack_from_iter(items)
                                .style(|s| s.flex_row().flex_grow(1.0).min_height(0).width_full())
                                .into_any()
                        },
                    )
                    .style(|s| s.flex_grow(1.0).min_height(0).width_full());
                    v_stack((main_row, footer_bar(app.clone())))
                        .style(|s| s.size_full().flex_col())
                        .into_any()
                }
            }
        },
    )
    .style(|s| s.size_full());

    let ui_font_size_sig = app.settings.ui_font_size;
    container(switcher)
        .style(move |s| {
            let fs = ui_font_size_sig.get().parse::<f32>().unwrap_or(13.0).clamp(8.0, 32.0);
            s.size_full()
                .background(theme::bg_root())
                .color(theme::text_normal())
                .font_size(fs)
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
        .on_event(EventListener::KeyDown, {
            let app = app.clone();
            move |e| {
                if let Event::KeyDown(ke) = e {
                    if let Some(action) = crate::hotkeys::resolve_action(&app, ke) {
                        if crate::hotkeys::dispatch_action(&app, &action) {
                            return EventPropagation::Stop;
                        }
                    }
                }
                EventPropagation::Continue
            }
        })
}

