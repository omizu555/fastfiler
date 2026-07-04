//! S-3: OLE D&D 共存スパイク (計画書 §3 S-3 / §8)。
//!
//! 検証内容:
//! 1. `window::Settings.platform_specific.drag_and_drop = false` で winit 既定の
//!    OLE 登録を止める
//! 2. `OleInitialize` を UI スレッドで行う (winit のイベントループ開始前)
//! 3. `window::run` + `HasWindowHandle` (正規経路) で HWND を取り、update()
//!    (UI スレッド) 内で domain の自前 IDropTarget を `RegisterDragDrop` する
//! 4. エクスプローラから左/右ボタンでファイルをドラッグ → enter/over/drop の
//!    コールバックが飛ぶこと (grfKeyState の MK_RBUTTON で右ボタン判別)
//!
//! 自動検証: 起動 → 登録まで。stdout に `OLE_REGISTER_OK` / `OLE_REGISTER_FAIL …` を出す。
//! 実ドロップは手動 (ウィンドウ内へエクスプローラからドラッグし、表示とコンソールを確認)。

use std::sync::{Arc, Mutex};

use fastfiler_domain::ole_dnd;
use fastfiler_win::drop_target::{
    self, DropCallbacks, DROPEFFECT_COPY, DROPEFFECT_MOVE, DROPEFFECT_NONE, MK_CONTROL, MK_RBUTTON,
    MK_SHIFT,
};
use iced::widget::{column, container, text};
use iced::{window, Element, Subscription, Task};

pub fn main() -> iced::Result {
    // winit イベントループ開始前 = 将来の UI スレッドで OLE を初期化する
    ole_dnd::init_ole();
    println!("OLE_AVAILABLE {}", ole_dnd::is_ole_available());

    iced::application(boot, update, view)
        .title("S-3: OLE D&D 共存スパイク")
        .subscription(subscription)
        .window(window::Settings {
            platform_specific: window::settings::PlatformSpecific {
                // winit 既定の IDropTarget 登録を無効化 (自前登録と競合させない)
                drag_and_drop: false,
                ..Default::default()
            },
            ..Default::default()
        })
        .window_size((720.0, 480.0))
        .run()
}

#[derive(Debug, Clone)]
enum Message {
    WindowOpened(window::Id),
    GotHwnd(Option<isize>),
    Poll,
    AutoClose,
}

#[derive(Default)]
struct App {
    status: String,
    /// コールバック (UI スレッド、OLE ドラッグ中) から表示用に書き込むログ
    shared_log: Arc<Mutex<Vec<String>>>,
    lines: Vec<String>,
}

fn boot() -> (App, Task<Message>) {
    (
        App {
            status: "ウィンドウ待ち…".into(),
            ..App::default()
        },
        fastfiler_iced::dev::autoclose_task(Message::AutoClose),
    )
}

fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::WindowOpened(id) => window::run(id, |w| {
            // HWND 取得の正規経路 (window::raw_id の u64=HWND は winit 内部表現依存なので使わない)。
            // window_handle() は Window の supertrait HasWindowHandle のメソッド。
            w.window_handle()
                .ok()
                .and_then(|h| fastfiler_win::window_interop::hwnd_from_raw(h.as_raw()))
        })
        .map(Message::GotHwnd),
        Message::GotHwnd(None) => {
            println!("OLE_REGISTER_FAIL hwnd を取得できませんでした");
            app.status = "HWND 取得失敗".into();
            Task::none()
        }
        Message::GotHwnd(Some(hwnd)) => {
            // update() は UI スレッド (= OleInitialize したスレッド) で走る
            let log = Arc::clone(&app.shared_log);
            let push = move |s: String| {
                println!("{s}");
                // ドロップコールバック中の panic で poison していても表示用ログは諦めるだけ
                if let Ok(mut l) = log.lock() {
                    l.push(s);
                }
            };
            // TODO(Phase 5): effect 決定は本実装では fastfiler-core へ移す (単体テスト対象)。
            // 本来の既定は「同一ドライブ = MOVE / 別ドライブ = COPY」(F-604、volume_key 比較)。
            // スパイクでは修飾キーの伝搬確認だけが目的なので簡略化しているが、
            // 希望 effect は必ず allowed マスク内から選ぶ (マスク外を返すと NONE に丸められ
            // ドロップ拒否になる)。
            let effect_of = |keys: u32, allowed: u32| {
                let desired = if keys & MK_CONTROL != 0 {
                    DROPEFFECT_COPY
                } else if keys & MK_SHIFT != 0 {
                    DROPEFFECT_MOVE
                } else {
                    DROPEFFECT_COPY
                };
                if desired & allowed != 0 {
                    desired
                } else if allowed & DROPEFFECT_COPY != 0 {
                    DROPEFFECT_COPY
                } else if allowed & DROPEFFECT_MOVE != 0 {
                    DROPEFFECT_MOVE
                } else {
                    DROPEFFECT_NONE
                }
            };
            let p_enter = push.clone();
            let p_leave = push.clone();
            let p_drop = push.clone();
            let result = drop_target::register(
                hwnd,
                DropCallbacks {
                    on_enter: Box::new(move |paths, pt, keys, allowed| {
                        p_enter(format!(
                            "ENTER {} paths at {:?} keys={keys:#04x} (右={}) allowed={allowed}",
                            paths.len(),
                            pt,
                            keys & MK_RBUTTON != 0
                        ));
                        effect_of(keys, allowed)
                    }),
                    on_over: Box::new(move |_paths, _pt, keys, allowed| effect_of(keys, allowed)),
                    on_leave: Box::new(move || p_leave("LEAVE".into())),
                    on_drop: Box::new(move |paths, _pt, keys, allowed| {
                        p_drop(format!(
                            "DROP keys={keys:#04x} (右ボタン={}) effect={} {:?}",
                            keys & MK_RBUTTON != 0,
                            effect_of(keys, allowed),
                            paths
                        ));
                        effect_of(keys, allowed)
                    }),
                },
            );
            app.status = match &result {
                Ok(()) => {
                    println!("OLE_REGISTER_OK hwnd={hwnd:#x}");
                    format!("登録 OK (hwnd={hwnd:#x}) — エクスプローラからドラッグしてください")
                }
                Err(e) => {
                    println!("OLE_REGISTER_FAIL {e}");
                    format!("登録失敗: {e}")
                }
            };
            Task::none()
        }
        Message::Poll => {
            // poison (コールバック中 panic) しても表示更新を諦めるだけ
            if let Ok(mut log) = app.shared_log.lock() {
                app.lines.extend(log.drain(..));
            }
            let len = app.lines.len();
            if len > 12 {
                app.lines.drain(..len - 12);
            }
            Task::none()
        }
        Message::AutoClose => {
            // 本実装でも踏襲するクリーンな終了手順: 登録解除 → OLE 解放 → exit
            drop_target::revoke_all();
            ole_dnd::shutdown_ole();
            iced::exit()
        }
    }
}

fn subscription(_app: &App) -> Subscription<Message> {
    Subscription::batch([
        window::open_events().map(Message::WindowOpened),
        iced::time::every(std::time::Duration::from_millis(300)).map(|_| Message::Poll),
    ])
}

fn view(app: &App) -> Element<'_, Message> {
    let log = app
        .lines
        .iter()
        .fold(column![].spacing(2), |col, s| col.push(text(s).size(13)));
    container(
        column![
            text("OLE D&D 受信スパイク").size(18),
            text(&app.status).size(14),
            text("左ドラッグ / 右ドラッグ / Ctrl・Shift 押下の 3 通りを確認").size(13),
            log,
        ]
        .spacing(10),
    )
    .padding(16)
    .into()
}
