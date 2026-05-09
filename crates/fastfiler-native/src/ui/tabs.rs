// Tabs panel — 縦型タブ + 列数セレクタ + [+] ボタン

use std::path::PathBuf;

use floem::prelude::*;
use floem::reactive::{SignalGet, SignalUpdate, SignalWith};
use floem::style::CursorStyle;
use floem::views::{container, dyn_container, h_stack, label, scroll, v_stack, Decorators};

use crate::fs_model::{initial_path, list_drives};
use crate::state::{AppState, Tab};
use crate::theme;
pub fn tab_button(app: AppState, tab: Tab) -> impl IntoView {
    let id = tab.id;
    let root_sig = tab.root;
    let active = app.active;

    let title_label = label(move || {
        // first leaf の title を反応的に取得
        root_sig.with(|r| {
            r.first_leaf()
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
                .color(theme::text_label())
                .cursor(CursorStyle::Pointer)
        })
        .on_click_stop({
            let app = app.clone();
            move |_| app.close_tab(id)
        });

    h_stack((title_label, close_btn))
        .style(move |s| {
            let is_active = active.get() == id;
            let bg = if is_active { theme::accent_select() } else { theme::bg_zebra_b() };
            s.height(28)
                .width_full()
                .items_center()
                .background(bg)
                .border(1)
                .border_color(theme::border_default())
                .cursor(CursorStyle::Pointer)
        })
        .on_click_stop(move |_| active.set(id))
}

/// 列数セレクタ (1 / 2 / 3 / 4)
pub fn cols_selector(app: AppState) -> impl IntoView {
    let cols = app.tab_cols;
    let settings_tab_columns = app.settings.tab_columns;
    let make_btn = move |n: usize| {
        let cols = cols;
        label(move || format!("{}", n))
            .style(move |s| {
                let active = cols.get() == n;
                let bg = if active { theme::accent_select() } else { theme::bg_header() };
                s.width(22)
                    .height(22)
                    .items_center()
                    .padding_horiz(4)
                    .background(bg)
                    .border(1)
                    .border_color(theme::border_default())
                    .cursor(CursorStyle::Pointer)
                    .color(theme::text_normal())
            })
            .on_click_stop(move |_| {
                cols.set(n);
                settings_tab_columns.set(n.to_string());
            })
    };
    h_stack((
        label(|| String::from("Cols:")).style(|s| s.padding_horiz(4).color(theme::text_dim())),
        make_btn(1),
        make_btn(2),
        make_btn(3),
        make_btn(4),
    ))
    .style(|s| s.gap(2).items_center().padding(4))
}

/// タブを N 列に行優先でチャンク分割した縦型タブパネル。
pub fn tabs_panel(app: AppState) -> impl IntoView {
    let tabs_sig = app.tabs;
    let cols_sig = app.tab_cols;
    let app_for_add = app.clone();

    let plus = label(|| String::from("+ New Tab"))
        .style(|s| {
            s.height(26)
                .width_full()
                .items_center()
                .padding_horiz(8)
                .color(theme::text_success())
                .cursor(CursorStyle::Pointer)
                .background(theme::bg_zebra_b())
                .border(1)
                .border_color(theme::border_default())
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
        label(|| String::from("Tabs")).style(|s| s.padding(6).font_bold().flex_grow(1.0).color(theme::text_label())),
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
                        .color(theme::text_normal())
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
            .style(|s| s.padding_horiz(6).padding_vert(4).font_bold().color(theme::text_dim())),
        floem::views::stack_from_iter(drives_items).style(|s| s.flex_col()),
    ))
    .style(|s| {
        s.flex_col()
            .border_bottom(1)
            .border_color(theme::border_default())
    });

    let body = v_stack((header, drives_section, plus, scroll(grid).style(|s| s.flex_grow(1.0).width_full())))
        .style(|s| s.flex_col().size_full().gap(4).padding(4));

    let tabs_width_sig = app.settings.tabs_width;
    container(body).style(move |s| {
        let w = tabs_width_sig
            .get()
            .parse::<f32>()
            .unwrap_or(220.0)
            .clamp(120.0, 600.0);
        s.width(w)
            .height_full()
            .background(theme::bg_panel())
            .border_right(1)
            .border_color(theme::border_default())
    })
}




