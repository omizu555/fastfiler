//! FastFiler (GPUI) エントリポイント。
//!
//! 詳細は doc/plan-2026-06-03-gpui-migration.md。

// リリースビルドではコンソールウィンドウを出さない (GUI アプリ)。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod pane;
mod session;
mod sink;
mod text_input;
mod tree;
#[cfg(windows)]
mod win32_single_instance;

use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, px, size,
};
use gpui_platform::application;

use crate::app::{FastFilerApp, default_start};

fn main() {
    // 多重起動防止: 既に起動中なら既存ウィンドウを前面化して静かに終了。
    #[cfg(windows)]
    {
        if !win32_single_instance::acquire_single_instance() {
            win32_single_instance::activate_existing_window();
            return;
        }
    }

    application().run(|cx: &mut App| {
        // テキスト入力 ("TextInput" コンテキスト限定) のキーバインドを登録。
        text_input::bind_keys(cx);

        // 前回セッション (タブ / 分割構成 / ウィンドウ位置) があれば復元。
        let saved = session::load();

        let bounds = saved
            .as_ref()
            .and_then(|s| s.window)
            .filter(|[_, _, w, h]| *w >= 400.0 && *h >= 300.0)
            .map(|[x, y, w, h]| Bounds {
                origin: point(px(x), px(y)),
                size: size(px(w), px(h)),
            })
            .unwrap_or_else(|| Bounds::centered(None, size(px(1000.0), px(660.0)), cx));
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                // タイトルは多重起動防止の FindWindowW ("FastFiler") とも対応。
                titlebar: Some(TitlebarOptions {
                    title: Some("FastFiler".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_window, cx| {
                cx.new(|cx| match saved {
                    Some(data) => FastFilerApp::from_session(data, cx),
                    None => FastFilerApp::new(default_start(), cx),
                })
            },
        )
        .expect("ウィンドウ生成に失敗");
        cx.activate(true);
    });
}
