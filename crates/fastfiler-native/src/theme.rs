// テーマカラー定義 (Light テーマ既定)
//
// main.rs 内で `theme::xxx()` で参照される全ての色を集約。
// 将来的にダークテーマへ切替えできるよう関数経由でアクセスする。

use floem::peniko::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Light,
    #[allow(dead_code)]
    Dark,
}

// 既定はライトテーマ
pub const MODE: Mode = Mode::Light;

#[inline]
fn pick(light: (u8, u8, u8), dark: (u8, u8, u8)) -> Color {
    let (r, g, b) = match MODE {
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

// アクセント (選択ハイライト等)
pub fn accent_select() -> Color { pick((180, 210, 255), (58, 96, 158)) }
