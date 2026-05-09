//! 設定ダイアログで使う共通フォーム部品。
//! 設定ダイアログ専用なので外部公開はしない (`pub(super)`)。

use floem::event::EventListener;
use floem::prelude::*;
use floem::style::CursorStyle;
use floem::views::{
    button, container, dyn_container, h_stack, label, text_input, v_stack, Decorators,
};

use crate::theme;
use crate::theme::fonts;

pub(super) fn section_label(text_str: &'static str) -> impl IntoView {
    label(move || text_str.to_string()).style(|s| {
        s.padding(8)
            .font_bold()
            .color(theme::text_normal())
            .border_bottom(1)
            .border_color(theme::border_default())
    })
}

pub(super) fn row_input(title: &'static str, sig: RwSignal<String>) -> impl IntoView {
    h_stack((
        label(move || title.to_string())
            .style(|s| s.width(220).padding(6).color(theme::text_label())),
        text_input(sig).style(|s| {
            s.flex_grow(1.0)
                .padding(4)
                .border(1)
                .border_color(theme::border_strong())
                .background(theme::bg_modal())
                .color(theme::text_normal())
        }),
    ))
    .style(|s| s.padding(4).items_center().gap(8))
}

pub(super) fn row_check(title: &'static str, sig: RwSignal<bool>) -> impl IntoView {
    h_stack((
        label(move || {
            let mark = if sig.get() { "[v]" } else { "[ ]" };
            format!("{} {}", mark, title)
        })
        .style(|s| s.padding(6).cursor(CursorStyle::Pointer).color(theme::text_normal()))
        .on_click_stop(move |_| sig.set(!sig.get())),
    ))
    .style(|s| s.padding(4))
}

pub(super) fn row_select(
    title: &'static str,
    sig: RwSignal<String>,
    options: Vec<&'static str>,
) -> impl IntoView {
    let buttons: Vec<floem::AnyView> = options
        .into_iter()
        .map(|opt| {
            let s = sig;
            label(move || opt.to_string())
                .style(move |st| {
                    let active = s.get() == opt;
                    let bg = if active { theme::accent_select() } else { theme::bg_chrome() };
                    st.padding_horiz(10)
                        .padding_vert(4)
                        .background(bg)
                        .border(1)
                        .border_color(theme::border_default())
                        .cursor(CursorStyle::Pointer)
                        .color(theme::text_normal())
                })
                .on_click_stop(move |_| s.set(opt.to_string()))
                .into_any()
        })
        .collect();

    h_stack((
        label(move || title.to_string())
            .style(|s| s.width(220).padding(6).color(theme::text_label())),
        floem::views::stack_from_iter(buttons).style(|s| s.flex_row().gap(2)),
    ))
    .style(|s| s.padding(4).items_center().gap(8))
}

/// インストール済みフォントから選ぶ簡易コンボボックス。
/// text_input でフィルタ可能、▼ で候補を開閉、候補クリックで確定する。
pub(super) fn row_font(title: &'static str, sig: RwSignal<String>) -> impl IntoView {
    let all_fonts: Vec<String> = fonts::installed_fonts().to_vec();
    if all_fonts.is_empty() {
        return row_input(title, sig).into_any();
    }
    let open = RwSignal::new(false);
    let filter = RwSignal::new(sig.get_untracked());

    // sig が外部から変わったら入力欄も同期
    {
        let filter = filter;
        floem::reactive::create_effect(move |_| {
            let v = sig.get();
            if filter.get_untracked() != v {
                filter.set(v);
            }
        });
    }

    let fonts_for_list = all_fonts.clone();
    let list_view = dyn_container(
        move || (open.get(), filter.get()),
        move |(o, f)| {
            if !o {
                return container(label(|| String::new())).style(|s| s.height(0)).into_any();
            }
            let f_lc = f.to_lowercase();
            let items: Vec<String> = fonts_for_list
                .iter()
                .filter(|n| f_lc.is_empty() || n.to_lowercase().contains(&f_lc))
                .take(500)
                .cloned()
                .collect();
            let entries: Vec<floem::AnyView> = items
                .into_iter()
                .map(|name| {
                    let n_for_click = name.clone();
                    let n_for_label = name.clone();
                    label(move || {
                        if n_for_label.is_empty() {
                            String::from("(system default)")
                        } else {
                            n_for_label.clone()
                        }
                    })
                    .style(|s| {
                        s.padding_horiz(8)
                            .padding_vert(4)
                            .width_full()
                            .cursor(CursorStyle::Pointer)
                            .color(theme::text_normal())
                    })
                    .on_click_stop(move |_| {
                        sig.set(n_for_click.clone());
                        filter.set(n_for_click.clone());
                        open.set(false);
                    })
                    .into_any()
                })
                .collect();
            floem::views::scroll(
                floem::views::stack_from_iter(entries).style(|s| s.flex_col().width_full()),
            )
            .style(|s| {
                s.height(280)
                    .width(260)
                    .border(1)
                    .border_color(theme::border_default())
                    .background(theme::bg_modal())
            })
            .into_any()
        },
    );

    let toggle_btn = button("▼").action(move || {
        let cur = open.get_untracked();
        if !cur {
            filter.set(String::new());
        } else {
            filter.set(sig.get_untracked());
        }
        open.set(!cur);
    });

    let input = text_input(filter)
        .style(|s| {
            s.width(220)
                .height(24)
                .padding_horiz(6)
                .border(1)
                .border_color(theme::border_default())
                .background(theme::bg_modal())
                .color(theme::text_normal())
        })
        .on_event_stop(EventListener::FocusGained, move |_| {
            filter.set(String::new());
            open.set(true);
        })
        .on_event_stop(EventListener::FocusLost, move |_| {
            // フォーカスが外れたら表示を選択値に戻す (候補クリックは別系統で処理済み)
            filter.set(sig.get_untracked());
        });

    h_stack((
        label(move || title.to_string())
            .style(|s| s.width(220).padding(6).color(theme::text_label())),
        v_stack((h_stack((input, toggle_btn)).style(|s| s.gap(2)), list_view))
            .style(|s| s.flex_col()),
    ))
    .style(|s| s.padding(4).items_start().gap(8))
    .into_any()
}
