// Tabs panel — 縦型タブ + 列数セレクタ + [+] ボタン

use std::path::PathBuf;

use floem::event::{Event, EventListener};
use floem::peniko::Color;
use floem::prelude::*;
use floem::reactive::{SignalGet, SignalUpdate, SignalWith};
use floem::style::{CursorStyle, FlexWrap};
use floem::views::{container, dyn_container, empty, h_stack, label, scroll, v_stack, Decorators};

use crate::fs_model::{initial_path, list_drives};
use crate::state::{AppState, Tab};
use crate::theme;
pub fn tab_button(app: AppState, tab: Tab) -> impl IntoView {
    let id = tab.id;
    let root_sig = tab.root;
    let active = app.active;
    let locked = tab.locked;

    let title_label = label(move || {
        // first leaf の title を反応的に取得 (pretty_title が `C: name` / `🌐 share` の prefix 付与済み)
        root_sig.with(|r| {
            r.first_leaf()
                .map(|p| {
                    let t = p.title.get();
                    if t.is_empty() {
                        String::from("(root)")
                    } else {
                        t
                    }
                })
                .unwrap_or_else(|| String::from("(empty)"))
        })
    })
    .style(|s| s.flex_grow(1.0).min_width(0).padding_horiz(8));

    let close_btn = label(move || {
        if locked.get() {
            String::from("🔒")
        } else {
            String::from("×")
        }
    })
    .style(|s| {
        s.padding_horiz(8)
            .color(theme::text_label())
            .cursor(CursorStyle::Pointer)
    })
    .on_click_stop({
        let app = app.clone();
        move |_| {
            // ロック中はクリックで解除のみ。close は行わない。
            if locked.get_untracked() {
                locked.set(false);
                return;
            }
            app.close_tab(id);
        }
    });

    // 上端の黄色ライン (ロック中だけ表示)
    let lock_bar = empty().style(move |s| {
        let v = locked.get();
        let h = if v { 3.0 } else { 0.0 };
        s.width_full()
            .height(h)
            .background(Color::rgb8(0xFF, 0xC8, 0x00))
    });

    let row = h_stack((title_label, close_btn))
        .style(move |s| {
            let is_active = active.get() == id;
            let bg = if is_active {
                theme::accent_select()
            } else {
                theme::bg_zebra_b()
            };
            s.height(28)
                .width_full()
                .items_center()
                .background(bg)
                .border(1)
                .border_color(theme::border_default())
                .cursor(CursorStyle::Pointer)
        })
        .on_click_stop(move |_| active.set(id));

    v_stack((lock_bar, row))
        .style(|s| s.width_full())
        .on_event_stop(EventListener::PointerDown, move |e| {
            // 中クリック (auxiliary) でロック状態をトグル
            if let Event::PointerDown(p) = e {
                if p.button.is_auxiliary() {
                    locked.update(|v| *v = !*v);
                }
            }
        })
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
                let bg = if active {
                    theme::accent_select()
                } else {
                    theme::bg_header()
                };
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

    let app_for_up = app.clone();
    let app_for_dn = app.clone();
    let nav_btn = move |label_str: &'static str, delta: i32, app_n: AppState| -> floem::AnyView {
        label(move || String::from(label_str))
            .style(|s| {
                s.height(22)
                    .width(22)
                    .items_center()
                    .justify_center()
                    .color(theme::text_label())
                    .cursor(CursorStyle::Pointer)
                    .background(theme::bg_zebra_b())
                    .border(1)
                    .border_color(theme::border_default())
                    .font_bold()
            })
            .on_click_stop(move |_| app_n.shift_active_tab(delta))
            .into_any()
    };
    let up_btn = nav_btn("↑", -1, app_for_up);
    let dn_btn = nav_btn("↓", 1, app_for_dn);

    let plus = label(|| String::from("+"))
        .style(|s| {
            s.height(22)
                .width(22)
                .items_center()
                .justify_center()
                .color(theme::text_success())
                .cursor(CursorStyle::Pointer)
                .background(theme::bg_zebra_b())
                .border(1)
                .border_color(theme::border_default())
                .font_bold()
        })
        .on_click_stop(move |_| app_for_add.add_tab(initial_path()));

    let tabs_width_sig = app.settings.tabs_width;
    let app_for_grid = app.clone();
    let grid = dyn_container(
        move || (tabs_sig.get(), cols_sig.get().max(1), tabs_width_sig.get()),
        move |(tabs, cols, width_str)| {
            let app = app_for_grid.clone();
            let total = tabs.len();
            let rows = if total == 0 { 0 } else { total.div_ceil(cols) };
            // パネル幅から固定列幅を算出 (padding 8 + gap 2 * (cols-1))
            let panel_w = width_str
                .parse::<f32>()
                .unwrap_or(220.0)
                .clamp(120.0, 600.0);
            let inner = (panel_w - 8.0 - 2.0 * (cols.saturating_sub(1) as f32)).max(60.0);
            let col_w = (inner / cols as f32).max(50.0);
            let mut row_views: Vec<floem::AnyView> = Vec::with_capacity(rows);
            for r in 0..rows {
                let start = r * cols;
                let end = (start + cols).min(total);
                let mut row_items: Vec<floem::AnyView> = Vec::with_capacity(cols);
                for t in tabs.iter().skip(start).take(end - start) {
                    row_items.push(
                        container(tab_button(app.clone(), t.clone()))
                            .style(move |s| s.width(col_w))
                            .into_any(),
                    );
                }
                let row_view = floem::views::stack_from_iter(row_items)
                    .style(|s| s.flex_row().gap(2).width_full());
                row_views.push(row_view.into_any());
            }
            floem::views::stack_from_iter(row_views)
                .style(|s| s.flex_col().gap(2).width_full())
                .into_any()
        },
    )
    .style(|s| s.flex_col().width_full());

    // ヘッダー右側に ↑↓ + を配置して縦スペース節約
    let header = h_stack((
        label(|| String::from("Tabs")).style(|s| {
            s.padding(6)
                .font_bold()
                .flex_grow(1.0)
                .color(theme::text_label())
        }),
        cols_selector(app.clone()),
        plus,
        up_btn,
        dn_btn,
    ))
    .style(|s| s.items_center().gap(4).padding(2));

    // Drives セクション (TabsPanel 内に配置) — コンパクト表示
    let drives_items: Vec<floem::AnyView> = list_drives()
        .into_iter()
        .map(|d| {
            let app = app.clone();
            let d_label = d.trim_end_matches('\\').to_string();
            label(move || d_label.clone())
                .style(|s| {
                    s.height(20)
                        .padding_horiz(6)
                        .items_center()
                        .justify_center()
                        .cursor(CursorStyle::Pointer)
                        .color(theme::text_normal())
                        .background(theme::bg_zebra_b())
                        .border(1)
                        .border_color(theme::border_default())
                })
                .on_click_stop(move |_| {
                    if let Some(p) = app.active_pane() {
                        p.navigate(PathBuf::from(d.clone()), true);
                    }
                })
                .into_any()
        })
        .collect();
    let drives_section = container(
        floem::views::stack_from_iter(drives_items)
            .style(|s| s.flex_row().flex_wrap(FlexWrap::Wrap).gap(2)),
    )
    .style(|s| {
        s.padding_horiz(4)
            .padding_vert(2)
            .border_bottom(1)
            .border_color(theme::border_default())
    });

    let body = v_stack((
        header,
        drives_section,
        scroll(grid).style(|s| s.flex_grow(1.0).min_height(0).width_full()),
    ))
    .style(|s| s.flex_col().size_full().gap(4).padding(4));

    let tabs_width_sig2 = app.settings.tabs_width;
    container(body).style(move |s| {
        let w = tabs_width_sig2
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
