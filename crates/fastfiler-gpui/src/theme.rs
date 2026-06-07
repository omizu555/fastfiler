//! テーマ (配色) / スタイル (形状) / UI フォントサイズ。
//!
//! `th()` で現在のテーマを取得する。static ベースなので **どこからでも**
//! (hover クロージャや TextElement の paint 内からでも) 参照できる。
//! 切替は `set_by_name()` → 呼び出し側で全ビューを再描画
//! (FastFilerApp::refresh_all)。選択中テーマ名は設定 (gpui_settings.json) に保存される。
//!
//! - **テーマ** = 色。組み込みプリセット 3 種 + `%APPDATA%\FastFiler\themes\*.json`
//!   のユーザーテーマ (`load_user_themes()`)。JSON は `base` のプリセットに
//!   `colors` の hex 色 (`"#rrggbb"` / `"#rrggbbaa"`) を上書きする形式。
//! - **スタイル** = 形 (角丸の強さ)。`ui_style()` / `radius_md()` / `radius_sm()`。
//! - フォントサイズは `font_px()` (設定値のキャッシュ)。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering};
use std::sync::{OnceLock, RwLock};

use gpui::{Pixels, Rgba, px, rgb, rgba};
use serde::{Deserialize, Serialize};

/// 色フィールドの一覧 (単一の情報源)。`Theme` / `ThemeColors` (JSON 上書き用) /
/// マージ / 全色書き出し をまとめて生成する。フィールドを増やすときはここに 1 行
/// 足すだけでよい (プリセット定義のリテラルにも追加が必要なのはコンパイラが教える)。
macro_rules! theme_colors {
    ($($field:ident),+ $(,)?) => {
        /// アプリ全体の配色パレット。
        #[derive(Clone)]
        pub struct Theme {
            pub name: &'static str,
            $(pub $field: Rgba,)+
        }

        /// テーマ JSON の色上書き (全フィールド任意、hex 文字列)。
        #[derive(Serialize, Deserialize, Default)]
        pub struct ThemeColors {
            $(
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub $field: Option<Rgba>,
            )+
        }

        impl Theme {
            /// 自分をベースに `c` で指定された色だけ上書きした新しいテーマ。
            fn with_overrides(&self, name: &'static str, c: &ThemeColors) -> Theme {
                Theme {
                    name,
                    $($field: c.$field.unwrap_or(self.$field),)+
                }
            }

            /// 全色を Some で書き出す (サンプル JSON 生成用)。
            fn to_colors(&self) -> ThemeColors {
                ThemeColors {
                    $($field: Some(self.$field),)+
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

static PRESETS: OnceLock<Vec<Theme>> = OnceLock::new();

pub fn presets() -> &'static [Theme] {
    PRESETS.get_or_init(|| vec![dark(), light(), midnight()])
}

// ---- 現在テーマ ----
//
// `th()` は描画中に大量に呼ばれるため lock-free を維持する。指す先は
// presets() の要素か Box::leak したユーザーテーマのみ ('static 保証)。

static CURRENT: AtomicPtr<Theme> = AtomicPtr::new(std::ptr::null_mut());

/// 現在のテーマ。
pub fn th() -> &'static Theme {
    let p = CURRENT.load(Ordering::Relaxed);
    if p.is_null() {
        &presets()[0]
    } else {
        // SAFETY: CURRENT には presets() の要素か Box::leak した Theme しか入らない。
        unsafe { &*p }
    }
}

/// 名前でテーマを選択 (設定画面 / 起動時復元用)。プリセット → ユーザーテーマの順に
/// 検索する (名前が重複した場合はプリセット優先)。見つかれば true。
pub fn set_by_name(name: &str) -> bool {
    let found: Option<*const Theme> = presets()
        .iter()
        .find(|t| t.name == name)
        .map(|t| t as *const Theme)
        .or_else(|| {
            user_themes()
                .read()
                .unwrap()
                .iter()
                .find(|t| t.name == name)
                .map(|t| *t as *const Theme)
        });
    if let Some(p) = found {
        CURRENT.store(p as *mut Theme, Ordering::Relaxed);
        true
    } else {
        false
    }
}

/// プリセット + ユーザーテーマの一覧 (設定画面のコンボ用)。
pub fn all_themes() -> Vec<&'static Theme> {
    let mut v: Vec<&'static Theme> = presets().iter().collect();
    v.extend(user_themes().read().unwrap().iter().copied());
    v
}

// ---- ユーザーテーマ (JSON ファイル) ----

/// テーマ JSON ファイルの形式。
#[derive(Serialize, Deserialize)]
struct ThemeFile {
    /// テーマ名 (コンボに表示)。
    name: String,
    /// ベースにするプリセット名 (省略時「ダーク」)。
    #[serde(default)]
    base: Option<String>,
    /// 上書きする色 (書いたものだけ反映)。
    #[serde(default)]
    colors: ThemeColors,
}

fn user_themes() -> &'static RwLock<Vec<&'static Theme>> {
    static STORE: OnceLock<RwLock<Vec<&'static Theme>>> = OnceLock::new();
    STORE.get_or_init(|| RwLock::new(Vec::new()))
}

/// ユーザーテーマの置き場所 (`%APPDATA%\FastFiler\themes`)。
pub fn themes_dir() -> Option<PathBuf> {
    let base = std::env::var("APPDATA").ok()?;
    Some(PathBuf::from(base).join("FastFiler").join("themes"))
}

/// themes フォルダの `*.json` を読み込んで差し替える。フォルダが無ければ作成して
/// サンプルを生成する。読み込み後、現在テーマを名前で引き直す (JSON 編集の反映 /
/// 削除時はプリセットへフォールバック)。戻り値: (読込数, エラーのファイル名)。
pub fn load_user_themes() -> (usize, Vec<String>) {
    let Some(dir) = themes_dir() else {
        return (0, Vec::new());
    };
    if !dir.is_dir() {
        let _ = std::fs::create_dir_all(&dir);
        write_samples(&dir);
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort();

    let mut loaded: Vec<&'static Theme> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    for path in files {
        let fname = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str::<ThemeFile>(&t).map_err(|e| e.to_string()))
        {
            Ok(tf) => {
                // プリセットと同名はプリセットが勝つため読み込まない (混乱防止)。
                if presets().iter().any(|t| t.name == tf.name) {
                    errors.push(format!("{fname} (プリセットと名前重複)"));
                    continue;
                }
                let base = tf
                    .base
                    .as_deref()
                    .and_then(|b| presets().iter().find(|t| t.name == b))
                    .unwrap_or(&presets()[0]);
                let name: &'static str = Box::leak(tf.name.into_boxed_str());
                // 再読み込みのたびに旧テーマが微小リークするが、サイズ・頻度ともに許容。
                let theme: &'static Theme = Box::leak(Box::new(base.with_overrides(name, &tf.colors)));
                loaded.push(theme);
            }
            Err(_) => errors.push(fname),
        }
    }
    let n = loaded.len();
    let current_name = th().name.to_string();
    *user_themes().write().unwrap() = loaded;
    if !set_by_name(&current_name) {
        // 現在テーマが消えた → 既定 (先頭プリセット) へ。
        CURRENT.store(std::ptr::null_mut(), Ordering::Relaxed);
    }
    (n, errors)
}

/// サンプルテーマを書き出す (themes フォルダ新規作成時のみ呼ばれる)。
/// ユーザーが削除したファイルは復活しない。
fn write_samples(dir: &Path) {
    // カスタム例はダークの全色入り — どのキーが弄れるか一覧になる。
    let samples: Vec<(&str, ThemeFile)> = vec![
        (
            "カスタム例.json",
            ThemeFile {
                name: "カスタム例".into(),
                base: Some("ダーク".into()),
                colors: dark().to_colors(),
            },
        ),
        (
            "ノルド風.json",
            ThemeFile {
                name: "ノルド風".into(),
                base: Some("ダーク".into()),
                colors: sample_nord().to_colors(),
            },
        ),
        (
            "ハイコントラスト.json",
            ThemeFile {
                name: "ハイコントラスト".into(),
                base: Some("ダーク".into()),
                colors: sample_high_contrast().to_colors(),
            },
        ),
        (
            "セピア.json",
            ThemeFile {
                name: "セピア".into(),
                base: Some("ライト".into()),
                colors: sample_sepia().to_colors(),
            },
        ),
    ];
    for (fname, tf) in samples {
        if let Ok(text) = serde_json::to_string_pretty(&tf) {
            let _ = std::fs::write(dir.join(fname), text);
        }
    }
}

/// サンプル: Nord 系の青灰ダーク。
fn sample_nord() -> Theme {
    Theme {
        name: "ノルド風",
        app_bg: rgb(0x2e3440),
        tab_bar_bg: rgb(0x292e39),
        tree_bg: rgb(0x2e3440),
        pane_bg: rgb(0x3b4252),
        input_bg: rgb(0x2e3440),
        bar_bg: rgb(0x434c5e),
        header_bg: rgb(0x3b4252),
        search_bar_bg: rgb(0x3b4a62),
        row_even: rgb(0x3b4252),
        row_odd: rgb(0x404a5c),
        surface_bg: rgb(0x434c5e),
        surface_hover: rgb(0x4c566a),
        button_bg: rgb(0x4c566a),
        button_hover: rgb(0x5b6678),
        separator: rgb(0x4c566a),
        handle_bg: rgb(0x232831),
        border_dim: rgb(0x262b35),
        sel_bg: rgb(0x3f5577),
        sel_cursor_bg: rgb(0x46608a),
        cursor_bg: rgb(0x3b4d6b),
        hover_bg: rgb(0x404f66),
        drop_bg: rgb(0x35455e),
        drop_row_bg: rgb(0x4a6280),
        handle_hover: rgb(0x5e81ac),
        menu_hover: rgb(0x4c6a92),
        danger_bg: rgb(0x5e3a42),
        danger_hover: rgb(0x6e444e),
        accent: rgb(0x88c0d0),
        accent_file: rgb(0x7a869a),
        text_bright: rgb(0xeceff4),
        text: rgb(0xd8dee9),
        text_soft: rgb(0xc8d0e0),
        text_dim: rgb(0x9aa5b8),
        text_faint: rgb(0x848fa3),
        text_disabled: rgb(0x5c6678),
        sel_translucent: rgba(0x5e81ac66),
        overlay_bg: rgba(0x000000aa),
    }
}

/// サンプル: 黒背景 + 高コントラスト。
fn sample_high_contrast() -> Theme {
    Theme {
        name: "ハイコントラスト",
        app_bg: rgb(0x000000),
        tab_bar_bg: rgb(0x000000),
        tree_bg: rgb(0x000000),
        pane_bg: rgb(0x000000),
        input_bg: rgb(0x000000),
        bar_bg: rgb(0x101010),
        header_bg: rgb(0x101010),
        search_bar_bg: rgb(0x001030),
        row_even: rgb(0x000000),
        row_odd: rgb(0x0c0c0c),
        surface_bg: rgb(0x101010),
        surface_hover: rgb(0x202020),
        button_bg: rgb(0x202020),
        button_hover: rgb(0x353535),
        separator: rgb(0x606060),
        handle_bg: rgb(0x303030),
        border_dim: rgb(0x404040),
        sel_bg: rgb(0x0050d0),
        sel_cursor_bg: rgb(0x0060ff),
        cursor_bg: rgb(0x003080),
        hover_bg: rgb(0x003080),
        drop_bg: rgb(0x002860),
        drop_row_bg: rgb(0x0050d0),
        handle_hover: rgb(0x40a0ff),
        menu_hover: rgb(0x0050d0),
        danger_bg: rgb(0xa00000),
        danger_hover: rgb(0xc00000),
        accent: rgb(0x40c0ff),
        accent_file: rgb(0xc0c0c0),
        text_bright: rgb(0xffffff),
        text: rgb(0xffffff),
        text_soft: rgb(0xf0f0f0),
        text_dim: rgb(0xd0d0d0),
        text_faint: rgb(0xb0b0b0),
        text_disabled: rgb(0x707070),
        sel_translucent: rgba(0x4090ff80),
        overlay_bg: rgba(0x000000cc),
    }
}

/// サンプル: 暖色ライト (セピア)。
fn sample_sepia() -> Theme {
    Theme {
        name: "セピア",
        app_bg: rgb(0xece0cf),
        tab_bar_bg: rgb(0xe2d4c0),
        tree_bg: rgb(0xe8dcc9),
        pane_bg: rgb(0xf8f1e3),
        input_bg: rgb(0xfdf8ec),
        bar_bg: rgb(0xe4d7c2),
        header_bg: rgb(0xddcfb8),
        search_bar_bg: rgb(0xe8dcc0),
        row_even: rgb(0xf8f1e3),
        row_odd: rgb(0xf0e7d5),
        surface_bg: rgb(0xf0e7d5),
        surface_hover: rgb(0xe2d4be),
        button_bg: rgb(0xd8c8ae),
        button_hover: rgb(0xcab698),
        separator: rgb(0xc8b89e),
        handle_bg: rgb(0xb8a888),
        border_dim: rgb(0xddd0bb),
        sel_bg: rgb(0xe0c89e),
        sel_cursor_bg: rgb(0xd8bc8a),
        cursor_bg: rgb(0xecdcbb),
        hover_bg: rgb(0xecddbe),
        drop_bg: rgb(0xe6d4ae),
        drop_row_bg: rgb(0xd8bc8a),
        handle_hover: rgb(0xb08c50),
        menu_hover: rgb(0xe4cfa4),
        danger_bg: rgb(0xe8c0b0),
        danger_hover: rgb(0xdca890),
        accent: rgb(0x9a6a28),
        accent_file: rgb(0x98876a),
        text_bright: rgb(0x2a1f10),
        text: rgb(0x3c2e1a),
        text_soft: rgb(0x4c3d26),
        text_dim: rgb(0x6e5d42),
        text_faint: rgb(0x837154),
        text_disabled: rgb(0xa89878),
        sel_translucent: rgba(0xb08c5066),
        overlay_bg: rgba(0x00000055),
    }
}

// ---- スタイル (形状プリセット) ----
//
// テーマ (色) とは独立した「形」の選択。現状は角丸の強さのみ。
// フォントサイズには連動させない (形は文字サイズと直交)。

/// UI スタイル (形状プリセット)。
#[derive(Clone)]
pub struct UiStyle {
    pub name: &'static str,
    /// ボタン / パネル / ダイアログの角丸 (px)。
    pub radius_md: f32,
    /// メニュー項目 / 行ハイライトの角丸 (px)。
    pub radius_sm: f32,
}

static STYLE_IX: AtomicUsize = AtomicUsize::new(0);
static STYLES: OnceLock<Vec<UiStyle>> = OnceLock::new();

pub fn style_presets() -> &'static [UiStyle] {
    STYLES.get_or_init(|| {
        vec![
            // モダン = 従来の見た目 (rounded_md=6px / rounded_sm=4px 相当)。
            UiStyle {
                name: "モダン",
                radius_md: 6.0,
                radius_sm: 4.0,
            },
            UiStyle {
                name: "シャープ",
                radius_md: 0.0,
                radius_sm: 0.0,
            },
            UiStyle {
                name: "ソフト",
                radius_md: 10.0,
                radius_sm: 6.0,
            },
        ]
    })
}

/// 現在のスタイル。
pub fn ui_style() -> &'static UiStyle {
    let ix = STYLE_IX.load(Ordering::Relaxed);
    let s = style_presets();
    &s[ix.min(s.len() - 1)]
}

/// 名前でスタイルを選択 (設定画面 / 起動時復元用)。見つかれば true。
pub fn set_style_by_name(name: &str) -> bool {
    if let Some(ix) = style_presets().iter().position(|s| s.name == name) {
        STYLE_IX.store(ix, Ordering::Relaxed);
        true
    } else {
        false
    }
}

/// ボタン / パネル / ダイアログの角丸 (旧 rounded_md 相当)。
pub fn radius_md() -> Pixels {
    px(ui_style().radius_md)
}

/// メニュー項目 / 行ハイライトの角丸 (旧 rounded_sm 相当)。
pub fn radius_sm() -> Pixels {
    px(ui_style().radius_sm)
}

// ---- UI フォントサイズ (設定値のキャッシュ) ----
//
// 描画ごとに大量参照されるため settings_store の RwLock ではなく
// AtomicU32 (f32 ビット) に持つ。起動時と設定変更時に `set_font_px()` で更新し、
// 呼び出し側で全ビューを再描画する (テーマ切替と同じ流れ)。

/// フォントサイズの可変範囲 (px)。
pub const FONT_MIN: f32 = 10.0;
pub const FONT_MAX: f32 = 24.0;

static FONT_PX: AtomicU32 = AtomicU32::new(16.0f32.to_bits());

/// 現在の UI フォントサイズ (px)。
pub fn font_px() -> f32 {
    f32::from_bits(FONT_PX.load(Ordering::Relaxed))
}

/// UI フォントサイズを設定 (起動時の復元 / 設定画面から)。範囲外はクランプ。
pub fn set_font_px(size: f32) {
    let v = if size.is_finite() {
        size.clamp(FONT_MIN, FONT_MAX)
    } else {
        16.0
    };
    FONT_PX.store(v.to_bits(), Ordering::Relaxed);
}

/// 一覧の行高 (既定 16px フォントで従来の 24px)。
pub fn row_h() -> Pixels {
    px(font_px() + 8.0)
}

/// 列見出しの行高 (既定で従来の 22px)。
pub fn header_h() -> Pixels {
    px(font_px() + 6.0)
}

/// 検索バー / フッターなどのバー高 (既定で従来の 34px)。
pub fn bar_h() -> Pixels {
    px(font_px() + 18.0)
}
