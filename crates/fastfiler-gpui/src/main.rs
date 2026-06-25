//! FastFiler (GPUI) エントリポイント。
//!
//! 詳細は doc/plan-2026-06-03-gpui-migration.md。

// リリースビルドではコンソールウィンドウを出さない (GUI アプリ)。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod hotkeys;
mod pane;
mod persist;
mod session;
mod settings_store;
mod sink;
mod text_input;
mod theme;
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

    // ホットキー設定を読み込み (無ければ既定値で生成)。
    hotkeys::load();

    // OLE D&D 送信の初期化 (UI スレッド)。gpui 側で初期化済みでも参照カウントで安全。
    #[cfg(windows)]
    fastfiler_domain::ole_dnd::init_ole();

    application().run(|cx: &mut App| {
        // テキスト入力 ("TextInput" コンテキスト限定) のキーバインドを登録。
        text_input::bind_keys(cx);

        // 前回セッション (タブ / 分割構成 / ウィンドウ位置) があれば復元。
        let saved = session::load();

        // 設定 (テーマ等)。旧バージョンの session.theme からの移行も吸収する。
        let settings = settings_store::load();
        // ユーザーテーマ (themes/*.json) を先に読み込む — 保存済みテーマ名が
        // ユーザーテーマでも set_by_name で解決できるように。
        theme::load_user_themes();
        let theme_name = settings
            .theme
            .clone()
            .or_else(|| saved.as_ref().and_then(|s| s.theme.clone()));
        if let Some(name) = theme_name {
            if theme::set_by_name(&name) && settings.theme.is_none() {
                // 旧 session 由来 → 設定ファイルへ移行保存。
                settings_store::update(|s| s.theme = Some(name.clone()));
            }
        }
        // UI フォントサイズ / スタイルの復元 (キャッシュへ反映)。
        theme::set_font_px(settings.font_size);
        if let Some(style) = &settings.style {
            theme::set_style_by_name(style);
        }

        let bounds = saved
            .as_ref()
            .and_then(|s| s.window)
            .filter(|[_, _, w, h]| *w >= 400.0 && *h >= 300.0)
            .map(|[x, y, w, h]| Bounds {
                origin: point(px(x), px(y)),
                size: size(px(w), px(h)),
            })
            .unwrap_or_else(|| Bounds::centered(None, size(px(1000.0), px(660.0)), cx));
        // 最大化で終了していたら最大化で復元 (bounds は restore 時の位置として使われる)。
        let window_bounds = if saved.as_ref().map(|s| s.maximized).unwrap_or(false) {
            WindowBounds::Maximized(bounds)
        } else {
            WindowBounds::Windowed(bounds)
        };
        cx.open_window(
            WindowOptions {
                window_bounds: Some(window_bounds),
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
