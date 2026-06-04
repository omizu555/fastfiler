//! テーマ (配色プリセット)。
//!
//! `th()` で現在のテーマを取得する。static ベースなので **どこからでも**
//! (hover クロージャや TextElement の paint 内からでも) 参照できる。
//! 切替は `cycle()` / `set_by_name()` → 呼び出し側で全ビューを再描画
//! (FastFilerApp::refresh_all)。選択中テーマ名はセッションに保存される。

use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui::{Rgba, rgb, rgba};

/// アプリ全体の配色パレット。
#[derive(Clone)]
pub struct Theme {
    pub name: &'static str,

    // 背景
    pub app_bg: Rgba,
    pub tab_bar_bg: Rgba,
    pub tree_bg: Rgba,
    pub pane_bg: Rgba,
    pub input_bg: Rgba,
    pub bar_bg: Rgba,
    pub header_bg: Rgba,
    pub search_bar_bg: Rgba,
    pub row_even: Rgba,
    pub row_odd: Rgba,
    pub surface_bg: Rgba,
    pub surface_hover: Rgba,
    pub button_bg: Rgba,
    pub button_hover: Rgba,
    pub separator: Rgba,
    pub handle_bg: Rgba,
    pub border_dim: Rgba,

    // 選択・インタラクション
    pub sel_bg: Rgba,
    pub sel_cursor_bg: Rgba,
    pub cursor_bg: Rgba,
    pub hover_bg: Rgba,
    pub drop_bg: Rgba,
    pub drop_row_bg: Rgba,
    pub handle_hover: Rgba,
    pub menu_hover: Rgba,
    pub danger_bg: Rgba,
    pub danger_hover: Rgba,

    // アクセント
    pub accent: Rgba,
    pub accent_file: Rgba,

    // テキスト
    pub text_bright: Rgba,
    pub text: Rgba,
    pub text_soft: Rgba,
    pub text_dim: Rgba,
    pub text_faint: Rgba,
    pub text_disabled: Rgba,

    // 半透明
    pub sel_translucent: Rgba,
    pub overlay_bg: Rgba,
}

fn dark() -> Theme {
    Theme {
        name: "ダーク",
        app_bg: rgb(0x111111),
        tab_bar_bg: rgb(0x161616),
        tree_bg: rgb(0x191919),
        pane_bg: rgb(0x1a1a1a),
        input_bg: rgb(0x141414),
        bar_bg: rgb(0x252525),
        header_bg: rgb(0x202020),
        search_bar_bg: rgb(0x1f2730),
        row_even: rgb(0x1e1e1e),
        row_odd: rgb(0x232323),
        surface_bg: rgb(0x2a2a2a),
        surface_hover: rgb(0x303030),
        button_bg: rgb(0x3a3a3a),
        button_hover: rgb(0x4a4a4a),
        separator: rgb(0x3f3f3f),
        handle_bg: rgb(0x0a0a0a),
        border_dim: rgb(0x101010),
        sel_bg: rgb(0x2d4661),
        sel_cursor_bg: rgb(0x33506e),
        cursor_bg: rgb(0x2a3340),
        hover_bg: rgb(0x33404d),
        drop_bg: rgb(0x1f2c3a),
        drop_row_bg: rgb(0x2a4a6a),
        handle_hover: rgb(0x3a6a9a),
        menu_hover: rgb(0x3a5a7a),
        danger_bg: rgb(0x553333),
        danger_hover: rgb(0x6a3a3a),
        accent: rgb(0x5aa9e6),
        accent_file: rgb(0x707070),
        text_bright: rgb(0xffffff),
        text: rgb(0xe0e0e0),
        text_soft: rgb(0xcccccc),
        text_dim: rgb(0x9a9a9a),
        text_faint: rgb(0x8a8a8a),
        text_disabled: rgb(0x666666),
        sel_translucent: rgba(0x3a6a9a66),
        overlay_bg: rgba(0x000000aa),
    }
}

fn light() -> Theme {
    Theme {
        name: "ライト",
        app_bg: rgb(0xe8e8e8),
        tab_bar_bg: rgb(0xdcdcdc),
        tree_bg: rgb(0xe4e4e4),
        pane_bg: rgb(0xfafafa),
        input_bg: rgb(0xffffff),
        bar_bg: rgb(0xe0e0e0),
        header_bg: rgb(0xd8d8d8),
        search_bar_bg: rgb(0xdde4ec),
        row_even: rgb(0xfafafa),
        row_odd: rgb(0xf0f0f0),
        surface_bg: rgb(0xeeeeee),
        surface_hover: rgb(0xdadada),
        button_bg: rgb(0xd0d0d0),
        button_hover: rgb(0xc0c0c0),
        separator: rgb(0xc8c8c8),
        handle_bg: rgb(0xb8b8b8),
        border_dim: rgb(0xdddddd),
        sel_bg: rgb(0xbcd4ee),
        sel_cursor_bg: rgb(0xa8c8ec),
        cursor_bg: rgb(0xd4e2f4),
        hover_bg: rgb(0xd8e4f0),
        drop_bg: rgb(0xcce0f4),
        drop_row_bg: rgb(0xaaccee),
        handle_hover: rgb(0x6a9aca),
        menu_hover: rgb(0xc4d8ec),
        danger_bg: rgb(0xeec4c4),
        danger_hover: rgb(0xe4a8a8),
        accent: rgb(0x1a6ac0),
        accent_file: rgb(0x909090),
        text_bright: rgb(0x000000),
        text: rgb(0x202020),
        text_soft: rgb(0x333333),
        text_dim: rgb(0x555555),
        text_faint: rgb(0x707070),
        text_disabled: rgb(0x9a9a9a),
        sel_translucent: rgba(0x1a6ac066),
        overlay_bg: rgba(0x00000055),
    }
}

fn midnight() -> Theme {
    Theme {
        name: "ミッドナイト",
        app_bg: rgb(0x0a0e1a),
        tab_bar_bg: rgb(0x0e1322),
        tree_bg: rgb(0x101628),
        pane_bg: rgb(0x121a2e),
        input_bg: rgb(0x0c1220),
        bar_bg: rgb(0x18203a),
        header_bg: rgb(0x141c34),
        search_bar_bg: rgb(0x16223e),
        row_even: rgb(0x121a2e),
        row_odd: rgb(0x161f36),
        surface_bg: rgb(0x1c2742),
        surface_hover: rgb(0x223052),
        button_bg: rgb(0x263452),
        button_hover: rgb(0x32426a),
        separator: rgb(0x2a3450),
        handle_bg: rgb(0x080c16),
        border_dim: rgb(0x0c1220),
        sel_bg: rgb(0x2a4a7a),
        sel_cursor_bg: rgb(0x35589a),
        cursor_bg: rgb(0x20335a),
        hover_bg: rgb(0x24385e),
        drop_bg: rgb(0x1c3050),
        drop_row_bg: rgb(0x2c4e80),
        handle_hover: rgb(0x4a7ab4),
        menu_hover: rgb(0x35588e),
        danger_bg: rgb(0x5a3040),
        danger_hover: rgb(0x70405a),
        accent: rgb(0x6ab4ff),
        accent_file: rgb(0x70809a),
        text_bright: rgb(0xffffff),
        text: rgb(0xdce6f4),
        text_soft: rgb(0xb8c6dc),
        text_dim: rgb(0x90a0ba),
        text_faint: rgb(0x708098),
        text_disabled: rgb(0x506078),
        sel_translucent: rgba(0x4a7ab466),
        overlay_bg: rgba(0x000000aa),
    }
}

static THEME_IX: AtomicUsize = AtomicUsize::new(0);
static PRESETS: OnceLock<Vec<Theme>> = OnceLock::new();

pub fn presets() -> &'static [Theme] {
    PRESETS.get_or_init(|| vec![dark(), light(), midnight()])
}

/// 現在のテーマ。
pub fn th() -> &'static Theme {
    let ix = THEME_IX.load(Ordering::Relaxed);
    let p = presets();
    &p[ix.min(p.len() - 1)]
}

/// 次のプリセットへ切替え、新テーマを返す。
pub fn cycle() -> &'static Theme {
    let n = presets().len();
    let ix = (THEME_IX.load(Ordering::Relaxed) + 1) % n;
    THEME_IX.store(ix, Ordering::Relaxed);
    th()
}

/// 名前でテーマを選択 (セッション復元用)。見つかれば true。
pub fn set_by_name(name: &str) -> bool {
    if let Some(ix) = presets().iter().position(|t| t.name == name) {
        THEME_IX.store(ix, Ordering::Relaxed);
        true
    } else {
        false
    }
}
