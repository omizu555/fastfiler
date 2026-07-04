//! S-1: 日本語 IME スパイク (計画書 §3 S-1 / §7)。
//!
//! iced 0.14 標準 `text_input` の IME 品質を **Windows 11 + MS-IME の実機** で確認する。
//! 起動: `cargo run -p fastfiler-iced --example spike_ime`
//!
//! 確認シナリオ (結果は Issue #1 / 計画書 §15 に記録):
//! 1. 未確定文字列 (preedit) がキャレット位置に表示されるか (over-the-spot)
//! 2. 変換候補ウィンドウがキャレット近くに出るか
//! 3. 長い未確定文字列 / 文節移動 / 確定 / Esc 取消
//! 4. **キーボード言語切替 (Win+Space) 後の候補ウィンドウ位置** (iced#3189 の再現確認)
//! 5. Enter 確定 → 下の一覧に追加されること (on_submit 経路)

use iced::widget::{column, container, text, text_input};
use iced::{Element, Task};

pub fn main() -> iced::Result {
    iced::application(boot, update, view)
        .title("S-1: 日本語 IME スパイク")
        .window_size((720.0, 480.0))
        .run()
}

#[derive(Debug, Clone)]
enum Message {
    Input(String),
    Submit,
    AutoClose,
}

#[derive(Default)]
struct App {
    value: String,
    committed: Vec<String>,
}

fn boot() -> (App, Task<Message>) {
    (
        App::default(),
        fastfiler_iced::dev::autoclose_task(Message::AutoClose),
    )
}

fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Input(v) => {
            app.value = v;
            Task::none()
        }
        Message::Submit => {
            if !app.value.is_empty() {
                println!("SUBMIT {}", app.value);
                app.committed.push(std::mem::take(&mut app.value));
            }
            Task::none()
        }
        Message::AutoClose => {
            println!("IME_SPIKE_BUILT_OK (品質判定は手動)");
            iced::exit()
        }
    }
}

fn view(app: &App) -> Element<'_, Message> {
    let list = app
        .committed
        .iter()
        .rev()
        .take(10)
        .fold(column![].spacing(2), |col, s| {
            col.push(text(format!("確定: {s}")).size(14))
        });

    container(
        column![
            text("日本語 IME 確認 (詳細手順はファイル先頭コメント)").size(16),
            text("1) 入力欄で日本語を打つ 2) 変換 3) Win+Space で言語切替後にもう一度").size(13),
            text_input("ここに日本語を入力して Enter…", &app.value)
                .on_input(Message::Input)
                .on_submit(Message::Submit)
                .size(18)
                .padding(10),
            list,
        ]
        .spacing(12),
    )
    .padding(16)
    .into()
}
