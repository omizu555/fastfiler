//! FastFiler iced 版のエントリポイント (起動だけ。組み立ては lib の app 参照)。
//!
//! 検証用の環境変数:
//! - `FASTFILER_OPEN=<path>`     起動時に開くフォルダ (既定: ユーザープロファイル)
//! - `FASTFILER_BENCH=1`         B-1/B-4 計測モード (open/paint ms を出力して終了)
//! - `FASTFILER_SYNTH=<n>`       B-2 用の合成一覧 (FS を介さず n 件)
//! - `FASTFILER_AUTOCLOSE_MS=<n>` n ミリ秒後に自動終了 (起動確認用)

use fastfiler_iced::app;

pub fn main() -> iced::Result {
    // winit イベントループ開始前 = 将来の UI スレッドで OLE を初期化 (S-3 実証済み)
    fastfiler_domain::ole_dnd::init_ole();
    iced::application(app::boot, app::update, app::view)
        .title("FastFiler (iced)")
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
