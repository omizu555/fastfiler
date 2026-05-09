// FastFiler — エントリポイント

mod actions;
mod fonts;
mod fs_model;
mod hotkeys;
mod logger;
mod settings;
mod state;
mod theme;
mod ui;

use ui::app_view::app_view;

fn main() {
    use floem::kurbo::{Point as KPoint, Size as KSize};
    use floem::window::WindowConfig;
    use settings::PersistedSettings;

    logger::init();
    flog!("[main] settings load start");
    let p = PersistedSettings::load_or_default();
    flog!("[main] settings loaded: theme={} accent={} window=({:?}x{:?} @ {:?},{:?})",
        p.theme, p.accent_color, p.window_w, p.window_h, p.window_x, p.window_y);
    // テーマ・アクセントカラーをグローバル設定として反映 (起動時1回)
    crate::theme::set_mode_from_str(&p.theme);
    crate::theme::set_preset_from_str(&p.theme_preset);
    crate::theme::set_accent_from_str(&p.accent_color);

    let mut cfg = WindowConfig::default().title("FastFiler");
    if let (Some(w), Some(h)) = (p.window_w, p.window_h) {
        if w >= 200 && h >= 150 {
            cfg = cfg.size(KSize::new(w as f64, h as f64));
        }
    }
    if let (Some(x), Some(y)) = (p.window_x, p.window_y) {
        cfg = cfg.position(KPoint::new(x as f64, y as f64));
    }

    floem::Application::new()
        .window(move |_| app_view(), Some(cfg))
        .run();
}

