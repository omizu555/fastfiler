//! FastFiler ネイティブ GUI のライブラリエントリポイント。
//!
//! 実行可能バイナリは `main.rs` から [`run_app`] を呼ぶだけ。
//! テストや別バイナリから FastFiler の起動を再利用したいケースに備えてライブラリ化している。
//!
//! モジュール構成 (機能別):
//! - [`core`]   — AppState / PaneState / fs_model / actions
//! - [`theme`]  — カラーパレット / インストール済みフォント取得
//! - [`settings`] — 設定モデルと設定ダイアログ
//! - [`hotkeys`] — ホットキー解決と dispatch
//! - [`logger`]  — ファイルロガー (`%APPDATA%/FastFiler/fastfiler.log`)
//! - [`ui`]      — floem ベース UI (app_view / pane / tabs / tree / footer / splitter)

#[macro_use]
pub mod logger;

pub mod core;
pub mod hotkeys;
pub mod search;
pub mod settings;
pub mod theme;
pub mod ui;

// 既存コードが使う `crate::state` / `crate::actions` / `crate::fs_model`
// を維持するための後方互換エイリアス。
pub use core::{actions, fs_model, state};

use ui::app_view::app_view;

/// FastFiler を起動する。`fn main()` から 1 回だけ呼ぶ想定。
pub fn run_app() {
    use floem::kurbo::{Point as KPoint, Size as KSize};
    use floem::window::WindowConfig;
    use settings::PersistedSettings;

    logger::init();
    flog!("[main] settings load start");
    let p = PersistedSettings::load_or_default();
    flog!(
        "[main] settings loaded: theme={} accent={} window=({:?}x{:?} @ {:?},{:?})",
        p.theme,
        p.accent_color,
        p.window_w,
        p.window_h,
        p.window_x,
        p.window_y
    );
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
