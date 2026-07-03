//! iced アプリの組み立て (Phase 1: 単一ペイン)。
//!
//! この層の仕事は「入力 → core の Msg 変換」「Effect の実行」「view の組み立て」だけ
//! (計画書 §5.1 の薄い皮)。状態遷移のロジックは fastfiler-core に置く。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use fastfiler_core::model::{ModalKind, DEFAULT_ROW_H};
use fastfiler_core::transfer::ConflictChoice;
use fastfiler_core::update::{navigate, update_pane};
use fastfiler_core::{domain_event, Entry, NavKey, Overlay, PaneMsg, PaneState};
use fastfiler_domain::undo::UndoManager;
use iced::keyboard::{self, key::Named, Key};
use iced::widget::{button, column, container, image, row, stack, text, text_input};
use iced::{mouse, window, Element, Event, Length, Subscription, Task};

use crate::effects::{self, domain_channel, IconBytes, Jobs, OpOutcome, PaneWatcher};
use crate::widgets::file_list::{FileList, ListEvent};

/// モーダル入力欄のウィジェット Id (開いたときのフォーカス/選択操作の宛先)。
fn modal_input_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("modal-input")
}
fn path_input_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("path-input")
}

pub struct App {
    pane: PaneState,
    icons: HashMap<String, image::Handle>,
    /// キーボード修飾キーの現在値 (マウスイベントに modifiers が乗らないため追跡)。
    modifiers: keyboard::Modifiers,
    /// ペインの watcher 資源 (navigate で付け替え。Phase 3 で PaneId → watcher 表へ)。
    watcher: PaneWatcher,
    /// ファイルジョブのレジストリ (キャンセル) + 通し番号。
    jobs: Jobs,
    /// Undo 履歴 (全ペイン共通・直近 20 件 — domain の実装を再利用。ADR 0006/0008)。
    undo: UndoManager,
    /// 実行中 move ジョブの Undo 候補 (JobDone 全件成功で確定 — ADR 0008)。
    pending_move_undo: HashMap<u64, Vec<fastfiler_domain::undo::MoveItem>>,
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
    /// ジョブ起動通知 (move の Undo 候補を控える)。
    JobStarted {
        job_id: u64,
        move_undo: Option<Vec<fastfiler_domain::undo::MoveItem>>,
    },
    /// ブロッキング操作の完了 (Undo 記録 + ステータス表示)。
    OpDone(OpOutcome),
    /// Ctrl+Z (UndoManager から pop して逆操作を実行)。
    PerformUndo,
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
        jobs: Jobs::new(),
        undo: UndoManager::new(),
        pending_move_undo: HashMap::new(),
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
            // 移動ジョブの Undo 確定: 全件成功時のみ記録 (ADR 0008。部分成功は非記録)
            if let domain_event::DomainEvent::JobDone {
                job_id,
                ok,
                canceled,
                done_files,
                total_files,
                ..
            } = &ev
            {
                if let Some(items) = app.pending_move_undo.remove(job_id) {
                    if *ok && !*canceled && done_files == total_files {
                        app.undo
                            .push(fastfiler_domain::undo::UndoOp::Move { items });
                    }
                }
            }
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
        Msg::JobStarted { job_id, move_undo } => {
            if let Some(items) = move_undo {
                app.pending_move_undo.insert(job_id, items);
            }
            Task::none()
        }
        Msg::OpDone(outcome) => match outcome {
            OpOutcome::Done { record, message } => {
                if let Some(op) = record {
                    app.undo.push(op);
                }
                match message {
                    Some(m) => app.apply(PaneMsg::StatusMsg(m)),
                    None => Task::none(),
                }
            }
            OpOutcome::Failed(e) => app.apply(PaneMsg::StatusMsg(format!("エラー: {e}"))),
        },
        Msg::PerformUndo => match app.undo.pop() {
            Some(op) => effects::perform_undo(op),
            None => app.apply(PaneMsg::StatusMsg("元に戻す操作はありません".into())),
        },
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
        let overlay_before = self.pane.overlay.is_some();
        let effects = update_pane(&mut self.pane, msg);
        // 読み込み Effect が出たら watcher の監視先も現在フォルダへ追従
        if effects
            .iter()
            .any(|e| matches!(e, fastfiler_core::Effect::LoadDir { .. }))
        {
            self.watcher.watch(&self.pane.cur_path);
        }
        let mut task = self.run_effects(effects);
        // 入力オーバーレイが開いたらフォーカスを移す (F2 は拡張子手前まで選択 — USAGE.md §2)
        if !overlay_before {
            task = Task::batch([task, self.overlay_focus_task()]);
        }
        task
    }

    fn overlay_focus_task(&self) -> Task<Msg> {
        use iced::widget::operation;
        match &self.pane.overlay {
            Some(Overlay::PathEdit { .. }) => operation::focus(path_input_id()),
            Some(Overlay::Modal { kind, value }) => {
                let focus = operation::focus(modal_input_id());
                let select: Task<Msg> = match kind {
                    ModalKind::Rename { .. } => {
                        // F2 は拡張子手前まで選択された状態で開く (USAGE.md §2)
                        let stem_chars = fastfiler_core::transfer::split_name(value)
                            .0
                            .chars()
                            .count();
                        operation::select_range(modal_input_id(), 0, stem_chars)
                    }
                    _ => Task::none(),
                };
                Task::batch([focus, select])
            }
            _ => Task::none(),
        }
    }

    fn run_effects(&self, effects: Vec<fastfiler_core::Effect>) -> Task<Msg> {
        if effects.is_empty() {
            return Task::none(); // 大半のメッセージは Effect なし — キー集合の複製を避ける
        }
        let known: HashSet<String> = self.icons.keys().cloned().collect();
        Task::batch(
            effects
                .into_iter()
                .map(|e| effects::run(e, known.clone(), self.synth, &self.jobs)),
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
        Key::Named(Named::F2) => Some(PaneMsg::OpenRename),
        Key::Named(Named::F7) => Some(PaneMsg::OpenNewFolder),
        Key::Named(Named::F8) => Some(PaneMsg::OpenNewFile),
        Key::Named(Named::Delete) => Some(PaneMsg::RequestDelete),
        // CapsLock ON では "A" が届くため大文字小文字を無視して比較
        Key::Character(c) if m.control() && c.eq_ignore_ascii_case("a") => Some(PaneMsg::SelectAll),
        Key::Character(c) if m.control() && c.eq_ignore_ascii_case("c") => {
            Some(PaneMsg::RequestCopy)
        }
        Key::Character(c) if m.control() && c.eq_ignore_ascii_case("x") => {
            Some(PaneMsg::RequestCut)
        }
        Key::Character(c) if m.control() && c.eq_ignore_ascii_case("v") => {
            Some(PaneMsg::RequestPaste)
        }
        Key::Character(c) if m.control() && c.eq_ignore_ascii_case("z") => Some(PaneMsg::Undo),
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
            .id(path_input_id())
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

    // フッタ: ジョブ進捗 (+キャンセル) > 一時メッセージ > 件数/選択数
    let footer: Element<'_, Msg> = if let Some(job) = &app.pane.job {
        let label = match job.kind.as_str() {
            "move" => "移動中",
            "delete" => "削除中",
            _ => "コピー中",
        };
        row![
            text(format!(
                "{label}: {}/{} — {}",
                job.done_files, job.total_files, job.current
            ))
            .size(13)
            .width(Length::Fill),
            button(text("キャンセル").size(12))
                .padding([1, 8])
                .on_press(Msg::Pane(PaneMsg::ClearSelection)), // Esc と同じ経路 (job 優先)
        ]
        .spacing(8)
        .into()
    } else {
        let status = if app.pane.loading {
            "読み込み中…".to_string()
        } else if let Some(e) = &app.pane.load_error {
            format!("エラー: {e}")
        } else if let Some(m) = &app.pane.status_msg {
            m.clone()
        } else if app.pane.selected.is_empty() {
            format!("{} 項目", app.pane.entries.len())
        } else {
            format!(
                "{} 項目 / {} 個を選択",
                app.pane.entries.len(),
                app.pane.selected.len()
            )
        };
        row![text(status).size(13)].spacing(8).into()
    };
    let footer = container(footer).padding([4, 8]).width(Length::Fill);

    let main = column![path_row, list, footer];

    // モーダル/衝突ダイアログは view 最上段の stack で合成 (計画書 §4 オーバーレイ方針)
    match &app.pane.overlay {
        Some(Overlay::Modal { kind, value }) => {
            let (title, placeholder) = match kind {
                ModalKind::Rename { .. } => ("名前の変更", "新しい名前"),
                ModalKind::NewFolder => ("新しいフォルダ", "フォルダ名"),
                ModalKind::NewFile => ("新しいファイル", "ファイル名"),
            };
            let card = dialog_card(
                title,
                column![
                    text_input(placeholder, value)
                        .id(modal_input_id())
                        .on_input(|v| Msg::Pane(PaneMsg::ModalInput(v)))
                        .on_submit(Msg::Pane(PaneMsg::ModalCommit))
                        .size(14)
                        .padding(6),
                    row![
                        button(text("OK").size(13))
                            .padding([3, 14])
                            .on_press(Msg::Pane(PaneMsg::ModalCommit)),
                        button(text("キャンセル").size(13))
                            .padding([3, 14])
                            .on_press(Msg::Pane(PaneMsg::ModalCancel)),
                    ]
                    .spacing(8),
                ]
                .spacing(10)
                .into(),
            );
            stack![main, card].into()
        }
        Some(Overlay::Conflict { plan }) => {
            let n = plan.conflicts.len();
            let first = plan
                .conflicts
                .first()
                .and_then(|&i| plan.items.get(i))
                .map(|it| it.to_name.clone())
                .unwrap_or_default();
            let detail = if n == 1 {
                format!("「{first}」は既に存在します。")
            } else {
                format!("「{first}」ほか {n} 件が既に存在します。")
            };
            let card = dialog_card(
                "同名のファイルがあります",
                column![
                    text(detail).size(13),
                    text("(複数件には一括で適用されます)").size(12),
                    row![
                        button(text("上書き").size(13))
                            .padding([3, 12])
                            .on_press(Msg::Pane(PaneMsg::Conflict(ConflictChoice::Overwrite))),
                        button(text("別名で保存").size(13))
                            .padding([3, 12])
                            .on_press(Msg::Pane(PaneMsg::Conflict(ConflictChoice::RenameBoth))),
                        button(text("キャンセル").size(13))
                            .padding([3, 12])
                            .on_press(Msg::Pane(PaneMsg::Conflict(ConflictChoice::Cancel))),
                    ]
                    .spacing(8),
                ]
                .spacing(10)
                .into(),
            );
            stack![main, card].into()
        }
        _ => main.into(),
    }
}

/// 中央寄せのダイアログカード (背景を薄く暗くする)。
fn dialog_card<'a>(title: &'a str, body: Element<'a, Msg>) -> Element<'a, Msg> {
    let panel = container(column![text(title).size(16), body].spacing(12))
        .padding(16)
        .max_width(420)
        .style(container::rounded_box);
    container(panel)
        .center(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.35).into()),
            ..container::Style::default()
        })
        .into()
}
