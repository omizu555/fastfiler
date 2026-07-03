//! iced アプリの組み立て (Phase 3: 縦タブ + BSP 分割ペイン)。
//!
//! この層の仕事は「入力 → core の Msg 変換」「Effect の実行」「view の組み立て」だけ
//! (計画書 §5.1 の薄い皮)。状態遷移のロジックは fastfiler-core に置く。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use fastfiler_core::app_model::{panes_alive, AppModel};
use fastfiler_core::bsp::{PaneNode, SplitDir, PANE_MIN_PX};
use fastfiler_core::model::{ModalKind, DEFAULT_ROW_H};
use fastfiler_core::transfer::ConflictChoice;
use fastfiler_core::tree::TreeMsg;
use fastfiler_core::update::navigate;
use fastfiler_core::update_app::{update_app, AppMsg, DragMsg, TabMsg};
use fastfiler_core::{domain_event, Entry, NavKey, Overlay, PaneId, PaneMsg};
use fastfiler_domain::undo::UndoManager;
use iced::keyboard::{self, key::Named, Key};
use iced::widget::{button, column, container, image, row, stack, text, text_input};
use iced::{mouse, window, Element, Event, Length, Size, Subscription, Task};

use crate::effects::{self, domain_channel, IconBytes, Jobs, OpOutcome, PaneWatcher};
use crate::widgets::context_menu::{ContextMenu, MenuEvent};
use crate::widgets::drag_handle::DragHandle;
use crate::widgets::file_list::{FileList, ListEvent, HEADER_H};
use crate::widgets::tab_bar::{TabBar, TabBarEvent, TabItem, CELL_H};
use crate::widgets::tree_list::{TreeEvent, TreeList};

/// モーダル入力欄のウィジェット Id (開いたときのフォーカス/選択操作の宛先)。
fn modal_input_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("modal-input")
}
fn path_input_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("path-input")
}
fn search_input_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("search-input")
}

pub struct App {
    model: AppModel,
    icons: HashMap<String, image::Handle>,
    /// キーボード修飾キーの現在値 (マウスイベントに modifiers が乗らないため追跡)。
    modifiers: keyboard::Modifiers,
    /// ペイン毎の watcher 資源 (PaneClosed で解放 — N-02 の検証対象)。
    watchers: HashMap<PaneId, PaneWatcher>,
    /// ファイルジョブのレジストリ (キャンセル) + 通し番号。
    jobs: Jobs,
    /// ジョブの所属ペイン (進捗/完了イベントのルーティング先)。
    job_owner: HashMap<u64, PaneId>,
    /// 検索 (domain。アプリ全体で高々 1 検索 — 前回は自動キャンセル)。
    search: std::sync::Arc<fastfiler_domain::search::SearchState>,
    /// 検索の所属ペイン (job_id → pane)。
    search_owner: HashMap<u64, PaneId>,
    /// Everything ポート (Phase 6 で設定化。既定 80)。
    everything_port: u16,
    /// Undo 履歴 (全ペイン共通・直近 20 件 — domain の実装を再利用。ADR 0006/0008)。
    undo: UndoManager,
    /// 実行中 move ジョブの Undo 候補 (JobDone 全件成功で確定 — ADR 0008)。
    pending_move_undo: HashMap<u64, Vec<fastfiler_domain::undo::MoveItem>>,
    /// ウィンドウの論理サイズ (分割リサイズの px→比率換算とセッション保存用)。
    window_size: Size,
    /// ウィンドウ位置 (セッション保存用)。
    window_pos: Option<iced::Point>,
    /// 非最大化時の最後の位置サイズ (最大化中の保存はこれを使う)。
    saved_bounds: Option<[f32; 4]>,
    restore_maximized: bool,
    /// メインウィンドウ Id (is_maximized 問い合わせ用)。
    window_id: Option<window::Id>,
    /// Win32 HWND (シェルメニュー / OLE 用。WindowOpened 後に確定)。
    hwnd: Option<isize>,
    /// セッション保存のデバウンス世代 (800ms)。
    save_seq: u64,
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
    /// FileList のイベント (どのペインからかを添えて)。
    List(PaneId, ListEvent),
    /// core メッセージの直接投入。
    Core(AppMsg),
    Key(keyboard::Event),
    Window(window::Event),
    /// domain イベント (watcher / ジョブ)。
    Domain(String, serde_json::Value),
    DirLoaded {
        pane: PaneId,
        generation: u64,
        result: Result<Vec<Entry>, String>,
        icons: IconBytes,
    },
    /// ブロッキング操作の完了 (Undo 記録 + ステータス表示)。
    OpDone(OpOutcome),
    /// Ctrl+Z (UndoManager から pop して逆操作を実行)。
    PerformUndo,
    /// セッション保存デバウンス満了 (seq 一致時のみ保存)。
    SessionSaveTick(u64),
    /// is_maximized の結果を添えて保存を実行。
    DoSessionSave(bool),
    /// メインウィンドウが開いた (Id 記録 + 位置復元)。
    WindowOpened(window::Id),
    GotHwnd(Option<isize>),
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

    // セッション復元 (F-1001)。FASTFILER_OPEN 指定時は復元をスキップ (検証用)
    let session = if std::env::var("FASTFILER_OPEN").is_ok() {
        None
    } else {
        fastfiler_core::session::load()
    };
    let (mut model, restored_panes, saved_bounds, restore_maximized) = match &session {
        Some(data) => {
            let (m, panes) = fastfiler_core::session::restore(data, start.clone());
            (m, panes, data.window, data.maximized)
        }
        None => {
            let m = AppModel::new(start.clone());
            (m, vec![], None, false)
        }
    };
    {
        let id = model.focused_pane();
        model.panes[id].row_h = DEFAULT_ROW_H;
    }
    let mut app = App {
        model,
        icons: HashMap::new(),
        modifiers: keyboard::Modifiers::default(),
        watchers: HashMap::new(),
        jobs: Jobs::new(),
        job_owner: HashMap::new(),
        search: std::sync::Arc::new(fastfiler_domain::search::SearchState::default()),
        search_owner: HashMap::new(),
        everything_port: 80,
        undo: UndoManager::new(),
        pending_move_undo: HashMap::new(),
        window_size: Size::new(960.0, 640.0),
        window_pos: None,
        saved_bounds,
        restore_maximized,
        window_id: None,
        hwnd: None,
        save_seq: 0,
        synth,
        bench,
    };
    // 復元された全ペイン (または初期ペイン) の読み込みを開始
    let load = if restored_panes.is_empty() {
        let id = app.model.focused_pane();
        let effects = navigate(&mut app.model.panes[id], id, start);
        app.run_effects(effects)
    } else {
        let tasks: Vec<Task<Msg>> = restored_panes
            .into_iter()
            .map(|id| {
                let effects = fastfiler_core::update::reload(&mut app.model.panes[id], id);
                app.run_effects(effects)
            })
            .collect();
        Task::batch(tasks)
    };

    // B-3: タブ/ペイン開閉ストレス (FASTFILER_STRESS=n)。
    // 実 watcher 込みで n 回開閉し、PANES_ALIVE と watcher 数がベースラインへ
    // 戻ることを stdout に出す (スレッド/ハンドル数は外部の Get-Process で計測)。
    if let Ok(n) = std::env::var("FASTFILER_STRESS") {
        if let Ok(n) = n.parse::<u32>() {
            let before = (panes_alive(), app.watchers.len());
            for _ in 0..n {
                let _ = app.apply(AppMsg::Tab(TabMsg::Add));
                let _ = app.apply(AppMsg::Tab(TabMsg::SplitFocused(SplitDir::Row)));
                let _ = app.apply(AppMsg::Tab(TabMsg::SplitFocused(SplitDir::Column)));
                let active = app.model.active;
                let _ = app.apply(AppMsg::Tab(TabMsg::Close(active)));
            }
            println!(
                "STRESS_RESULT cycles={n} panes_before={} panes_after={} watchers_before={} watchers_after={}",
                before.0,
                panes_alive(),
                before.1,
                app.watchers.len()
            );
        }
    }

    let drives = effects::run(fastfiler_core::Effect::LoadDrives, &app.jobs);
    let auto = crate::dev::autoclose_task(Msg::AutoClose);
    (app, Task::batch([load, drives, auto]))
}

fn dirs_home() -> PathBuf {
    std::env::var("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("C:\\"))
}

pub fn update(app: &mut App, msg: Msg) -> Task<Msg> {
    match msg {
        Msg::List(pane, ev) => {
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
                ListEvent::RowRightClicked { ix, x, y } => {
                    if app.modifiers.shift() {
                        // Shift+右クリック = Windows シェルメニュー (F-905)
                        return app.apply(AppMsg::Pane(
                            pane,
                            PaneMsg::ShellMenuRequest { row: Some(ix) },
                        ));
                    }
                    let (templates, commands, templates_dir, can_paste) =
                        effects::collect_menu_context();
                    return app.apply(AppMsg::Pane(
                        pane,
                        PaneMsg::OpenMenu {
                            at: (x, y),
                            row: Some(ix),
                            templates,
                            commands,
                            templates_dir,
                            can_paste,
                        },
                    ));
                }
                ListEvent::DragStarted { ix, right_button } => {
                    return app.apply(AppMsg::Drag(DragMsg::Started {
                        pane,
                        row: ix,
                        right_button,
                    }));
                }
                ListEvent::DragHover { row } => {
                    return app.apply(AppMsg::Drag(DragMsg::Hover { pane, row }));
                }
                ListEvent::DragDropped { row, x, y } => {
                    // 右ボタンドロップならメニュー用コマンドを採取 (when: "drop")
                    let commands = if app.model.drag.as_ref().is_some_and(|d| d.right_button) {
                        effects::collect_menu_context().1
                    } else {
                        vec![]
                    };
                    return app.apply(AppMsg::Drag(DragMsg::Dropped {
                        pane,
                        row,
                        ctrl: app.modifiers.control(),
                        shift: app.modifiers.shift(),
                        at: (x, y),
                        commands,
                    }));
                }
                ListEvent::BlankRightClicked { x, y } => {
                    let (templates, commands, templates_dir, can_paste) =
                        effects::collect_menu_context();
                    return app.apply(AppMsg::Pane(
                        pane,
                        PaneMsg::OpenMenu {
                            at: (x, y),
                            row: None,
                            templates,
                            commands,
                            templates_dir,
                            can_paste,
                        },
                    ));
                }
            };
            app.apply(AppMsg::Pane(pane, pane_msg))
        }
        Msg::Core(m) => app.apply(m),
        Msg::Domain(event, payload) => {
            let ev = domain_event::parse(&event, &payload);
            match &ev {
                domain_event::DomainEvent::FsChange { .. } => {
                    // 全ペインへブロードキャスト (各ペインが cur_path で選別する)
                    let ids: Vec<PaneId> = app.model.panes.keys().collect();
                    let tasks: Vec<Task<Msg>> = ids
                        .into_iter()
                        .map(|id| app.apply(AppMsg::Pane(id, PaneMsg::Domain(ev.clone()))))
                        .collect();
                    Task::batch(tasks)
                }
                domain_event::DomainEvent::JobProgress { job_id, .. } => {
                    match app.job_owner.get(job_id).copied() {
                        Some(id) => app.apply(AppMsg::Pane(id, PaneMsg::Domain(ev))),
                        None => Task::none(),
                    }
                }
                domain_event::DomainEvent::JobDone {
                    job_id,
                    ok,
                    canceled,
                    done_files,
                    total_files,
                    ..
                } => {
                    // 移動ジョブの Undo 確定: 全件成功時のみ記録 (ADR 0008)
                    if let Some(items) = app.pending_move_undo.remove(job_id) {
                        if *ok && !*canceled && done_files == total_files {
                            app.undo
                                .push(fastfiler_domain::undo::UndoOp::Move { items });
                        }
                    }
                    match app.job_owner.remove(job_id) {
                        Some(id) => app.apply(AppMsg::Pane(id, PaneMsg::Domain(ev))),
                        None => Task::none(),
                    }
                }
                domain_event::DomainEvent::SearchHit { job_id, .. } => {
                    match app.search_owner.get(job_id).copied() {
                        Some(id) => app.apply(AppMsg::Pane(id, PaneMsg::Domain(ev))),
                        None => Task::none(),
                    }
                }
                domain_event::DomainEvent::SearchDone { job_id, .. } => {
                    match app.search_owner.remove(job_id) {
                        Some(id) => app.apply(AppMsg::Pane(id, PaneMsg::Domain(ev))),
                        None => Task::none(),
                    }
                }
                domain_event::DomainEvent::Unknown { .. } => Task::none(),
            }
        }
        Msg::Key(keyboard::Event::ModifiersChanged(m)) => {
            app.modifiers = m;
            Task::none()
        }
        Msg::Key(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
            app.modifiers = modifiers;
            match key_to_msg(&key, modifiers) {
                Some(m) => update(app, m),
                None => Task::none(),
            }
        }
        Msg::Key(_) => Task::none(),
        Msg::Window(ev) => {
            match ev {
                // フォーカス喪失中に修飾キーを離すと ModifiersChanged が届かず
                // stale になる (Alt+Tab 復帰後の誤 Ctrl+クリック) ため、境界でリセット
                window::Event::Unfocused | window::Event::Focused => {
                    app.modifiers = keyboard::Modifiers::default();
                    if app.model.drag.is_some() {
                        let _ = update_app(&mut app.model, AppMsg::Drag(DragMsg::Cancel));
                    }
                }
                window::Event::Resized(size) => {
                    app.window_size = size;
                    return app.schedule_save();
                }
                window::Event::Moved(pos) => {
                    app.window_pos = Some(pos);
                    return app.schedule_save();
                }
                window::Event::CloseRequested => {
                    // 終了時保存 (F-1001): 最大化状態を確認してから保存 → exit
                    return app.query_and_save(true);
                }
                _ => {}
            }
            Task::none()
        }
        Msg::WindowOpened(id) => {
            app.window_id = Some(id);
            // HWND を正規経路で取得 (シェルメニュー / Phase 5c の OLE 用)
            let hwnd_task = window::run(id, |w| {
                w.window_handle()
                    .ok()
                    .and_then(|h| fastfiler_win::window_interop::hwnd_from_raw(h.as_raw()))
            })
            .map(Msg::GotHwnd);
            // ウィンドウ位置・サイズ・最大化の復元
            let mut tasks = Vec::new();
            if let Some([x, y, w, h]) = app.saved_bounds {
                tasks.push(window::move_to::<Msg>(id, iced::Point::new(x, y)));
                tasks.push(window::resize::<Msg>(id, Size::new(w, h)));
            }
            if app.restore_maximized {
                tasks.push(window::maximize::<Msg>(id, true));
            }
            tasks.push(hwnd_task);
            Task::batch(tasks)
        }
        Msg::GotHwnd(h) => {
            app.hwnd = h;
            Task::none()
        }
        Msg::SessionSaveTick(seq) => {
            if seq == app.save_seq {
                app.query_and_save(false)
            } else {
                Task::none()
            }
        }
        Msg::DoSessionSave(maximized) => {
            app.save_session(maximized);
            Task::none()
        }
        Msg::DirLoaded {
            pane,
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
            app.apply(AppMsg::Pane(pane, m))
        }
        Msg::OpDone(outcome) => match outcome {
            OpOutcome::Done { record, message } => {
                if let Some(op) = record {
                    app.undo.push(op);
                }
                let status = match message {
                    Some(m) => app.apply(AppMsg::Focused(PaneMsg::StatusMsg(m))),
                    None => Task::none(),
                };
                // watcher が効かないフォルダでも結果を反映 (明示 reload — GPUI パリティ)
                let reload = app.apply(AppMsg::Focused(PaneMsg::Reload));
                Task::batch([status, reload])
            }
            OpOutcome::Failed(e) => {
                app.apply(AppMsg::Focused(PaneMsg::StatusMsg(format!("エラー: {e}"))))
            }
        },
        Msg::PerformUndo => match app.undo.pop() {
            Some(op) => effects::perform_undo(op),
            None => app.apply(AppMsg::Focused(PaneMsg::StatusMsg(
                "元に戻す操作はありません".into(),
            ))),
        },
        Msg::Frame(now) => {
            // ベンチ: Loaded 後の最初の描画フレームで計測を打ち切る
            if let Some(b) = &mut app.bench {
                if let (Some(loaded), false) = (b.loaded_at, b.reported) {
                    if now >= loaded {
                        b.reported = true;
                        let id = app.model.focused_pane();
                        println!(
                            "BENCH_OPEN_MS {:.1}\nBENCH_PAINT_MS {:.1}\nBENCH_ROWS {}",
                            (loaded - b.t0).as_secs_f64() * 1000.0,
                            (now - b.t0).as_secs_f64() * 1000.0,
                            app.model.panes[id].entries.len()
                        );
                        return iced::exit();
                    }
                }
            }
            Task::none()
        }
        Msg::AutoClose => {
            app.save_session(false);
            let id = app.model.focused_pane();
            println!(
                "WINDOW_OK rows={} panes_alive={} watchers={}",
                app.model.panes[id].entries.len(),
                panes_alive(),
                app.watchers.len()
            );
            iced::exit()
        }
    }
}

impl App {
    /// core の update を呼び、返った Effect を Task に変換する。
    fn apply(&mut self, msg: AppMsg) -> Task<Msg> {
        let focused_before = self.model.focused_pane();
        let overlay_before = self
            .model
            .panes
            .get(focused_before)
            .is_some_and(|p| p.overlay.is_some());
        let search_before = self
            .model
            .panes
            .get(focused_before)
            .is_some_and(|p| p.search.is_some());
        let effects = update_app(&mut self.model, msg);
        let mut task = self.run_effects(effects);
        // 入力オーバーレイ/検索バーが開いたらフォーカスを移す
        if !overlay_before {
            task = Task::batch([task, self.overlay_focus_task()]);
        }
        let search_now = self
            .model
            .panes
            .get(self.model.focused_pane())
            .is_some_and(|p| p.search.is_some());
        if !search_before && search_now {
            task = Task::batch([task, iced::widget::operation::focus(search_input_id())]);
        }
        task
    }

    /// セッション保存を 800ms デバウンスで予約する。
    fn schedule_save(&mut self) -> Task<Msg> {
        self.bump_save_seq()
    }

    fn bump_save_seq(&mut self) -> Task<Msg> {
        self.save_seq += 1;
        let seq = self.save_seq;
        Task::future(async move {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            Msg::SessionSaveTick(seq)
        })
    }

    /// is_maximized を確認してから保存する (exit_after = 終了時保存 → その後閉じる)。
    fn query_and_save(&mut self, exit_after: bool) -> Task<Msg> {
        match self.window_id {
            Some(id) => {
                let q = window::is_maximized(id).map(Msg::DoSessionSave);
                if exit_after {
                    q.chain(Task::future(async { Msg::AutoClose }))
                } else {
                    q
                }
            }
            None => {
                self.save_session(false);
                if exit_after {
                    Task::future(async { Msg::AutoClose })
                } else {
                    Task::none()
                }
            }
        }
    }

    fn save_session(&mut self, maximized: bool) {
        // 最大化中は直前の通常時 bounds を保持 (GPUI 版と同じ)
        let bounds = if maximized {
            self.saved_bounds
        } else {
            let p = self.window_pos.unwrap_or(iced::Point::new(100.0, 100.0));
            Some([p.x, p.y, self.window_size.width, self.window_size.height])
        };
        if !maximized {
            self.saved_bounds = bounds;
        }
        let data = fastfiler_core::session::snapshot(&self.model, bounds, maximized);
        fastfiler_core::session::save(&data);
    }

    fn overlay_focus_task(&self) -> Task<Msg> {
        use iced::widget::operation;
        let id = self.model.focused_pane();
        let Some(p) = self.model.panes.get(id) else {
            return Task::none();
        };
        match &p.overlay {
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

    fn run_effects(&mut self, effects: Vec<fastfiler_core::Effect>) -> Task<Msg> {
        use fastfiler_core::Effect;
        if effects.is_empty() {
            return Task::none();
        }
        let known: HashSet<String> = self.icons.keys().cloned().collect();
        let mut tasks = Vec::new();
        for e in effects {
            match e {
                Effect::LoadDir {
                    pane,
                    generation,
                    path,
                } => {
                    // watcher の監視先をこのペインの新フォルダへ追従
                    self.watchers
                        .entry(pane)
                        .or_default()
                        .watch(std::path::Path::new(&path));
                    tasks.push(effects::load_dir_task(
                        pane,
                        generation,
                        path,
                        known.clone(),
                        self.synth,
                    ));
                }
                Effect::PaneClosed(id) => {
                    // watcher 等の付随リソースを解放 (N-02)
                    self.watchers.remove(&id);
                }
                // ジョブ起動: id 採番 → Undo 候補 + 所属ペイン登録 → スレッド起動の順
                // (起動後の登録だと速いジョブの JobDone に追い越されるレース)
                Effect::SpawnJob { pane, op, items } => {
                    let job_id = self.jobs.next_id();
                    self.job_owner.insert(job_id, pane);
                    if op == fastfiler_core::transfer::TransferOp::Move {
                        let undo_items = items
                            .iter()
                            .map(|(from, to)| fastfiler_domain::undo::MoveItem {
                                from: from.clone(),
                                to: to.clone(),
                            })
                            .collect();
                        self.pending_move_undo.insert(job_id, undo_items);
                    }
                    effects::spawn_job(&self.jobs, job_id, op, items);
                }
                Effect::StartSearch { pane, root, query } => {
                    if let Some(job_id) =
                        effects::start_search(&self.search, root, query, self.everything_port)
                    {
                        self.search_owner.insert(job_id, pane);
                        // job_id を core に確定させる (旧検索の遅延イベント混入ガード)。
                        // 同期に反映したいので update_app を直接回す (Effect は出ない)
                        let _ = update_app(
                            &mut self.model,
                            AppMsg::Pane(pane, PaneMsg::SearchStarted(job_id)),
                        );
                    }
                }
                Effect::ShowShellMenu { paths } => {
                    if let Some(hwnd) = self.hwnd {
                        let paths: Vec<String> = paths
                            .iter()
                            .map(|p| p.to_string_lossy().to_string())
                            .collect();
                        // TrackPopupMenu はウィンドウ所有スレッド (=UI) で回す必要がある。
                        // メニュー表示中は自前のメッセージループが回る (GPUI 版と同じ)
                        let _ = fastfiler_domain::shell::show_shell_context_menu(hwnd, paths);
                    }
                }
                Effect::ScheduleSessionSave => {
                    tasks.push(self.bump_save_seq());
                }
                other => tasks.push(effects::run(other, &self.jobs)),
            }
        }
        Task::batch(tasks)
    }
}

/// キー入力 → core メッセージ (固定キー。ホットキー設定は Phase 6)。
fn key_to_msg(key: &Key, m: keyboard::Modifiers) -> Option<Msg> {
    let shift = m.shift();
    let pane = |p: PaneMsg| Some(Msg::Core(AppMsg::Focused(p)));
    let tab = |t: TabMsg| Some(Msg::Core(AppMsg::Tab(t)));
    match key.as_ref() {
        Key::Named(Named::ArrowUp) => pane(PaneMsg::Nav(NavKey::Up, shift)),
        Key::Named(Named::ArrowDown) => pane(PaneMsg::Nav(NavKey::Down, shift)),
        Key::Named(Named::ArrowLeft) if m.alt() => pane(PaneMsg::GoBack),
        Key::Named(Named::ArrowRight) if m.alt() => pane(PaneMsg::GoForward),
        Key::Named(Named::PageUp) => pane(PaneMsg::Nav(NavKey::PageUp, shift)),
        Key::Named(Named::PageDown) => pane(PaneMsg::Nav(NavKey::PageDown, shift)),
        Key::Named(Named::Home) => pane(PaneMsg::Nav(NavKey::Home, shift)),
        Key::Named(Named::End) => pane(PaneMsg::Nav(NavKey::End, shift)),
        Key::Named(Named::Enter) => pane(PaneMsg::ActivateCursor),
        Key::Named(Named::Backspace) => pane(PaneMsg::GoParent),
        Key::Named(Named::Escape) => pane(PaneMsg::ClearSelection),
        Key::Named(Named::F5) => pane(PaneMsg::Reload),
        Key::Named(Named::F2) => pane(PaneMsg::OpenRename),
        Key::Named(Named::F6) => tab(TabMsg::CycleFocus),
        Key::Named(Named::F7) => pane(PaneMsg::OpenNewFolder),
        Key::Named(Named::F8) => pane(PaneMsg::OpenNewFile),
        Key::Named(Named::Delete) => pane(PaneMsg::RequestDelete),
        Key::Named(Named::Tab) if m.control() && m.shift() => tab(TabMsg::Prev),
        Key::Named(Named::Tab) if m.control() => tab(TabMsg::Next),
        // CapsLock ON では "A" が届くため大文字小文字を無視して比較
        Key::Character(c) if m.control() && c.eq_ignore_ascii_case("f") => {
            pane(PaneMsg::OpenSearch)
        }
        Key::Character(c) if m.control() && c.eq_ignore_ascii_case("a") => pane(PaneMsg::SelectAll),
        Key::Character(c) if m.control() && c.eq_ignore_ascii_case("c") => {
            pane(PaneMsg::RequestCopy)
        }
        Key::Character(c) if m.control() && c.eq_ignore_ascii_case("x") => {
            pane(PaneMsg::RequestCut)
        }
        Key::Character(c) if m.control() && c.eq_ignore_ascii_case("v") => {
            pane(PaneMsg::RequestPaste)
        }
        Key::Character(c) if m.control() && c.eq_ignore_ascii_case("z") => Some(Msg::PerformUndo),
        _ => None,
    }
}

pub fn subscription(app: &App) -> Subscription<Msg> {
    let mut subs = vec![
        keyboard::listen().map(Msg::Key),
        window::events().map(|(_id, ev)| Msg::Window(ev)),
        window::open_events().map(Msg::WindowOpened),
        // domain (watcher / ジョブ) イベント: アプリ単一チャネルの受信端
        Subscription::run(domain_events).map(|(e, p)| Msg::Domain(e, p)),
        // マウス第 4/5 ボタン = 戻る/進む (F-303。ウィンドウ全域で有効)
        iced::event::listen_with(|event, _status, _id| match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Back)) => {
                Some(Msg::Core(AppMsg::Focused(PaneMsg::GoBack)))
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Forward)) => {
                Some(Msg::Core(AppMsg::Focused(PaneMsg::GoForward)))
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
    // ---- 縦タブバー (左) ----
    let tabs: Vec<TabItem> = (0..app.model.tabs.len())
        .map(|ix| TabItem {
            title: app.model.tab_title(ix),
            locked: app.model.tabs[ix].locked,
        })
        .collect();
    let tab_bar = TabBar::new(tabs, app.model.active, 1, |ev| {
        Msg::Core(AppMsg::Tab(match ev {
            TabBarEvent::Select(ix) => TabMsg::Select(ix),
            TabBarEvent::Close(ix) => TabMsg::Close(ix),
            TabBarEvent::ToggleLock(ix) => TabMsg::ToggleLock(ix),
            TabBarEvent::Reorder { from, to } => TabMsg::Reorder { from, to },
        }))
    });
    let add_btn = button(text("＋").size(14))
        .style(button::text)
        .padding([2, 10])
        .on_press(Msg::Core(AppMsg::Tab(TabMsg::Add)));
    let tree_btn = button(text("ツリー").size(12))
        .style(button::text)
        .padding([2, 8])
        .on_press(Msg::Core(AppMsg::Tree(TreeMsg::ToggleVisible)));
    let tab_col =
        container(column![row![add_btn, tree_btn].spacing(4), Element::new(tab_bar)].spacing(2))
            .width(Length::Fixed(app.model.tab_width))
            .height(Length::Fill)
            .padding(2);
    // タブバー幅リサイズ (100〜600px)
    let cur_w = app.model.tab_width;
    let tab_w_handle = DragHandle::new(true, 1.0, move |d| {
        Msg::Core(AppMsg::Tab(TabMsg::SetTabWidth(cur_w + d)))
    });

    // ---- ペイン領域 (BSP 再帰) ----
    let tab = app.model.active_tab();
    let tree_w = if app.model.tree.visible {
        app.model.tree.width + 6.0
    } else {
        0.0
    };
    let pane_area_w = (app.window_size.width - app.model.tab_width - 6.0 - tree_w).max(100.0);
    let pane_area_h = (app.window_size.height - 24.0).max(100.0);
    let pane_area = render_node(
        app,
        &tab.root,
        Size::new(pane_area_w, pane_area_h),
        tab.root.leaves().len() > 1,
    );

    // ---- ワークスペースツリー (表示中のみ) ----
    let main: Element<'_, Msg> = if app.model.tree.visible {
        let tree_list = TreeList::new(&app.model.tree, |ev| match ev {
            TreeEvent::Open(p) => Msg::Core(AppMsg::Focused(PaneMsg::NavigateTo(p))),
            TreeEvent::Toggle(p) => Msg::Core(AppMsg::Tree(TreeMsg::Toggle(p))),
            TreeEvent::RemoveServer(sv) => Msg::Core(AppMsg::Tree(TreeMsg::RemoveServer(sv))),
            TreeEvent::Scrolled(o) => Msg::Core(AppMsg::Tree(TreeMsg::Scrolled(o))),
            TreeEvent::ViewportChanged { height } => {
                Msg::Core(AppMsg::Tree(TreeMsg::ViewportChanged { height }))
            }
        });
        let tree_col = container(Element::new(tree_list))
            .width(Length::Fixed(app.model.tree.width))
            .height(Length::Fill)
            .padding(2);
        let cur_tw = app.model.tree.width;
        let tree_handle = DragHandle::new(true, 1.0, move |d| {
            Msg::Core(AppMsg::Tree(TreeMsg::SetWidth(cur_tw + d)))
        });
        row![tab_col, tab_w_handle, tree_col, tree_handle, pane_area].into()
    } else {
        row![tab_col, tab_w_handle, pane_area].into()
    };
    let root = column![main, container(text("FastFiler").size(11)).padding([2, 8]),];

    // ---- モーダル / 衝突ダイアログ (フォーカスペインの overlay を stack で合成) ----
    let focused = app.model.focused_pane();
    match app.model.panes.get(focused).and_then(|p| p.overlay.clone()) {
        Some(Overlay::DropMenu { items, at, .. }) => {
            let pane = focused;
            let menu = ContextMenu::new(items, at, vec![], move |ev| match ev {
                MenuEvent::Clicked(path) => {
                    Msg::Core(AppMsg::Pane(pane, PaneMsg::MenuClicked(path)))
                }
                MenuEvent::Close => Msg::Core(AppMsg::Pane(pane, PaneMsg::MenuClose)),
            });
            stack![root, Element::new(menu)].into()
        }
        Some(Overlay::ContextMenu {
            items,
            at,
            open_path,
            ..
        }) => {
            let pane = focused;
            let menu = ContextMenu::new(items, at, open_path, move |ev| match ev {
                MenuEvent::Clicked(path) => {
                    Msg::Core(AppMsg::Pane(pane, PaneMsg::MenuClicked(path)))
                }
                MenuEvent::Close => Msg::Core(AppMsg::Pane(pane, PaneMsg::MenuClose)),
            });
            stack![root, Element::new(menu)].into()
        }
        Some(Overlay::Modal { kind, value }) => {
            let (title, placeholder) = match kind {
                ModalKind::Rename { .. } => ("名前の変更", "新しい名前"),
                ModalKind::NewFolder => ("新しいフォルダ", "フォルダ名"),
                ModalKind::NewFile => ("新しいファイル", "ファイル名"),
            };
            let card = dialog_card(
                title.to_string(),
                column![
                    text_input(placeholder, &value)
                        .id(modal_input_id())
                        .on_input(|v| Msg::Core(AppMsg::Focused(PaneMsg::ModalInput(v))))
                        .on_submit(Msg::Core(AppMsg::Focused(PaneMsg::ModalCommit)))
                        .size(14)
                        .padding(6),
                    row![
                        button(text("OK").size(13))
                            .padding([3, 14])
                            .on_press(Msg::Core(AppMsg::Focused(PaneMsg::ModalCommit))),
                        button(text("キャンセル").size(13))
                            .padding([3, 14])
                            .on_press(Msg::Core(AppMsg::Focused(PaneMsg::ModalCancel))),
                    ]
                    .spacing(8),
                ]
                .spacing(10)
                .into(),
            );
            stack![root, card].into()
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
            let choice = |c: ConflictChoice| Msg::Core(AppMsg::Focused(PaneMsg::Conflict(c)));
            let card = dialog_card(
                "同名のファイルがあります".to_string(),
                column![
                    text(detail).size(13),
                    text("(複数件には一括で適用されます)").size(12),
                    row![
                        button(text("上書き").size(13))
                            .padding([3, 12])
                            .on_press(choice(ConflictChoice::Overwrite)),
                        button(text("別名で保存").size(13))
                            .padding([3, 12])
                            .on_press(choice(ConflictChoice::RenameBoth)),
                        button(text("キャンセル").size(13))
                            .padding([3, 12])
                            .on_press(choice(ConflictChoice::Cancel)),
                    ]
                    .spacing(8),
                ]
                .spacing(10)
                .into(),
            );
            stack![root, card].into()
        }
        _ => root.into(),
    }
}

/// BSP を再帰描画する。`size` は px→比率換算のための概算サイズ。
fn render_node<'a>(app: &'a App, node: &'a PaneNode, size: Size, multi: bool) -> Element<'a, Msg> {
    match node {
        PaneNode::Leaf(id) => pane_view(app, *id, multi),
        PaneNode::Split {
            id,
            dir,
            ratios,
            children,
        } => {
            let split_id = *id;
            let vertical = *dir == SplitDir::Row; // Row = 左右に並ぶ = 縦の仕切り
            let total_px = if vertical { size.width } else { size.height };
            let min_ratio = (PANE_MIN_PX / total_px.max(1.0)).min(0.45);
            let mut items: Vec<Element<'a, Msg>> = Vec::new();
            for (i, child) in children.iter().enumerate() {
                let r = ratios
                    .get(i)
                    .copied()
                    .unwrap_or(1.0 / children.len() as f32);
                let child_size = if vertical {
                    Size::new(size.width * r, size.height)
                } else {
                    Size::new(size.width, size.height * r)
                };
                if i > 0 {
                    let handle_ix = i - 1;
                    let per_px = 1.0 / total_px.max(1.0);
                    items.push(
                        DragHandle::new(vertical, per_px, move |d| {
                            Msg::Core(AppMsg::Tab(TabMsg::Resize {
                                split_id,
                                handle_ix,
                                delta_ratio: d,
                                min_ratio,
                            }))
                        })
                        .into(),
                    );
                }
                let child_el = render_node(app, child, child_size, multi);
                let portion = (r * 1000.0) as u16;
                items.push(
                    container(child_el)
                        .width(if vertical {
                            Length::FillPortion(portion.max(1))
                        } else {
                            Length::Fill
                        })
                        .height(if vertical {
                            Length::Fill
                        } else {
                            Length::FillPortion(portion.max(1))
                        })
                        .into(),
                );
            }
            if vertical {
                iced::widget::Row::with_children(items)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            } else {
                iced::widget::Column::with_children(items)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
        }
    }
}

/// 1 ペインの描画: ヘッダ (パスバー + ↔ ↕ ×) + 一覧 + フッタ。
/// フォーカスペインには青枠 (F-204)。
fn pane_view(app: &App, id: PaneId, multi: bool) -> Element<'_, Msg> {
    let Some(p) = app.model.panes.get(id) else {
        return text("").into();
    };
    let focused = app.model.focused_pane() == id;

    // パスバー (フォーカスペインのみ PathEdit を input にする)
    let path_bar: Element<'_, Msg> = match &p.overlay {
        Some(Overlay::PathEdit { value }) if focused => text_input("パスを入力…", value)
            .id(path_input_id())
            .on_input(|v| Msg::Core(AppMsg::Focused(PaneMsg::PathEditInput(v))))
            .on_submit(Msg::Core(AppMsg::Focused(PaneMsg::PathEditCommit)))
            .size(13)
            .padding([1, 6])
            .into(),
        _ => button(text(p.cur_path.to_string_lossy().to_string()).size(13))
            .style(button::text)
            .padding([1, 6])
            .width(Length::Fill)
            .on_press(Msg::Core(AppMsg::Pane(id, PaneMsg::OpenPathEdit)))
            .into(),
    };
    let hbtn = |label: &'static str, msg: Msg| {
        button(text(label).size(12))
            .style(button::text)
            .padding([1, 6])
            .on_press(msg)
    };
    let header = row![
        hbtn("↑", Msg::Core(AppMsg::Pane(id, PaneMsg::GoParent))),
        path_bar,
        hbtn(
            "↔",
            Msg::Core(AppMsg::Tab(TabMsg::SplitFocused(SplitDir::Row)))
        ),
        hbtn(
            "↕",
            Msg::Core(AppMsg::Tab(TabMsg::SplitFocused(SplitDir::Column)))
        ),
        hbtn("×", Msg::Core(AppMsg::Tab(TabMsg::ClosePane(id)))),
    ]
    .spacing(2)
    .align_y(iced::Alignment::Center);

    // 検索バー (Ctrl+F — 一覧は表示されたまま、Enter で結果リストに切替)
    let search_bar: Option<Element<'_, Msg>> = p.search.as_ref().map(|sui| {
        let summary: Element<'_, Msg> = if sui.running {
            text("検索中…").size(12).into()
        } else if let Some(s) = &sui.summary {
            text(s.clone()).size(12).into()
        } else {
            text("").size(12).into()
        };
        row![
            text_input("検索 (Everything / 内蔵)…", &sui.query)
                .id(search_input_id())
                .on_input(move |v| Msg::Core(AppMsg::Pane(id, PaneMsg::SearchInput(v))))
                .on_submit(Msg::Core(AppMsg::Pane(id, PaneMsg::SearchCommit)))
                .size(13)
                .padding([2, 6]),
            summary,
            button(text("×").size(13))
                .style(button::text)
                .padding([1, 6])
                .on_press(Msg::Core(AppMsg::Pane(id, PaneMsg::SearchClose))),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .into()
    });

    // 検索の結果リスト表示中は entries を hits に差し替え (F-701)
    let drag_active = app.model.drag.is_some();
    let drop_highlight = app
        .model
        .drag
        .as_ref()
        .and_then(|d| d.over)
        .filter(|(pane, _)| *pane == id)
        .and_then(|(_, row)| row);
    let showing_results = p.search.as_ref().is_some_and(|s| s.showing);
    let list: Element<'_, Msg> = if showing_results {
        let sui = p.search.as_ref().unwrap();
        FileList::new(p, &app.icons, move |ev| Msg::List(id, ev))
            .entries_override(&sui.hits, p.load_gen.wrapping_add(1_000_000))
            .into()
    } else {
        FileList::new(p, &app.icons, move |ev| Msg::List(id, ev))
            .drag_context(drag_active, drop_highlight)
            .into()
    };

    // フッタ: ジョブ進捗 > 一時メッセージ > 件数/選択数
    let footer: Element<'_, Msg> = if let Some(job) = &p.job {
        let label = match job.kind.as_str() {
            "move" => "移動中",
            "delete" => "削除中",
            _ => "コピー中",
        };
        let current = std::path::Path::new(&job.current)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| job.current.clone());
        row![
            text(format!(
                "{label}: {}/{} — {current}",
                job.done_files, job.total_files
            ))
            .size(12)
            .width(Length::Fill),
            button(text("キャンセル").size(11))
                .padding([1, 8])
                .on_press(Msg::Core(AppMsg::Pane(id, PaneMsg::CancelJobRequest))),
        ]
        .spacing(8)
        .into()
    } else {
        let status = if p.loading {
            "読み込み中…".to_string()
        } else if let Some(e) = &p.load_error {
            format!("エラー: {e}")
        } else if let Some(m) = &p.status_msg {
            m.clone()
        } else if p.selected.is_empty() {
            format!("{} 項目", p.entries.len())
        } else {
            format!("{} 項目 / {} 個を選択", p.entries.len(), p.selected.len())
        };
        row![text(status).size(12)].into()
    };

    let mut body = column![container(header).padding([2, 4])];
    if let Some(bar) = search_bar {
        body = body.push(container(bar).padding([2, 4]));
    }
    let body = body.push(list).push(container(footer).padding([2, 6]));
    // フォーカスペインの青枠 (複数ペイン時のみ強調 — GPUI と同じ視覚言語)
    let border_color = if focused && multi {
        iced::Color::from_rgb(0.25, 0.55, 1.0)
    } else {
        iced::Color::TRANSPARENT
    };
    container(body)
        .style(move |_| container::Style {
            border: iced::Border {
                color: border_color,
                width: if focused && multi { 2.0 } else { 0.0 },
                radius: 2.0.into(),
            },
            ..container::Style::default()
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// 中央寄せのダイアログカード (背景を薄く暗くする)。
fn dialog_card(title: String, body: Element<'_, Msg>) -> Element<'_, Msg> {
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

// FileList の HEADER_H / TabBar の CELL_H は将来のレイアウト計算用に re-export
#[allow(unused)]
const _: (f32, f32) = (HEADER_H, CELL_H);
