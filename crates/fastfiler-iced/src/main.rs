//! FastFiler iced 版のエントリポイント (起動だけ。組み立ては lib の app 参照)。
//!
//! 検証用の環境変数:
//! - `FASTFILER_OPEN=<path>`     起動時に開くフォルダ (既定: ユーザープロファイル)
//! - `FASTFILER_BENCH=1`         B-1/B-4 計測モード (open/paint ms を出力して終了)
//! - `FASTFILER_SYNTH=<n>`       B-2 用の合成一覧 (FS を介さず n 件)
//! - `FASTFILER_AUTOCLOSE_MS=<n>` n ミリ秒後に自動終了 (起動確認用)

use fastfiler_iced::app;

pub fn main() -> iced::Result {
    // 多重起動防止 (F-1106): 既に起動中なら前面化して静かに終了。
    // 自動検証フロー (AUTOCLOSE/STRESS/BENCH/SYNTH/OPEN) では抑止しない —
    // 機械可読マーカ (WINDOW_OK 等) が出ずハーネスが待ちぼうけになるため
    let dev_run = [
        "FASTFILER_AUTOCLOSE_MS",
        "FASTFILER_STRESS",
        "FASTFILER_BENCH",
        "FASTFILER_SYNTH",
        "FASTFILER_OPEN",
    ]
    .iter()
    .any(|k| std::env::var_os(k).is_some());
    if !dev_run && !fastfiler_win::single_instance::acquire_single_instance() {
        fastfiler_win::single_instance::activate_existing_window();
        return Ok(());
    }
    // winit イベントループ開始前 = 将来の UI スレッドで OLE を初期化 (S-3 実証済み)
    fastfiler_domain::ole_dnd::init_ole();
    // UI フォント (設定 F-1103)。既定は Yu Gothic UI (Windows の日本語 UI 標準 —
    // iced 既定フォントの日本語グリフ問題への対応)。起動時に一度だけ確定する
    let font_name: &'static str = Box::leak(
        fastfiler_iced::settings::get()
            .font_family
            .unwrap_or_else(|| "Yu Gothic UI".to_string())
            .into_boxed_str(),
    );
    iced::application(app::boot, app::update, app::view)
        .title("FastFiler (iced)")
        .default_font(iced::Font::with_name(font_name))
        .theme(app::theme)
        .subscription(app::subscription)
        .exit_on_close_request(false) // 終了時セッション保存のため CloseRequested を受ける
        .window(iced::window::Settings {
            platform_specific: iced::window::settings::PlatformSpecific {
                // winit 既定の IDropTarget 登録を無効化 (自前 OLE 登録と競合させない)
                drag_and_drop: false,
                ..Default::default()
            },
            ..Default::default()
        })
        .window_size((960.0, 640.0))
        .run()
}
