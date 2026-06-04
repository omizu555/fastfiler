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

/// メモリ調査用の counting global allocator (feature `mem-debug` のみ)。
/// 通常ビルドでは差し替えない (System のまま)。
/// `dhat-heap` 有効時は dhat 側のアロケータを使うため無効化する。
#[cfg(all(feature = "mem-debug", not(feature = "dhat-heap")))]
#[global_allocator]
static GLOBAL_ALLOC: core::debug_mem::TrackingAlloc = core::debug_mem::TrackingAlloc;

/// dhat ヒーププロファイラ用 global allocator (feature `dhat-heap` のみ)。
/// アロケーションのコールスタックを記録し、終了時に dhat-heap.json を出力する。
#[cfg(feature = "dhat-heap")]
#[global_allocator]
static DHAT_ALLOC: dhat::Alloc = dhat::Alloc;

pub mod core;
pub mod hotkeys;
pub mod search;
pub mod settings;
pub mod theme;
pub mod ui;

#[cfg(windows)]
pub mod win32;

// 既存コードが使う `crate::state` / `crate::actions` / `crate::fs_model`
// を維持するための後方互換エイリアス。
pub use core::{actions, fs_model, state};

use ui::app_view::app_view;

/// FastFiler を起動する。`fn main()` から 1 回だけ呼ぶ想定。
pub fn run_app() {
    use floem::kurbo::{Point as KPoint, Size as KSize};
    use floem::window::WindowConfig;
    use settings::PersistedSettings;

    // dhat ヒーププロファイラを起動 (feature `dhat-heap` のみ)。
    // run_app の戻り (ウィンドウ閉鎖で event loop が return) まで保持し、
    // Drop 時に dhat-heap.json (アロケーション元のコールスタック付き) を出力する。
    #[cfg(feature = "dhat-heap")]
    let _dhat_profiler = dhat::Profiler::new_heap();

    // wgpu バックエンドを GL に固定してメモリ使用量を抑える (DX12/Vulkan 比で実測 ~45% 減)。
    // ユーザーが環境変数で明示指定している場合はそちらを尊重する。
    // floem (wgpu) 初期化より前に設定する必要がある。
    if std::env::var_os("WGPU_BACKEND").is_none() {
        std::env::set_var("WGPU_BACKEND", "gl");
    }

    logger::init();
    flog!("[main] settings load start");

    // 多重起動チェック (Windows のみ)。既に起動中なら既存ウィンドウを前面化して終了。
    #[cfg(windows)]
    {
        if !crate::win32::single_instance::acquire_single_instance() {
            flog!("[main] another instance is already running; activating existing window and exiting");
            crate::win32::single_instance::activate_existing_window();
            return;
        }
    }

    // OLE D&D (送信側) 初期化。UI スレッドで 1 回だけ呼ぶ。
    // 失敗してもアプリは続行 (is_ole_available() が false で start_drag が抑止される)。
    #[cfg(windows)]
    {
        fastfiler_domain::ole_dnd::init_ole();
        flog!(
            "[main] ole init done: available={}",
            fastfiler_domain::ole_dnd::is_ole_available()
        );
    }

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

    // メモリ調査用: 2 秒周期でスナップショットをログに出す (feature `mem-debug` のみ)。
    // floem タイマーに依存しない独立スレッドで、atomic と Win32 のみを読む。
    #[cfg(feature = "mem-debug")]
    {
        crate::core::debug_mem::log_snapshot("startup");
        std::thread::spawn(|| loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            crate::core::debug_mem::log_snapshot("tick");
        });
    }

    floem::Application::new()
        .window(move |_| app_view(), Some(cfg))
        .run();
}
