//! テーマ (配色)。GPUI 版 theme.rs の 37 色キー + themes/*.json 互換 (N-05)。
//!
//! - プリセット 3 種 (ダーク/ライト/ミッドナイト) は GPUI 版と同一の色値
//! - ユーザーテーマ: `%APPDATA%\FastFiler\themes\*.json`
//!   (`{ "name": ..., "base": プリセット名, "colors": { キー: "#rrggbb(aa)" } }`)
//! - iced への反映は `to_iced()` で `iced::Theme::custom` に射影する。
//!   iced の extended_palette が二次色を自動生成するため、37 キーのうち
//!   主要色 (背景/文字/アクセント/危険/成功系) が palette を駆動する。
//!   細部キーも保持しており、必要な widget は `current()` から直接読める。
//! - GPUI 版の Box::leak 方式と違い、再読込しても leak しない (Vec 差し替え)。

use std::path::PathBuf;
use std::sync::RwLock;

use iced::Color;
use once_cell::sync::Lazy;
use serde::Deserialize;

fn c(rgb: u32) -> Color {
    Color::from_rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

fn ca(rgba: u32) -> Color {
    Color::from_rgba8(
        (rgba >> 24) as u8,
        (rgba >> 16) as u8,
        (rgba >> 8) as u8,
        (rgba & 0xff) as f32 / 255.0,
    )
}

/// 色フィールドの一覧 (単一の情報源 — GPUI 版と同じマクロ方式)。
macro_rules! theme_colors {
    ($($field:ident),+ $(,)?) => {
        /// アプリ全体の配色パレット (37 色キー)。
        #[derive(Clone, Debug, PartialEq)]
        pub struct Theme {
            pub name: String,
            $(pub $field: Color,)+
        }

        /// テーマ JSON の色上書き (全フィールド任意、hex 文字列)。
        #[derive(Deserialize, Default)]
        pub struct ThemeColors {
            $(
                #[serde(default)]
                pub $field: Option<String>,
            )+
        }

        impl Theme {
            /// 自分をベースに JSON で指定された色だけ上書きした新しいテーマ。
            fn with_overrides(&self, name: String, o: &ThemeColors) -> Theme {
                Theme {
                    name,
                    $($field: o.$field.as_deref().and_then(parse_hex).unwrap_or(self.$field),)+
                }
            }
        }
    };
}

theme_colors! {
    // 背景
    app_bg, tab_bar_bg, tree_bg, pane_bg, input_bg, bar_bg, header_bg,
    search_bar_bg, row_even, row_odd, surface_bg, surface_hover,
    button_bg, button_hover, separator, handle_bg, border_dim,
    // 選択・インタラクション
    sel_bg, sel_cursor_bg, cursor_bg, hover_bg, drop_bg, drop_row_bg,
    handle_hover, menu_hover, danger_bg, danger_hover,
    // アクセント
    accent, accent_file,
    // テキスト
    text_bright, text, text_soft, text_dim, text_faint, text_disabled,
    // 半透明
    sel_translucent, overlay_bg,
}

/// `#rrggbb` / `#rrggbbaa` をパースする (GPUI 版テーマ JSON と同じ表記)。
fn parse_hex(s: &str) -> Option<Color> {
    let h = s.strip_prefix('#')?;
    match h.len() {
        6 => u32::from_str_radix(h, 16).ok().map(c),
        8 => u32::from_str_radix(h, 16).ok().map(ca),
        _ => None,
    }
}

fn dark() -> Theme {
    Theme {
        name: "ダーク".into(),
        app_bg: c(0x111111),
        tab_bar_bg: c(0x161616),
        tree_bg: c(0x191919),
        pane_bg: c(0x1a1a1a),
        input_bg: c(0x141414),
        bar_bg: c(0x252525),
        header_bg: c(0x202020),
        search_bar_bg: c(0x1f2730),
        row_even: c(0x1e1e1e),
        row_odd: c(0x232323),
        surface_bg: c(0x2a2a2a),
        surface_hover: c(0x303030),
        button_bg: c(0x3a3a3a),
        button_hover: c(0x4a4a4a),
        separator: c(0x3f3f3f),
        handle_bg: c(0x0a0a0a),
        border_dim: c(0x101010),
        sel_bg: c(0x2d4661),
        sel_cursor_bg: c(0x33506e),
        cursor_bg: c(0x2a3340),
        hover_bg: c(0x33404d),
        drop_bg: c(0x1f2c3a),
        drop_row_bg: c(0x2a4a6a),
        handle_hover: c(0x3a6a9a),
        menu_hover: c(0x3a5a7a),
        danger_bg: c(0x553333),
        danger_hover: c(0x6a3a3a),
        accent: c(0x5aa9e6),
        accent_file: c(0x707070),
        text_bright: c(0xffffff),
        text: c(0xe0e0e0),
        text_soft: c(0xcccccc),
        text_dim: c(0x9a9a9a),
        text_faint: c(0x8a8a8a),
        text_disabled: c(0x666666),
        sel_translucent: ca(0x3a6a9a66),
        overlay_bg: ca(0x000000aa),
    }
}

fn light() -> Theme {
    Theme {
        name: "ライト".into(),
        app_bg: c(0xe8e8e8),
        tab_bar_bg: c(0xdcdcdc),
        tree_bg: c(0xe4e4e4),
        pane_bg: c(0xfafafa),
        input_bg: c(0xffffff),
        bar_bg: c(0xe0e0e0),
        header_bg: c(0xd8d8d8),
        search_bar_bg: c(0xdde4ec),
        row_even: c(0xfafafa),
        row_odd: c(0xf0f0f0),
        surface_bg: c(0xeeeeee),
        surface_hover: c(0xdadada),
        button_bg: c(0xd0d0d0),
        button_hover: c(0xc0c0c0),
        separator: c(0xc8c8c8),
        handle_bg: c(0xb8b8b8),
        border_dim: c(0xdddddd),
        sel_bg: c(0xbcd4ee),
        sel_cursor_bg: c(0xa8c8ec),
        cursor_bg: c(0xd4e2f4),
        hover_bg: c(0xd8e4f0),
        drop_bg: c(0xcce0f4),
        drop_row_bg: c(0xaaccee),
        handle_hover: c(0x6a9aca),
        menu_hover: c(0xc4d8ec),
        danger_bg: c(0xeec4c4),
        danger_hover: c(0xe4a8a8),
        accent: c(0x1a6ac0),
        accent_file: c(0x909090),
        text_bright: c(0x000000),
        text: c(0x202020),
        text_soft: c(0x333333),
        text_dim: c(0x555555),
        text_faint: c(0x707070),
        text_disabled: c(0x9a9a9a),
        sel_translucent: ca(0x1a6ac066),
        overlay_bg: ca(0x00000055),
    }
}

fn midnight() -> Theme {
    Theme {
        name: "ミッドナイト".into(),
        app_bg: c(0x0a0e1a),
        tab_bar_bg: c(0x0e1322),
        tree_bg: c(0x101628),
        pane_bg: c(0x121a2e),
        input_bg: c(0x0c1220),
        bar_bg: c(0x18203a),
        header_bg: c(0x141c34),
        search_bar_bg: c(0x16223e),
        row_even: c(0x121a2e),
        row_odd: c(0x161f36),
        surface_bg: c(0x1c2742),
        surface_hover: c(0x223052),
        button_bg: c(0x263452),
        button_hover: c(0x32426a),
        separator: c(0x2a3450),
        handle_bg: c(0x080c16),
        border_dim: c(0x0c1220),
        sel_bg: c(0x2a4a7a),
        sel_cursor_bg: c(0x35589a),
        cursor_bg: c(0x20335a),
        hover_bg: c(0x24385e),
        drop_bg: c(0x1c3050),
        drop_row_bg: c(0x2c4e80),
        handle_hover: c(0x4a7ab4),
        menu_hover: c(0x35588e),
        danger_bg: c(0x5a3040),
        danger_hover: c(0x70405a),
        accent: c(0x6ab4ff),
        accent_file: c(0x70809a),
        text_bright: c(0xffffff),
        text: c(0xdce6f4),
        text_soft: c(0xb8c6dc),
        text_dim: c(0x90a0ba),
        text_faint: c(0x708098),
        text_disabled: c(0x506078),
        sel_translucent: ca(0x4a7ab466),
        overlay_bg: ca(0x000000aa),
    }
}

/// ユーザーテーマ JSON (GPUI 版と同一形式)。
#[derive(Deserialize)]
struct ThemeFile {
    name: String,
    #[serde(default)]
    base: Option<String>,
    #[serde(default)]
    colors: ThemeColors,
}

/// 全テーマ (プリセット 3 種 + ユーザーテーマ)。再読込で丸ごと差し替える
/// (GPUI 版の Box::leak と違い leak しない — 計画書 §10 の改善点)。
static THEMES: Lazy<RwLock<Vec<Theme>>> = Lazy::new(|| RwLock::new(build_themes()));

fn presets() -> [Theme; 3] {
    [dark(), light(), midnight()]
}

fn themes_dir() -> Option<PathBuf> {
    let base = std::env::var("APPDATA").ok()?;
    Some(PathBuf::from(base).join("FastFiler").join("themes"))
}

fn build_themes() -> Vec<Theme> {
    let mut out: Vec<Theme> = presets().to_vec();
    let Some(dir) = themes_dir() else { return out };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return out;
    };
    let mut files: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        })
        .collect();
    files.sort();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        let Ok(tf) = serde_json::from_str::<ThemeFile>(&text) else {
            continue;
        };
        // プリセットと同名は無視 (GPUI 版と同じ)
        if out.iter().any(|t| t.name == tf.name) {
            continue;
        }
        let base = tf
            .base
            .as_deref()
            .and_then(|b| presets().iter().find(|t| t.name == b).cloned())
            .unwrap_or_else(dark);
        out.push(base.with_overrides(tf.name, &tf.colors));
    }
    out
}

/// themes/*.json を再読込する。戻り値: 読み込んだユーザーテーマ数。
pub fn reload_user_themes() -> usize {
    let themes = build_themes();
    let n = themes.len() - 3;
    if let Ok(mut g) = THEMES.write() {
        *g = themes;
    }
    n
}

/// テーマ名の一覧 (設定画面の選択肢)。
pub fn theme_names() -> Vec<String> {
    THEMES
        .read()
        .map(|g| g.iter().map(|t| t.name.clone()).collect())
        .unwrap_or_default()
}

/// 名前でテーマを取得 (無ければ既定 = ダーク)。
pub fn by_name(name: Option<&str>) -> Theme {
    let g = THEMES.read().ok();
    g.as_ref()
        .and_then(|v| {
            name.and_then(|n| v.iter().find(|t| t.name == n))
                .or_else(|| v.first())
        })
        .cloned()
        .unwrap_or_else(dark)
}

impl Theme {
    /// iced のテーマへ射影する。extended_palette の二次色 (weak/strong) は
    /// iced が自動生成する。細部キーが必要な widget は Theme を直接使う。
    pub fn to_iced(&self) -> iced::Theme {
        iced::Theme::custom(
            self.name.clone(),
            iced::theme::Palette {
                background: self.pane_bg,
                text: self.text,
                primary: self.accent,
                success: c(0x3a9a5a),
                warning: c(0xd0a030),
                danger: self.danger_bg,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parse_and_overrides() {
        assert_eq!(parse_hex("#ff0000"), Some(Color::from_rgb8(255, 0, 0)));
        assert!(parse_hex("#ff0000ff").is_some());
        assert_eq!(parse_hex("red"), None);
        let o = ThemeColors {
            accent: Some("#123456".into()),
            ..Default::default()
        };
        let t = dark().with_overrides("カスタム".into(), &o);
        assert_eq!(t.accent, c(0x123456));
        assert_eq!(t.app_bg, dark().app_bg); // 未指定はベースの色
    }

    #[test]
    fn presets_available_by_name() {
        assert_eq!(by_name(Some("ライト")).name, "ライト");
        assert_eq!(by_name(Some("存在しない")).name, "ダーク"); // フォールバック
        assert_eq!(by_name(None).name, "ダーク");
        assert!(theme_names().len() >= 3);
    }
}
