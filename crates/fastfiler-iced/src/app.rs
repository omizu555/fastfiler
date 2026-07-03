//! iced アプリの組み立て (Phase 1: 単一ペイン)。
//!
//! この層の仕事は「入力 → core の Msg 変換」「Effect の実行」「view の組み立て」だけ
//! (計画書 §5.1 の薄い皮)。状態遷移のロジックは fastfiler-core に置く。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use fastfiler_core::model::DEFAULT_ROW_H;
use fastfiler_core::update::{navigate, update_pane};
use fastfiler_core::{Entry, NavKey, PaneMsg, PaneState};
use iced::keyboard::{self, key::Named, Key};
use iced::widget::{column, container, image, row, text};
use iced::{window, Element, Length, Subscription, Task};

use crate::effects::{self, IconBytes};
use crate::widgets::file_list::{FileList, ListEvent};

pub struct App {
    pane: PaneState,
    icons: HashMap<String, image::Handle>,
    /// キーボード修飾キーの現在値 (マウスイベントに modifiers が乗らないため追跡)。
    modifiers: keyboard::Modifiers,
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
    Key(keyboard::Event),
    Window(window::Event),
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
        synth,
        bench,
    };
    // 初期フォルダの読み込み (通常のナビゲーションと同じ経路)
    let effects = navigate(&mut app.pane, start);
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

/// キー入力 → core メッセージ (Phase 1 の固定キー。ホットキー設定は Phase 6)。
fn key_to_msg(key: &Key, m: keyboard::Modifiers) -> Option<PaneMsg> {
    let shift = m.shift();
    match key.as_ref() {
        Key::Named(Named::ArrowUp) => Some(PaneMsg::Nav(NavKey::Up, shift)),
        Key::Named(Named::ArrowDown) => Some(PaneMsg::Nav(NavKey::Down, shift)),
        Key::Named(Named::PageUp) => Some(PaneMsg::Nav(NavKey::PageUp, shift)),
        Key::Named(Named::PageDown) => Some(PaneMsg::Nav(NavKey::PageDown, shift)),
        Key::Named(Named::Home) => Some(PaneMsg::Nav(NavKey::Home, shift)),
        Key::Named(Named::End) => Some(PaneMsg::Nav(NavKey::End, shift)),
        Key::Named(Named::Enter) => Some(PaneMsg::ActivateCursor),
        Key::Named(Named::Backspace) => Some(PaneMsg::GoParent),
        Key::Named(Named::Escape) => Some(PaneMsg::ClearSelection),
        Key::Named(Named::F5) => None, // F5 再読み込みは watcher と併せて Phase 2
        // CapsLock ON では "A" が届くため大文字小文字を無視して比較
        Key::Character(c) if m.control() && c.eq_ignore_ascii_case("a") => Some(PaneMsg::SelectAll),
        _ => None,
    }
}

pub fn subscription(app: &App) -> Subscription<Msg> {
    let mut subs = vec![
        keyboard::listen().map(Msg::Key),
        window::events().map(|(_id, ev)| Msg::Window(ev)),
    ];
    if app.bench.is_some() {
        subs.push(window::frames().map(Msg::Frame));
    }
    Subscription::batch(subs)
}

pub fn view(app: &App) -> Element<'_, Msg> {
    let path_bar = container(text(app.pane.cur_path.to_string_lossy().to_string()).size(14))
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

    column![path_bar, list, footer].into()
}
