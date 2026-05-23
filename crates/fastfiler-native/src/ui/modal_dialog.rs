//! ファイル操作 (新規フォルダ/新規ファイル/リネーム) 用センターポップアップダイアログ。
//!
//! 以前はペイン上部の `modal_bar` (横バー) で表示していたが、視認性向上のため
//! アプリ中央に半透明オーバーレイ + フローティングカードを重ねる方式に変更した。
//!
//! state は `PaneState.modal_kind` / `modal_input` をそのまま流用するため、
//! 操作経路 (hotkey / コンテキストメニュー / start_* メソッド) には影響しない。
//! ルート view から全ペインの `modal_kind` を track し、最初の non-None を引き出す。

use floem::event::{Event, EventListener};
use floem::keyboard::{Key, NamedKey};
use floem::peniko::Color;
use floem::reactive::SignalGet;
use floem::style::Position;
use floem::views::{
    button, container, dyn_container, empty, h_stack, label, text_input, v_stack, Decorators,
};
use floem::IntoView;

use crate::core::state::{AppState, ModalKind};
use crate::theme;

/// アプリルートに重ねるモーダルオーバーレイ全体。
pub fn modal_dialog(app: AppState) -> impl IntoView {
    let tabs_sig = app.tabs;
    dyn_container(
        move || {
            // 全ペインの modal_kind を track して最初の non-None を取り出す。
            // (各 pane.modal_kind.get() が track され、変化時に dyn_container が再評価される)
            tabs_sig.get().iter().find_map(|t| {
                t.all_panes()
                    .into_iter()
                    .find_map(|p| match p.modal_kind.get() {
                        ModalKind::None => None,
                        kind => Some((p.id, kind)),
                    })
            })
        },
        move |maybe| match maybe {
            None => empty().into_any(),
            Some((pane_id, kind)) => render_overlay(app.clone(), pane_id, kind).into_any(),
        },
    )
    .style(|s| {
        s.position(Position::Absolute)
            .inset_left(0)
            .inset_top(0)
            .inset_right(0)
            .inset_bottom(0)
    })
}

fn render_overlay(app: AppState, pane_id: u64, kind: ModalKind) -> impl IntoView {
    // pane_id から PaneState を解決
    let Some(pane) = find_pane(&app, pane_id) else {
        return empty().into_any();
    };

    let title = match &kind {
        ModalKind::NewFolder => "新規フォルダ名",
        ModalKind::NewFile => "新規ファイル名",
        ModalKind::Rename(_) => "新しい名前",
        ModalKind::None => "",
    };

    let modal_input = pane.modal_input;
    let modal_kind_sig = pane.modal_kind;
    let pane_for_ok = pane.clone();
    let pane_for_cancel = pane.clone();
    let pane_for_enter = pane.clone();
    let pane_for_overlay = pane.clone();
    let um_ok = app.undo_manager.clone();
    let um_enter = app.undo_manager.clone();

    let input = text_input(modal_input)
        .request_focus(move || {
            // modal_kind が変化したらフォーカス要求 (None → 任意の時に発火)
            modal_kind_sig.get();
        })
        .style(|s| {
            s.width_full()
                .padding(6)
                .border(1)
                .border_color(theme::border_focus())
                .background(theme::bg_panel())
                .color(theme::text_normal())
        })
        .on_event_stop(EventListener::KeyDown, move |e| {
            if let Event::KeyDown(ke) = e {
                match &ke.key.logical_key {
                    Key::Named(NamedKey::Enter) => pane_for_enter.confirm_modal(&um_enter),
                    Key::Named(NamedKey::Escape) => pane_for_enter.close_modal(),
                    _ => {}
                }
            }
        });

    let buttons = h_stack((
        button("OK").action(move || pane_for_ok.confirm_modal(&um_ok)),
        button("Cancel").action(move || pane_for_cancel.close_modal()),
    ))
    .style(|s| s.gap(8).justify_end().width_full());

    let card = v_stack((
        label(move || title.to_string())
            .style(|s| s.font_bold().color(theme::text_normal()).margin_bottom(4)),
        input,
        buttons,
    ))
    .style(|s| {
        s.gap(10)
            .padding(16)
            .width(380)
            .background(theme::bg_modal())
            .border(1)
            .border_color(theme::border_strong())
            .border_radius(6)
    })
    // カード上のクリックがオーバーレイに伝播して close されないようガード
    .on_click_stop(|_| {});

    // 中央配置用ラッパ (justify_center + items_center)
    let centered = container(card).style(|s| {
        s.width_full()
            .height_full()
            .justify_center()
            .items_center()
            // 半透明黒オーバーレイ
            .background(Color::rgba8(0, 0, 0, 96))
    });

    // 背景クリックで close (ただしカード内クリックは伝播停止しないと閉じてしまう)
    centered
        .on_click_stop(move |_| {
            pane_for_overlay.close_modal();
        })
        .into_any()
}

fn find_pane(app: &AppState, pane_id: u64) -> Option<crate::core::state::PaneState> {
    app.tabs
        .get_untracked()
        .iter()
        .flat_map(|t| t.all_panes())
        .find(|p| p.id == pane_id)
}
