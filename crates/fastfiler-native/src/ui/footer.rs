// Footer bar — ステータス表示 + 設定/ヘルプボタン (現状非表示)

use floem::prelude::*;
use floem::reactive::{SignalGet, SignalUpdate};
use floem::style::CursorStyle;
use floem::views::{h_stack, label, Decorators};

use crate::state::AppState;
use crate::theme;
pub fn footer_bar(app: AppState) -> impl IntoView {
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
    .style(|s| s.flex_grow(1.0).padding_horiz(8).color(theme::text_dim()));

    let gear = label(|| String::from("⚙ Settings"))
        .style(|s| {
            s.height(22)
                .padding_horiz(10)
                .items_center()
                .cursor(CursorStyle::Pointer)
                .color(theme::text_normal())
                .border_left(1)
                .border_color(theme::border_default())
        })
        .on_click_stop(move |_| settings_open.set(true));

    h_stack((status, gear)).style(|s| {
        s.height(26)
            .width_full()
            .items_center()
            .background(theme::bg_chrome())
            .border_top(1)
            .border_color(theme::border_default())
    })
}


