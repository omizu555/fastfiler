// テーマカラー定義
//
// `MODE_VAL` (AtomicU8) でアプリ全体の Light/Dark を切替。
// 設定変更時は次回起動から反映 (再起動不要にすると全 view 再構築が必要なため)。
// アクセントカラー (`ACCENT`) は selection 色等に使用。

use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

use floem::peniko::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Light = 0,
    Dark = 1,
}

static MODE_VAL: AtomicU8 = AtomicU8::new(Mode::Light as u8);
/// アクセントカラー (RGB を 0x00RRGGBB に詰める。0xFF000000 ビットが立っていれば「未設定」)
static ACCENT: AtomicU32 = AtomicU32::new(0xFF00_0000);

pub fn set_mode_from_str(s: &str) {
    let m = match s {
        "dark" => Mode::Dark,
        // "light" / "system" / その他は Light にフォールバック
        _ => Mode::Light,
    };
    MODE_VAL.store(m as u8, Ordering::Relaxed);
}

pub fn set_accent_from_str(s: &str) {
    let s = s.trim().trim_start_matches('#');
    if s.len() == 6 {
        if let Ok(v) = u32::from_str_radix(s, 16) {
            ACCENT.store(v, Ordering::Relaxed);
            return;
        }
    }
    ACCENT.store(0xFF00_0000, Ordering::Relaxed);
}

#[inline]
fn current_mode() -> Mode {
    if MODE_VAL.load(Ordering::Relaxed) == Mode::Dark as u8 { Mode::Dark } else { Mode::Light }
}

#[inline]
fn pick(light: (u8, u8, u8), dark: (u8, u8, u8)) -> Color {
    let (r, g, b) = match current_mode() {
        Mode::Light => light,
        Mode::Dark => dark,
    };
    Color::rgb8(r, g, b)
}

// 背景
pub fn bg_root() -> Color    { pick((245, 245, 247), (24, 24, 28)) }
pub fn bg_chrome() -> Color  { pick((232, 232, 236), (20, 20, 24)) }
pub fn bg_panel() -> Color   { pick((250, 250, 252), (28, 28, 32)) }
pub fn bg_modal() -> Color   { pick((255, 255, 255), (30, 30, 34)) }
pub fn bg_status() -> Color  { pick((238, 238, 242), (36, 36, 40)) }
pub fn bg_header() -> Color  { pick((220, 222, 226), (40, 40, 44)) }
pub fn bg_zebra_a() -> Color { pick((250, 250, 252), (28, 28, 30)) }
pub fn bg_zebra_b() -> Color { pick((242, 244, 247), (34, 34, 38)) }

// 罫線
pub fn border_default() -> Color { pick((205, 205, 210), (60, 60, 60)) }
pub fn border_modal() -> Color   { pick((180, 180, 186), (50, 50, 50)) }
pub fn border_strong() -> Color  { pick((150, 150, 158), (80, 80, 80)) }
pub fn border_focus() -> Color   { pick((100, 130, 200), (120, 120, 120)) }

// 文字色
pub fn text_normal() -> Color   { pick((30, 30, 36),   (220, 220, 220)) }
pub fn text_label() -> Color    { pick((50, 50, 58),   (200, 200, 200)) }
pub fn text_dim() -> Color      { pick((90, 90, 98),   (180, 180, 180)) }
pub fn text_very_dim() -> Color { pick((130, 130, 138),(140, 140, 140)) }
pub fn text_emphasis() -> Color { pick((30, 90, 200),  (180, 200, 230)) }
pub fn text_success() -> Color  { pick((30, 120, 50),  (180, 220, 180)) }
pub fn text_dir() -> Color      { pick((20, 80, 180),  (120, 200, 255)) }

// アクセント (選択ハイライト等) — accent_color が指定されていればそれを優先
pub fn accent_select() -> Color {
    let v = ACCENT.load(Ordering::Relaxed);
    if v & 0xFF00_0000 == 0 {
        let r = ((v >> 16) & 0xFF) as u8;
        let g = ((v >> 8) & 0xFF) as u8;
        let b = (v & 0xFF) as u8;
        Color::rgb8(r, g, b)
    } else {
        pick((180, 210, 255), (58, 96, 158))
    }
}
