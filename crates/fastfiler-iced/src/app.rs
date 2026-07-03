//! iced アプリの組み立て (Phase 1: 単一ペイン)。
//!
//! この層の仕事は「入力 → core の Msg 変換」「Effect の実行」「view の組み立て」だけ
//! (計画書 §5.1 の薄い皮)。状態遷移のロジックは fastfiler-core に置く。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use fastfiler_core::model::DEFAULT_ROW_H;
use fastfiler_core::update::{navigate, update_pane};
use fastfiler_core::{domain_event, Entry, NavKey, Overlay, PaneMsg, PaneState};
use iced::keyboard::{self, key::Named, Key};
use iced::widget::{button, column, container, image, row, text, text_input};
use iced::{mouse, window, Element, Event, Length, Subscription, Task};

use crate::effects::{self, domain_channel, IconBytes, PaneWatcher};
use crate::widgets::file_list::{FileList, ListEvent};

pub struct App {
    pane: PaneState,
    icons: HashMap<String, image::Handle>,
    /// キーボード修飾キーの現在値 (マウスイベントに modifiers が乗らないため追跡)。
    modifiers: keyboard::Modifiers,
    /// ペインの watcher 資源 (navigate で付け替え。Phase 3 で PaneId → watcher 表へ)。
    watcher: PaneWatcher,
    /// B-2 用: 合成一覧の件数 (FASTFILER_SYNTH=n)。
    synth: Option<usize>,
    bench: Option<Bench>,
}

/// B-1/B-4 計測 (FASTFILER_BENCH=1): 起動 → Loaded → 直後の描画フレームを stdout へ。
struct Bench {
    t0: Instant,
    loaded_at: Option<Instant>,
    reported: bool,
}

#[derive(Debug, Clone)]
pub enum Msg {
    List(ListEvent),
    /// core メッセージの直接投入 (デバウンス tick・パス入力・履歴ボタン等)。
    Pane(PaneMsg),
    Key(keyboard::Event),
    Window(window::Event),
    /// マウス第 4/5 ボタン (戻る/進む) — ウィンドウ全体で有効。
    MouseNav(PaneMsg),
    /// domain イベント (watcher / ジョブ)。
    Domain(String, serde_json::Value),
    DirLoaded {
        generation: u64,
        result: Result<Vec<Entry>, String>,
        icons: IconBytes,
    },
    Frame(Instant),
    AutoClose,
}

pub fn boot() -> (App, Task<Msg>) {
    let start = std::env::var("FASTFILER_OPEN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs_home());
    let synth = std::env::var("FASTFILER_SYNTH")
        .ok()
        .and_then(|v| v.parse().ok());
    let bench = std::env::var("FASTFILER_BENCH")
        .is_ok_and(|v| v == "1")
        .then(|| Bench {
            t0: Instant::now(),
            loaded_at: None,
            reported: false,
        });

    let mut pane = PaneState::new(start.clone());
    pane.row_h = DEFAULT_ROW_H;
    let mut app = App {
        pane,
        icons: HashMap::new(),
        modifiers: keyboard::Modifiers::default(),
        watcher: PaneWatcher::new(),
        synth,
        bench,
    };
    // 初期フォルダの読み込み (通常のナビゲーションと同じ経路)
    let effects = navigate(&mut app.pane, start);
    app.watcher.watch(&app.pane.cur_path); // apply() を通らないため明示的に監視開始
    let load = app.run_effects(effects);
    let auto = crate::dev::autoclose_task(Msg::AutoClose);
    (app, Task::batch([load, auto]))
}

fn dirs_home() -> PathBuf {
    std::env::var("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("C:\\"))
}

pub fn update(app: &mut App, msg: Msg) -> Task<Msg> {
    match msg {
        Msg::List(ev) => {
            let pane_msg = match ev {
                ListEvent::RowPressed { ix, .. } => PaneMsg::RowPressed {
                    ix,
                    // 修飾キーは App が追跡している現在値を焼き込む
                    ctrl: app.modifiers.control(),
                    shift: app.modifiers.shift(),
                },
                ListEvent::RowDoubleClicked { ix } => PaneMsg::RowDoubleClicked { ix },
                ListEvent::BlankPressed => PaneMsg::BlankPressed,
                ListEvent::HeaderClicked(col) => PaneMsg::HeaderClicked(col),
                ListEvent::ColResized { col, width } => PaneMsg::ColResized { col, width },
                ListEvent::Scrolled(o) => PaneMsg::Scrolled(o),
                ListEvent::ViewportChanged { height } => PaneMsg::ViewportChanged { height },
            };
            app.apply(pane_msg)
        }
        Msg::Pane(m) | Msg::MouseNav(m) => app.apply(m),
        Msg::Domain(event, payload) => {
            let ev = domain_event::parse(&event, &payload);
            app.apply(PaneMsg::Domain(ev))
        }
        Msg::Key(keyboard::Event::ModifiersChanged(m)) => {
            app.modifiers = m;
            Task::none()
        }
        Msg::Key(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
            app.modifiers = modifiers;
            match key_to_msg(&key, modifiers) {
                Some(m) => app.apply(m),
                None => Task::none(),
            }
        }
        Msg::Key(_) => Task::none(),
        Msg::Window(ev) => {
            // フォーカス喪失中に修飾キーを離すと ModifiersChanged が届かず
            // stale になる (Alt+Tab 復帰後の誤 Ctrl+クリック) ため、境界でリセット
            if matches!(ev, window::Event::Unfocused | window::Event::Focused) {
                app.modifiers = keyboard::Modifiers::default();
            }
            Task::none()
        }
        Msg::DirLoaded {
            generation,
            result,
            icons,
        } => {
            for (key, bytes) in icons {
                app.icons.insert(key, image::Handle::from_bytes(bytes));
            }
            if let Some(b) = &mut app.bench {
                if b.loaded_at.is_none() {
                    b.loaded_at = Some(Instant::now());
                }
            }
            let m = match result {
                Ok(entries) => PaneMsg::Loaded {
                    generation,
                    entries,
                },
                Err(error) => PaneMsg::LoadFailed { generation, error },
            };
            app.apply(m)
        }
        Msg::Frame(now) => {
            // ベンチ: Loaded 後の最初の描画フレームで計測を打ち切る
            if let Some(b) = &mut app.bench {
                if let (Some(loaded), false) = (b.loaded_at, b.reported) {
                    b.reported = true;
                    println!(
                        "BENCH_OPEN_MS {:.1}\nBENCH_PAINT_MS {:.1}\nBENCH_ROWS {}",
                        (loaded - b.t0).as_secs_f64() * 1000.0,
                        (now - b.t0).as_secs_f64() * 1000.0,
                        app.pane.entries.len()
                    );
                    return iced::exit();
                }
            }
            Task::none()
        }
        Msg::AutoClose => {
            println!("WINDOW_OK rows={}", app.pane.entries.len());
            iced::exit()
        }
    }
}

impl App {
    /// core の update を呼び、返った Effect を Task に変換する。
    fn apply(&mut self, msg: PaneMsg) -> Task<Msg> {
        let effects = update_pane(&mut self.pane, msg);
        // 読み込み Effect が出たら watcher の監視先も現在フォルダへ追従
        if effects
            .iter()
            .any(|e| matches!(e, fastfiler_core::Effect::LoadDir { .. }))
        {
            self.watcher.watch(&self.pane.cur_path);
        }
        self.run_effects(effects)
    }

    fn run_effects(&self, effects: Vec<fastfiler_core::Effect>) -> Task<Msg> {
        if effects.is_empty() {
            return Task::none(); // 大半のメッセージは Effect なし — キー集合の複製を避ける
        }
        let known: HashSet<String> = self.icons.keys().cloned().collect();
        Task::batch(
            effects
                .into_iter()
                .map(|e| effects::run(e, known.clone(), self.synth)),
        )
    }
}

/// キー入力 → core メッセージ (固定キー。ホットキー設定は Phase 6)。
fn key_to_msg(key: &Key, m: keyboard::Modifiers) -> Option<PaneMsg> {
    let shift = m.shift();
    match key.as_ref() {
        Key::Named(Named::ArrowUp) => Some(PaneMsg::Nav(NavKey::Up, shift)),
        Key::Named(Named::ArrowDown) => Some(PaneMsg::Nav(NavKey::Down, shift)),
        Key::Named(Named::ArrowLeft) if m.alt() => Some(PaneMsg::GoBack),
        Key::Named(Named::ArrowRight) if m.alt() => Some(PaneMsg::GoForward),
        Key::Named(Named::PageUp) => Some(PaneMsg::Nav(NavKey::PageUp, shift)),
        Key::Named(Named::PageDown) => Some(PaneMsg::Nav(NavKey::PageDown, shift)),
        Key::Named(Named::Home) => Some(PaneMsg::Nav(NavKey::Home, shift)),
        Key::Named(Named::End) => Some(PaneMsg::Nav(NavKey::End, shift)),
        Key::Named(Named::Enter) => Some(PaneMsg::ActivateCursor),
        Key::Named(Named::Backspace) => Some(PaneMsg::GoParent),
        Key::Named(Named::Escape) => Some(PaneMsg::ClearSelection),
        Key::Named(Named::F5) => Some(PaneMsg::Reload),
        // CapsLock ON では "A" が届くため大文字小文字を無視して比較
        Key::Character(c) if m.control() && c.eq_ignore_ascii_case("a") => Some(PaneMsg::SelectAll),
        _ => None,
    }
}

pub fn subscription(app: &App) -> Subscription<Msg> {
    let mut subs = vec![
        keyboard::listen().map(Msg::Key),
        window::events().map(|(_id, ev)| Msg::Window(ev)),
        // domain (watcher / ジョブ) イベント: アプリ単一チャネルの受信端
        Subscription::run(domain_events).map(|(e, p)| Msg::Domain(e, p)),
        // マウス第 4/5 ボタン = 戻る/進む (F-303。ウィンドウ全域で有効)
        iced::event::listen_with(|event, _status, _id| match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Back)) => {
                Some(Msg::MouseNav(PaneMsg::GoBack))
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Forward)) => {
                Some(Msg::MouseNav(PaneMsg::GoForward))
            }
            _ => None,
        }),
    ];
    if app.bench.is_some() {
        subs.push(window::frames().map(Msg::Frame));
    }
    Subscription::batch(subs)
}

/// domain チャネルの受信ストリーム (プロセスで 1 回だけ subscribe される)。
fn domain_events() -> impl iced::futures::Stream<Item = (String, serde_json::Value)> {
    domain_channel::take_receiver()
}

pub fn view(app: &App) -> Element<'_, Msg> {
    // パスバー: 通常はテキスト + クリックで直接入力 (F-304)、
    // PathEdit オーバーレイ中は text_input (Enter 確定 / Esc 取消)
    let path_bar: Element<'_, Msg> = match &app.pane.overlay {
        Some(Overlay::PathEdit { value }) => text_input("パスを入力…", value)
            .on_input(|v| Msg::Pane(PaneMsg::PathEditInput(v)))
            .on_submit(Msg::Pane(PaneMsg::PathEditCommit))
            .size(14)
            .padding([2, 6])
            .into(),
        _ => button(text(app.pane.cur_path.to_string_lossy().to_string()).size(14))
            .style(button::text)
            .padding([2, 6])
            .width(Length::Fill)
            .on_press(Msg::Pane(PaneMsg::OpenPathEdit))
            .into(),
    };
    let nav_btn = |label: &'static str, enabled: bool, msg: PaneMsg| {
        button(text(label).size(14))
            .style(button::text)
            .padding([2, 8])
            .on_press_maybe(enabled.then_some(Msg::Pane(msg)))
    };
    let path_row = container(
        row![
            nav_btn("←", !app.pane.history_back.is_empty(), PaneMsg::GoBack),
            nav_btn("→", !app.pane.history_fwd.is_empty(), PaneMsg::GoForward),
            nav_btn("↑", app.pane.cur_path.parent().is_some(), PaneMsg::GoParent),
            path_bar,
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center),
    )
    .padding([4, 8])
    .width(Length::Fill);

    let list: Element<'_, Msg> = FileList::new(&app.pane, &app.icons, Msg::List).into();

    let status = if app.pane.loading {
        "読み込み中…".to_string()
    } else if let Some(e) = &app.pane.load_error {
        format!("エラー: {e}")
    } else if app.pane.selected.is_empty() {
        format!("{} 項目", app.pane.entries.len())
    } else {
        format!(
            "{} 項目 / {} 個を選択",
            app.pane.entries.len(),
            app.pane.selected.len()
        )
    };
    let footer = container(row![text(status).size(13)].spacing(8))
        .padding([4, 8])
        .width(Length::Fill);

    column![path_row, list, footer].into()
}
