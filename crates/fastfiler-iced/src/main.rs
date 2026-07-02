//! FastFiler iced 版のエントリポイント。
//! Phase 0: ウィンドウが出て描画されることの確認のみ (計画書 §12 Phase 0)。
//!
//! 検証用: 環境変数 `FASTFILER_AUTOCLOSE_MS=N` を設定すると N ミリ秒後に
//! 描画フレーム数を stdout へ出力して自動終了する
//! (`WINDOW_OK frames=…` = 起動 + 描画成功 / `WINDOW_FAIL` = 1 フレームも描けていない)。

use iced::widget::{center, column, text};
use iced::{window, Element, Subscription, Task};

pub fn main() -> iced::Result {
    iced::application(boot, update, view)
        .title("FastFiler (iced) — Phase 0")
        .subscription(subscription)
        .window_size((640.0, 360.0))
        .run()
}

#[derive(Debug, Clone)]
enum Message {
    Frame,
    AutoClose,
}

#[derive(Default)]
struct App {
    frames: u64,
}

fn boot() -> (App, Task<Message>) {
    (
        App::default(),
        fastfiler_iced::dev::autoclose_task(Message::AutoClose),
    )
}

fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Frame => {
            app.frames += 1;
            Task::none()
        }
        Message::AutoClose => {
            if app.frames > 0 {
                println!("WINDOW_OK frames={}", app.frames);
            } else {
                println!("WINDOW_FAIL frames=0");
            }
            iced::exit()
        }
    }
}

fn subscription(_app: &App) -> Subscription<Message> {
    window::frames().map(|_| Message::Frame)
}

fn view(app: &App) -> Element<'_, Message> {
    center(
        column![
            text("FastFiler (iced) — Phase 0 足場").size(24),
            text(format!("iced 0.14.0 / 描画フレーム: {}", app.frames)),
            text("日本語表示の確認: ファイラ 縦タブ 分割ペイン 🗂"),
        ]
        .spacing(8),
    )
    .into()
}
