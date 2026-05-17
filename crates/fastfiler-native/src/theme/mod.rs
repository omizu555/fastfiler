// テーマカラー定義
//
// `MODE_VAL` (AtomicU8) でアプリ全体の Light/Dark を切替。
// 設定変更時は次回起動から反映 (再起動不要にすると全 view 再構築が必要なため)。
// アクセントカラー (`ACCENT`) は selection 色等に使用。

pub mod fonts;

use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};

use floem::peniko::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Light = 0,
    Dark = 1,
}

static MODE_VAL: AtomicU8 = AtomicU8::new(Mode::Light as u8);
/// アクセントカラー (RGB を 0x00RRGGBB に詰める。0xFF000000 ビットが立っていれば「未設定」)
static ACCENT: AtomicU32 = AtomicU32::new(0xFF00_0000);
/// テーマプリセット id (0 = default = 既存 light/dark を使う)
static PRESET_ID: AtomicU8 = AtomicU8::new(0);

pub fn set_mode_from_str(s: &str) {
    let m = match s {
        "dark" => Mode::Dark,
        // "light" / "system" / その他は Light にフォールバック
        _ => Mode::Light,
    };
    MODE_VAL.store(m as u8, Ordering::Relaxed);
}

#[derive(Clone, Copy)]
struct PresetColors {
    mode: Mode,
    bg_root: (u8, u8, u8),
    bg_chrome: (u8, u8, u8),
    bg_panel: (u8, u8, u8),
    bg_modal: (u8, u8, u8),
    bg_status: (u8, u8, u8),
    bg_header: (u8, u8, u8),
    bg_zebra_a: (u8, u8, u8),
    bg_zebra_b: (u8, u8, u8),
    border_default: (u8, u8, u8),
    text_normal: (u8, u8, u8),
    text_dim: (u8, u8, u8),
    text_emphasis: (u8, u8, u8),
    accent: (u8, u8, u8),
}

const DRACULA: PresetColors = PresetColors {
    mode: Mode::Dark,
    bg_root: (40, 42, 54),
    bg_chrome: (33, 34, 44),
    bg_panel: (40, 42, 54),
    bg_modal: (44, 47, 60),
    bg_status: (33, 34, 44),
    bg_header: (52, 55, 70),
    bg_zebra_a: (40, 42, 54),
    bg_zebra_b: (45, 47, 61),
    border_default: (68, 71, 90),
    text_normal: (248, 248, 242),
    text_dim: (200, 200, 220),
    text_emphasis: (139, 233, 253),
    accent: (189, 147, 249),
};

const SOLARIZED_DARK: PresetColors = PresetColors {
    mode: Mode::Dark,
    bg_root: (0, 43, 54),
    bg_chrome: (7, 54, 66),
    bg_panel: (0, 43, 54),
    bg_modal: (7, 54, 66),
    bg_status: (7, 54, 66),
    bg_header: (15, 67, 80),
    bg_zebra_a: (0, 43, 54),
    bg_zebra_b: (5, 50, 62),
    border_default: (88, 110, 117),
    text_normal: (147, 161, 161),
    text_dim: (101, 123, 131),
    text_emphasis: (38, 139, 210),
    accent: (38, 139, 210),
};

const SOLARIZED_LIGHT: PresetColors = PresetColors {
    mode: Mode::Light,
    bg_root: (253, 246, 227),
    bg_chrome: (238, 232, 213),
    bg_panel: (253, 246, 227),
    bg_modal: (255, 251, 235),
    bg_status: (238, 232, 213),
    bg_header: (228, 222, 200),
    bg_zebra_a: (253, 246, 227),
    bg_zebra_b: (246, 239, 218),
    border_default: (147, 161, 161),
    text_normal: (88, 110, 117),
    text_dim: (101, 123, 131),
    text_emphasis: (38, 139, 210),
    accent: (38, 139, 210),
};

const NORD: PresetColors = PresetColors {
    mode: Mode::Dark,
    bg_root: (46, 52, 64),
    bg_chrome: (59, 66, 82),
    bg_panel: (46, 52, 64),
    bg_modal: (59, 66, 82),
    bg_status: (59, 66, 82),
    bg_header: (67, 76, 94),
    bg_zebra_a: (46, 52, 64),
    bg_zebra_b: (52, 59, 72),
    border_default: (76, 86, 106),
    text_normal: (216, 222, 233),
    text_dim: (180, 187, 200),
    text_emphasis: (136, 192, 208),
    accent: (136, 192, 208),
};

const MONOKAI: PresetColors = PresetColors {
    mode: Mode::Dark,
    bg_root: (39, 40, 34),
    bg_chrome: (30, 31, 26),
    bg_panel: (39, 40, 34),
    bg_modal: (49, 50, 44),
    bg_status: (30, 31, 26),
    bg_header: (60, 61, 53),
    bg_zebra_a: (39, 40, 34),
    bg_zebra_b: (45, 46, 39),
    border_default: (73, 72, 62),
    text_normal: (248, 248, 242),
    text_dim: (200, 200, 195),
    text_emphasis: (102, 217, 239),
    accent: (249, 38, 114),
};

pub fn set_preset_from_str(s: &str) {
    let id: u8 = match s {
        "dracula" => 1,
        "solarizedDark" | "solarized-dark" => 2,
        "solarizedLight" | "solarized-light" => 3,
        "nord" => 4,
        "monokai" => 5,
        _ => 0,
    };
    PRESET_ID.store(id, Ordering::Relaxed);
    if let Some(p) = current_preset() {
        MODE_VAL.store(p.mode as u8, Ordering::Relaxed);
    }
}

fn current_preset() -> Option<&'static PresetColors> {
    match PRESET_ID.load(Ordering::Relaxed) {
        1 => Some(&DRACULA),
        2 => Some(&SOLARIZED_DARK),
        3 => Some(&SOLARIZED_LIGHT),
        4 => Some(&NORD),
        5 => Some(&MONOKAI),
        _ => None,
    }
}

#[inline]
fn rgb(c: (u8, u8, u8)) -> Color {
    Color::rgb8(c.0, c.1, c.2)
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
    if MODE_VAL.load(Ordering::Relaxed) == Mode::Dark as u8 {
        Mode::Dark
    } else {
        Mode::Light
    }
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
pub fn bg_root() -> Color {
    if let Some(p) = current_preset() {
        return rgb(p.bg_root);
    }
    pick((245, 245, 247), (24, 24, 28))
}
pub fn bg_chrome() -> Color {
    if let Some(p) = current_preset() {
        return rgb(p.bg_chrome);
    }
    pick((232, 232, 236), (20, 20, 24))
}
pub fn bg_panel() -> Color {
    if let Some(p) = current_preset() {
        return rgb(p.bg_panel);
    }
    pick((250, 250, 252), (28, 28, 32))
}
pub fn bg_modal() -> Color {
    if let Some(p) = current_preset() {
        return rgb(p.bg_modal);
    }
    pick((255, 255, 255), (30, 30, 34))
}
pub fn bg_status() -> Color {
    if let Some(p) = current_preset() {
        return rgb(p.bg_status);
    }
    pick((238, 238, 242), (36, 36, 40))
}
pub fn bg_header() -> Color {
    if let Some(p) = current_preset() {
        return rgb(p.bg_header);
    }
    pick((220, 222, 226), (40, 40, 44))
}
pub fn bg_zebra_a() -> Color {
    if let Some(p) = current_preset() {
        return rgb(p.bg_zebra_a);
    }
    pick((250, 250, 252), (28, 28, 30))
}
pub fn bg_zebra_b() -> Color {
    if let Some(p) = current_preset() {
        return rgb(p.bg_zebra_b);
    }
    pick((242, 244, 247), (34, 34, 38))
}
/// 行ホバー時の薄い強調色。選択色より控えめにし、明暗モードで自然に見える色を返す。
pub fn bg_hover() -> Color {
    pick((226, 232, 244), (52, 56, 66))
}

// 罫線
pub fn border_default() -> Color {
    if let Some(p) = current_preset() {
        return rgb(p.border_default);
    }
    pick((205, 205, 210), (60, 60, 60))
}
pub fn border_modal() -> Color {
    pick((180, 180, 186), (50, 50, 50))
}
pub fn border_strong() -> Color {
    pick((150, 150, 158), (80, 80, 80))
}
pub fn border_focus() -> Color {
    pick((100, 130, 200), (120, 120, 120))
}

// 文字色
pub fn text_normal() -> Color {
    if let Some(p) = current_preset() {
        return rgb(p.text_normal);
    }
    pick((30, 30, 36), (220, 220, 220))
}
pub fn text_label() -> Color {
    pick((50, 50, 58), (200, 200, 200))
}
pub fn text_dim() -> Color {
    if let Some(p) = current_preset() {
        return rgb(p.text_dim);
    }
    pick((90, 90, 98), (180, 180, 180))
}
pub fn text_very_dim() -> Color {
    pick((130, 130, 138), (140, 140, 140))
}
pub fn text_emphasis() -> Color {
    if let Some(p) = current_preset() {
        return rgb(p.text_emphasis);
    }
    pick((30, 90, 200), (180, 200, 230))
}
pub fn text_success() -> Color {
    pick((30, 120, 50), (180, 220, 180))
}
pub fn text_dir() -> Color {
    if let Some(p) = current_preset() {
        return rgb(p.text_emphasis);
    }
    pick((20, 80, 180), (120, 200, 255))
}

// アクセント (選択ハイライト等) — accent_color が指定されていればそれを優先、次に preset、最後に既定
pub fn accent_select() -> Color {
    let v = ACCENT.load(Ordering::Relaxed);
    if v & 0xFF00_0000 == 0 {
        let r = ((v >> 16) & 0xFF) as u8;
        let g = ((v >> 8) & 0xFF) as u8;
        let b = (v & 0xFF) as u8;
        Color::rgb8(r, g, b)
    } else if let Some(p) = current_preset() {
        rgb(p.accent)
    } else {
        pick((180, 210, 255), (58, 96, 158))
    }
}
