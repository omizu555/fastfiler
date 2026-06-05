//! Phase 3: アプリのルート。縦タブ + タブ内 BSP 分割ペイン + ドラッグリサイズ。
//!
//! `FastFilerApp` が複数タブを保持し、各タブはペインの **BSP ツリー** (`PaneNode`) を持つ。
//! 各 leaf は `Entity<PaneView>`。ペインは `PaneEvent` (Activated / SplitRequested /
//! CloseRequested) を emit し、`FastFilerApp` が購読してツリーを操作する。
//!
//! **リサイズ**: 分割境界のハンドルを GPUI 標準のドラッグ機構 (`on_drag` +
//! `on_drag_move`) で実装。`DragMoveEvent` が listener 要素 (split コンテナ) の
//! 実寸 bounds を渡してくれるため、マウス位置→比率の変換が直接できる
//! (参考: zed/crates/editor/src/split_editor_view.rs)。
//!
//! **メモリ目標 (本移植の主目的)**: タブ/ペインを閉じると `Entity<PaneView>` と購読が
//! drop され、`PaneView::drop` を起点に watcher / sink / spawn ループまで連鎖解放。
//! `live panes` 表示でベースラインへ戻ることを確認できる。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use gpui::{
    AnyElement, App, Context, DragMoveEvent, Empty, Entity, EntityId, FocusHandle, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, SharedString, Subscription, Window, div, prelude::*,
    px,
};

use crate::pane::{PANES_ALIVE, PaneEvent, PaneView, SplitDir};
use crate::session::{self, NodeData, SessionData};
use crate::settings_store;
use crate::text_input::TextInput;
use crate::theme::{self, th};
use crate::tree::{TreeEvent, TreeView};
use crate::hotkeys;

/// ペインの最小サイズ (px)。リサイズ時にこれ未満へは縮めない。
const MIN_PANE_PX: f32 = 80.0;

/// リサイズハンドルのドラッグペイロード。
/// `split_id` の Split 内、children[ix] と children[ix+1] の境界を表す。
struct DraggedHandle {
    split_id: u64,
    ix: usize,
}

/// ワークスペースツリーパネル幅のドラッグペイロード。
struct DraggedTreeHandle;

/// タブ内のペイン配置を表す BSP ツリー。
enum PaneNode {
    Leaf(Entity<PaneView>),
    Split {
        /// リサイズドラッグの対象特定用の安定 id。
        id: u64,
        dir: SplitDir,
        /// 各 child の比率 (合計 1.0)。
        ratios: Vec<f32>,
        children: Vec<PaneNode>,
    },
}

/// 1 タブ: ペインツリー + フォーカス中ペイン + 各ペインの購読。
struct TabState {
    root: PaneNode,
    focused: Option<EntityId>,
    /// ペイン id → (イベント購読, 変化観測)。閉じる際に drop して解放する。
    subs: HashMap<EntityId, (Subscription, Subscription)>,
}

impl TabState {
    fn focused_pane(&self) -> Option<&Entity<PaneView>> {
        match self.focused {
            Some(id) => find_pane(&self.root, id).or_else(|| first_pane(&self.root)),
            None => first_pane(&self.root),
        }
    }

    fn title(&self, cx: &App) -> String {
        self.focused_pane()
            .map(|p| p.read(cx).title())
            .unwrap_or_else(|| "—".to_string())
    }

    fn leaf_count(&self) -> usize {
        count_leaves(&self.root)
    }
}

pub struct FastFilerApp {
    tabs: Vec<TabState>,
    active: usize,
    next_split_id: u64,
    /// セッション保存のデバウンスフラグ。
    save_pending: bool,
    /// ワークスペースツリー (ドライブ起点)。クリックでフォーカスペインに開く。
    tree: Entity<TreeView>,
    _tree_sub: Subscription,
    show_tree: bool,
    /// ツリーパネル幅 (px、ドラッグで変更・保存)。
    tree_width: f32,
    /// 最新のウィンドウ位置/サイズ [x,y,w,h] (render で取得・保存)。
    window_bounds: Option<[f32; 4]>,
    /// 次の render でこのペインへキーボードフォーカスを移す (F6/タブ切替)。
    pending_focus: Option<EntityId>,
    /// 設定画面 (⚙)。Some = 表示中 (Everything ポート入力欄を保持)。
    settings_open: Option<Entity<TextInput>>,
    /// 設定画面の Esc/Enter 用フォーカス。
    settings_focus: FocusHandle,
}

impl FastFilerApp {
    /// ツリーパネルを生成し OpenDir イベントを購読する (両コンストラクタ共通)。
    fn make_tree(cx: &mut Context<Self>) -> (Entity<TreeView>, Subscription) {
        let tree = cx.new(|_| TreeView::new());
        let sub = cx.subscribe(&tree, |this, _tree, event: &TreeEvent, cx| match event {
            TreeEvent::OpenDir(path) => this.open_in_focused_pane(path.clone(), cx),
            // UNC 登録の変化 (削除等) はセッション保存。
            TreeEvent::UncChanged => this.schedule_save(cx),
        });
        (tree, sub)
    }

    pub fn new(start: PathBuf, cx: &mut Context<Self>) -> Self {
        let (tree, tree_sub) = Self::make_tree(cx);
        let mut this = Self {
            tabs: Vec::new(),
            active: 0,
            next_split_id: 1,
            save_pending: false,
            tree,
            _tree_sub: tree_sub,
            show_tree: true,
            tree_width: 220.0,
            window_bounds: None,
            pending_focus: None,
            settings_open: None,
            settings_focus: cx.focus_handle(),
        };
        this.register_quit_hook(cx);
        this.add_tab_at(start, cx);
        this
    }

    /// 保存済みセッション (タブ / 分割構成) から復元する。
    pub fn from_session(data: SessionData, cx: &mut Context<Self>) -> Self {
        let (tree, tree_sub) = Self::make_tree(cx);
        // 登録済み UNC share を復元 (サーバが落ちていてもツリー UI は壊れない)。
        if !data.unc_shares.is_empty() {
            let shares = data.unc_shares.clone();
            tree.update(cx, |t, _| t.set_unc_shares(shares));
        }
        let mut this = Self {
            tabs: Vec::new(),
            active: 0,
            next_split_id: 1,
            save_pending: false,
            tree,
            _tree_sub: tree_sub,
            show_tree: data.show_tree,
            tree_width: data.tree_width.clamp(120.0, 480.0),
            window_bounds: None,
            pending_focus: None,
            settings_open: None,
            settings_focus: cx.focus_handle(),
        };
        this.register_quit_hook(cx);
        for tab in &data.tabs {
            let mut subs = HashMap::new();
            let mut focused = None;
            let root = this.build_node(tab, &mut subs, &mut focused, cx);
            let focused = focused.or_else(|| first_pane(&root).map(|p| p.entity_id()));
            this.tabs.push(TabState {
                root,
                focused,
                subs,
            });
        }
        if this.tabs.is_empty() {
            this.add_tab_at(default_start(), cx);
        } else {
            this.active = data.active.min(this.tabs.len() - 1);
        }
        cx.notify();
        this
    }

    /// セッションデータからペインツリーを再構築する。
    /// 壊れたデータ (存在しないフォルダ / 子 0 の Split / 比率不正) は安全側に補正。
    fn build_node(
        &mut self,
        nd: &NodeData,
        subs: &mut HashMap<EntityId, (Subscription, Subscription)>,
        focused: &mut Option<EntityId>,
        cx: &mut Context<Self>,
    ) -> PaneNode {
        match nd {
            NodeData::Leaf { path, focused: f } => {
                let p = PathBuf::from(path);
                let start = if p.is_dir() { p } else { default_start() };
                let (pane, ev, ob) = Self::make_pane(start, cx);
                let id = pane.entity_id();
                subs.insert(id, (ev, ob));
                if *f && focused.is_none() {
                    *focused = Some(id);
                }
                PaneNode::Leaf(pane)
            }
            NodeData::Split {
                dir,
                ratios,
                children,
            } => {
                let mut kids: Vec<PaneNode> = children
                    .iter()
                    .map(|c| self.build_node(c, subs, focused, cx))
                    .collect();
                if kids.is_empty() {
                    let (pane, ev, ob) = Self::make_pane(default_start(), cx);
                    let id = pane.entity_id();
                    subs.insert(id, (ev, ob));
                    return PaneNode::Leaf(pane);
                }
                if kids.len() == 1 {
                    return kids.pop().unwrap();
                }
                let n = kids.len();
                let mut r = ratios.clone();
                let sum: f32 = r.iter().sum();
                if r.len() != n || !(0.5..=1.5).contains(&sum) {
                    r = vec![1.0 / n as f32; n];
                }
                let id = self.next_split_id;
                self.next_split_id += 1;
                let dir = if dir == "column" {
                    SplitDir::Column
                } else {
                    SplitDir::Row
                };
                PaneNode::Split {
                    id,
                    dir,
                    ratios: r,
                    children: kids,
                }
            }
        }
    }

    fn register_quit_hook(&self, cx: &mut Context<Self>) {
        cx.on_app_quit(|this, cx| {
            this.save_session(cx);
            async {}
        })
        .detach();
    }

    /// 現在の構成をセッションとして保存する。
    fn save_session(&self, cx: &App) {
        session::save(&SessionData {
            active: self.active,
            show_tree: self.show_tree,
            tree_width: self.tree_width,
            window: self.window_bounds,
            unc_shares: self.tree.read(cx).unc_shares().to_vec(),
            // テーマは gpui_settings.json へ移行済み (読み込み時の互換用に残置)。
            theme: None,
            tabs: self
                .tabs
                .iter()
                .map(|t| node_data(&t.root, t.focused, cx))
                .collect(),
        });
    }

    // ── 設定画面 (⚙) ──────────────────────────────────────────────

    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| TextInput::new(cx));
        let port = settings_store::get().everything_port.to_string();
        let len = port.len();
        input.update(cx, |i, cx| i.set_text_and_select(port, 0..len, cx));
        self.settings_open = Some(input);
        self.settings_focus.focus(window, cx);
        cx.notify();
    }

    fn close_settings(&mut self, cx: &mut Context<Self>) {
        if self.settings_open.take().is_some() {
            cx.notify();
        }
    }

    /// Everything ポート入力を適用 (u16 にパースできた場合のみ保存)。
    fn apply_settings_port(&mut self, cx: &mut Context<Self>) {
        let Some(input) = &self.settings_open else {
            return;
        };
        let text = input.read(cx).text().trim().to_string();
        if let Ok(port) = text.parse::<u16>() {
            if port > 0 {
                settings_store::update(|s| s.everything_port = port);
                cx.notify();
            }
        }
    }

    /// テーマをプリセット名で適用し、設定へ保存して全ビューを再描画。
    fn set_theme(&mut self, name: &'static str, cx: &mut Context<Self>) {
        theme::set_by_name(name);
        settings_store::update(|s| s.theme = Some(name.to_string()));
        self.refresh_all(cx);
    }

    /// 設定画面のオーバーレイ (開いていなければ None)。
    fn render_settings(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let input = self.settings_open.as_ref()?;

        let current_theme = th().name;
        let theme_btns: Vec<AnyElement> = theme::presets()
            .iter()
            .map(|t| {
                let name = t.name;
                let active = name == current_theme;
                div()
                    .id(SharedString::from(format!("th-{name}")))
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if active { th().sel_bg } else { th().button_bg })
                    .hover(|s| s.bg(th().button_hover))
                    .child(name)
                    .on_click(cx.listener(move |this, _e, _w, cx| this.set_theme(name, cx)))
                    .into_any_element()
            })
            .collect();

        let action_btn = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .px_3()
                .py_1()
                .rounded_md()
                .cursor_pointer()
                .bg(th().button_bg)
                .hover(|s| s.bg(th().button_hover))
                .child(label)
        };
        let sep = || div().h(px(1.0)).bg(th().separator);
        let section = |label: &'static str| div().text_color(th().text_dim).child(label);

        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .bg(th().overlay_bg)
                .track_focus(&self.settings_focus)
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _w, cx| {
                    match ev.keystroke.key.as_str() {
                        "escape" => this.close_settings(cx),
                        "enter" => this.apply_settings_port(cx),
                        _ => {}
                    }
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _e: &MouseDownEvent, _w, cx| {
                        this.close_settings(cx);
                        cx.stop_propagation();
                    }),
                )
                .child(
                    div()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_e, _w, cx| cx.stop_propagation())
                        .flex()
                        .flex_col()
                        .gap_3()
                        .w(px(520.0))
                        .p_4()
                        .rounded_md()
                        .bg(th().surface_bg)
                        .border_1()
                        .border_color(th().accent)
                        .text_color(th().text)
                        // タイトル行
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .child(div().flex_1().text_color(th().text_bright).child("設定"))
                                .child(
                                    div()
                                        .id("st-close")
                                        .px_2()
                                        .rounded_sm()
                                        .cursor_pointer()
                                        .hover(|s| s.bg(th().button_bg))
                                        .child("×")
                                        .on_click(
                                            cx.listener(|this, _e, _w, cx| this.close_settings(cx)),
                                        ),
                                ),
                        )
                        // 外観
                        .child(section("外観 — テーマ"))
                        .child(div().flex().flex_row().gap_2().children(theme_btns))
                        .child(sep())
                        // ホットキー
                        .child(section("ホットキー (gpui_hotkeys.json)"))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap_2()
                                .child(action_btn("st-hk-open", "設定ファイルを開く").on_click(
                                    cx.listener(|_t, _e, _w, _cx| {
                                        if let Some(f) = hotkeys::config_file() {
                                            let _ = fastfiler_domain::shell::open_with_shell(f);
                                        }
                                    }),
                                ))
                                .child(action_btn("st-hk-reload", "再読み込み").on_click(
                                    cx.listener(|_t, _e, _w, _cx| hotkeys::load()),
                                )),
                        )
                        .child(sep())
                        // フォルダ
                        .child(section("フォルダを開く"))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap_2()
                                .child(action_btn("st-tpl", "テンプレート").on_click(
                                    cx.listener(|this, _e, _w, cx| {
                                        if let Ok(d) =
                                            fastfiler_domain::templates::templates_dir()
                                        {
                                            this.close_settings(cx);
                                            this.open_in_focused_pane(PathBuf::from(d), cx);
                                        }
                                    }),
                                ))
                                .child(action_btn("st-cmd", "ユーザーコマンド").on_click(
                                    cx.listener(|this, _e, _w, cx| {
                                        if let Ok(d) =
                                            fastfiler_domain::user_commands::user_commands_dir()
                                        {
                                            this.close_settings(cx);
                                            this.open_in_focused_pane(PathBuf::from(d), cx);
                                        }
                                    }),
                                )),
                        )
                        .child(sep())
                        // 検索
                        .child(section("検索 — Everything HTTP ポート"))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap_2()
                                .items_center()
                                .child(div().w(px(140.0)).child(input.clone()))
                                .child(action_btn("st-port", "適用 (Enter)").on_click(
                                    cx.listener(|this, _e, _w, cx| this.apply_settings_port(cx)),
                                ))
                                .child(div().text_color(th().text_faint).child(
                                    SharedString::from(format!(
                                        "現在: {}",
                                        settings_store::get().everything_port
                                    )),
                                )),
                        ),
                )
                .into_any_element(),
        )
    }

    /// テーマ変更などで全ペイン + ツリーを再描画する。
    fn refresh_all(&mut self, cx: &mut Context<Self>) {
        let mut panes = Vec::new();
        for t in &self.tabs {
            collect_pane_entities(&t.root, &mut panes);
        }
        for p in panes {
            p.update(cx, |_, cx| cx.notify());
        }
        self.tree.update(cx, |_, cx| cx.notify());
        cx.notify();
    }

    /// アクティブタブのフォーカスペインにフォルダを開く (ツリーからの遷移先)。
    fn open_in_focused_pane(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(pane) = self
            .tabs
            .get(self.active)
            .and_then(|t| t.focused_pane())
            .cloned()
        else {
            return;
        };
        pane.update(cx, |p, cx| p.open_dir(path, cx));
    }

    fn toggle_tree(&mut self, cx: &mut Context<Self>) {
        self.show_tree = !self.show_tree;
        cx.notify();
        self.schedule_save(cx);
    }

    /// 800ms デバウンス付きでセッション保存を予約する。
    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        if self.save_pending {
            return;
        }
        self.save_pending = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(800))
                .await;
            let _ = this.update(cx, |app, cx| {
                app.save_pending = false;
                app.save_session(cx);
            });
        })
        .detach();
    }

    /// 新しいペインを生成し、イベント購読 + 変化観測を張って返す。
    fn make_pane(
        start: PathBuf,
        cx: &mut Context<Self>,
    ) -> (Entity<PaneView>, Subscription, Subscription) {
        let pane = cx.new(|cx| PaneView::new(start, cx));
        let ev = cx.subscribe(&pane, |this, emitter, event: &PaneEvent, cx| {
            let id = emitter.entity_id();
            match event {
                PaneEvent::Activated => this.set_focus(id, cx),
                PaneEvent::SplitRequested(dir) => this.split_pane(id, *dir, cx),
                PaneEvent::CloseRequested => this.close_pane(id, cx),
                PaneEvent::FocusNextPane => this.cycle_focus(cx),
                PaneEvent::SwitchTab(delta) => this.switch_tab_relative(*delta, cx),
            }
        });
        // ペイン内の変化 (フォルダ移動等) でタブ見出し等を更新 + セッション保存予約。
        // UNC パスを開いたらワークスペースツリーへ自動登録 (CONTEXT.md)。
        let ob = cx.observe(&pane, |this, pane, cx| {
            cx.notify();
            let p = pane.read(cx).cur_path().to_path_buf();
            if p.to_string_lossy().starts_with(r"\\") {
                this.tree.update(cx, |t, cx| {
                    t.register_unc(&p, cx);
                });
            }
            this.schedule_save(cx);
        });
        (pane, ev, ob)
    }

    fn add_tab_at(&mut self, start: PathBuf, cx: &mut Context<Self>) {
        let (pane, ev, ob) = Self::make_pane(start, cx);
        let id = pane.entity_id();
        let mut subs = HashMap::new();
        subs.insert(id, (ev, ob));
        self.tabs.push(TabState {
            root: PaneNode::Leaf(pane),
            focused: Some(id),
            subs,
        });
        self.active = self.tabs.len() - 1;
        cx.notify();
        self.schedule_save(cx);
    }

    fn add_tab(&mut self, cx: &mut Context<Self>) {
        let start = self
            .tabs
            .get(self.active)
            .and_then(|t| t.focused_pane())
            .map(|p| p.read(cx).cur_path().to_path_buf())
            .unwrap_or_else(default_start);
        self.add_tab_at(start, cx);
    }

    fn close_tab(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix >= self.tabs.len() || self.tabs.len() <= 1 {
            return; // 最後の 1 枚は残す
        }
        // TabState を drop → ツリー内の全 Entity<PaneView> と購読が連鎖 drop。
        self.tabs.remove(ix);
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
        cx.notify();
        self.schedule_save(cx);
    }

    fn select_tab(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix < self.tabs.len() {
            self.active = ix;
            // タブ切替後はそのタブのフォーカスペインへキーボードを移す。
            self.pending_focus = self.tabs[ix].focused;
            cx.notify();
            self.schedule_save(cx);
        }
    }

    /// タブを相対移動 (Ctrl+Tab / Ctrl+Shift+Tab)。
    fn switch_tab_relative(&mut self, delta: i32, cx: &mut Context<Self>) {
        let n = self.tabs.len() as i32;
        if n == 0 {
            return;
        }
        let next = ((self.active as i32 + delta) % n + n) % n;
        self.select_tab(next as usize, cx);
    }

    /// アクティブタブ内で次のペインへフォーカスを回す (F6)。
    fn cycle_focus(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let mut leaves = Vec::new();
        collect_leaves(&tab.root, &mut leaves);
        if leaves.is_empty() {
            return;
        }
        let cur = tab
            .focused
            .and_then(|f| leaves.iter().position(|&id| id == f))
            .unwrap_or(0);
        let next = leaves[(cur + 1) % leaves.len()];
        tab.focused = Some(next);
        self.pending_focus = Some(next);
        cx.notify();
    }

    /// ツリーパネル幅のドラッグ。
    fn on_tree_handle_drag(&mut self, e: &DragMoveEvent<DraggedTreeHandle>, cx: &mut Context<Self>) {
        let w = (e.event.position.x - e.bounds.origin.x) / px(1.0);
        self.tree_width = w.clamp(120.0, 480.0);
        cx.notify();
        self.schedule_save(cx);
    }

    fn tab_index_of(&self, pane: EntityId) -> Option<usize> {
        self.tabs
            .iter()
            .position(|t| find_pane(&t.root, pane).is_some())
    }

    fn set_focus(&mut self, pane: EntityId, cx: &mut Context<Self>) {
        if let Some(ti) = self.tab_index_of(pane) {
            if self.tabs[ti].focused != Some(pane) {
                self.tabs[ti].focused = Some(pane);
                cx.notify();
            }
        }
    }

    fn split_pane(&mut self, target: EntityId, dir: SplitDir, cx: &mut Context<Self>) {
        let Some(ti) = self.tab_index_of(target) else {
            return;
        };
        // 新ペインは分割元と同じフォルダで開く。
        let start = find_pane(&self.tabs[ti].root, target)
            .map(|p| p.read(cx).cur_path().to_path_buf())
            .unwrap_or_else(default_start);
        let (pane, ev, ob) = Self::make_pane(start, cx);
        let new_id = pane.entity_id();
        let split_id = self.next_split_id;
        self.next_split_id += 1;
        if split_node(&mut self.tabs[ti].root, target, dir, pane, split_id) {
            self.tabs[ti].subs.insert(new_id, (ev, ob));
            self.tabs[ti].focused = Some(new_id);
        }
        cx.notify();
        self.schedule_save(cx);
    }

    fn close_pane(&mut self, target: EntityId, cx: &mut Context<Self>) {
        let Some(ti) = self.tab_index_of(target) else {
            return;
        };
        if self.tabs[ti].leaf_count() <= 1 {
            return; // タブ内最後の 1 ペインは残す
        }
        if remove_node(&mut self.tabs[ti].root, target) {
            // 購読を drop → Entity<PaneView> も他に参照が無くなり drop (watcher 等解放)。
            self.tabs[ti].subs.remove(&target);
            if self.tabs[ti].focused == Some(target) {
                self.tabs[ti].focused = first_pane(&self.tabs[ti].root).map(|p| p.entity_id());
            }
            cx.notify();
            self.schedule_save(cx);
        }
    }

    /// リサイズハンドルのドラッグ移動。split コンテナの実寸 bounds から
    /// マウス位置を比率へ変換し、境界両側の比率を更新する。
    fn on_handle_drag(
        &mut self,
        split_id: u64,
        e: &DragMoveEvent<DraggedHandle>,
        cx: &mut Context<Self>,
    ) {
        // ペイロードを先にコピーして cx の借用を解放。
        let (p_sid, ix) = {
            let p = e.drag(cx);
            (p.split_id, p.ix)
        };
        // ネストした split の親コンテナにもイベントが来るため、自分宛てのみ処理。
        if p_sid != split_id {
            return;
        }

        let Some((ratios, dir)) = self
            .tabs
            .iter_mut()
            .find_map(|t| find_split_mut(&mut t.root, split_id))
        else {
            return;
        };
        if ix + 1 >= ratios.len() {
            return;
        }

        let (origin, size, mouse) = match dir {
            SplitDir::Row => (e.bounds.origin.x, e.bounds.size.width, e.event.position.x),
            SplitDir::Column => (e.bounds.origin.y, e.bounds.size.height, e.event.position.y),
        };
        if size <= px(0.) {
            return;
        }

        // 境界の目標位置 (コンテナ内 0..1)
        let t = (mouse - origin) / size;
        let before: f32 = ratios[..ix].iter().sum();
        let pair = ratios[ix] + ratios[ix + 1];
        let min = (px(MIN_PANE_PX) / size).min(pair / 2.0);
        let a = (t - before).clamp(min, pair - min);
        ratios[ix] = a;
        ratios[ix + 1] = pair - a;
        cx.notify();
        self.schedule_save(cx);
    }

    /// ペインツリーを再帰描画。フォーカス leaf は枠線ハイライト、
    /// 分割境界にはドラッグハンドルを挟む。
    fn render_node(
        &self,
        node: &PaneNode,
        focused: Option<EntityId>,
        cx: &Context<Self>,
    ) -> AnyElement {
        match node {
            PaneNode::Leaf(pane) => {
                let is_focused = focused == Some(pane.entity_id());
                div()
                    .size_full()
                    .border_1()
                    .border_color(if is_focused {
                        th().accent
                    } else {
                        th().border_dim
                    })
                    .child(pane.clone())
                    .into_any_element()
            }
            PaneNode::Split {
                id,
                dir,
                ratios,
                children,
            } => {
                let sid = *id;
                let mut container = div()
                    .id(SharedString::from(format!("split-{sid}")))
                    .flex()
                    .size_full()
                    // ハンドルのドラッグ中、このコンテナの bounds 付きで move が届く。
                    .on_drag_move(cx.listener(
                        move |this, e: &DragMoveEvent<DraggedHandle>, _w, cx| {
                            this.on_handle_drag(sid, e, cx);
                        },
                    ));
                container = match dir {
                    SplitDir::Row => container.flex_row(),
                    SplitDir::Column => container.flex_col(),
                };
                for (i, child) in children.iter().enumerate() {
                    if i > 0 {
                        container = container.child(render_handle(sid, i - 1, *dir));
                    }
                    let ratio = ratios.get(i).copied().unwrap_or(1.0);
                    container = container.child(
                        div()
                            .flex_grow(ratio)
                            .flex_basis(px(0.))
                            .overflow_hidden()
                            .child(self.render_node(child, focused, cx)),
                    );
                }
                container.into_any_element()
            }
        }
    }
}

impl Render for FastFilerApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // F6 / タブ切替で予約されたキーボードフォーカス移動を実行。
        if let Some(id) = self.pending_focus.take() {
            if let Some(pane) = self
                .tabs
                .get(self.active)
                .and_then(|t| find_pane(&t.root, id))
            {
                let fh = pane.read(cx).focus_handle.clone();
                fh.focus(window, cx);
            }
        }

        // ウィンドウ位置/サイズを記録 (変化時のみ保存予約)。
        {
            let wb = window.bounds();
            let arr = [
                wb.origin.x / px(1.0),
                wb.origin.y / px(1.0),
                wb.size.width / px(1.0),
                wb.size.height / px(1.0),
            ];
            if self.window_bounds != Some(arr) {
                let first = self.window_bounds.is_none();
                self.window_bounds = Some(arr);
                if !first {
                    self.schedule_save(cx);
                }
            }
        }

        let active = self.active;
        let alive = PANES_ALIVE.load(Ordering::Relaxed);

        let titles: Vec<(usize, String)> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| (i, t.title(cx)))
            .collect();

        let tab_items: Vec<_> = titles
            .into_iter()
            .map(|(i, title)| {
                let is_active = i == active;
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .id(SharedString::from(format!("tab-{i}")))
                            .flex_1()
                            .overflow_hidden()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(if is_active { th().sel_bg } else { th().header_bg })
                            .hover(|s| s.bg(th().hover_bg))
                            .text_color(th().text)
                            .child(title)
                            .on_click(cx.listener(move |this, _e, _w, cx| this.select_tab(i, cx))),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("close-{i}")))
                            .px_1()
                            .rounded_md()
                            .cursor_pointer()
                            .text_color(th().text_dim)
                            .hover(|s| s.bg(th().danger_bg).text_color(th().text_bright))
                            .child("×")
                            .on_click(cx.listener(move |this, _e, _w, cx| this.close_tab(i, cx))),
                    )
            })
            .collect();

        let body = self.render_node(&self.tabs[active].root, self.tabs[active].focused, cx);

        // ワークスペースツリーパネル + 幅リサイズハンドル (表示中のみ)。
        let tree_panel = if self.show_tree {
            Some(
                div()
                    .w(px(self.tree_width))
                    .flex_shrink_0()
                    .h_full()
                    .bg(th().tree_bg)
                    .child(self.tree.clone()),
            )
        } else {
            None
        };
        let tree_handle = if self.show_tree {
            Some(
                div()
                    .id("tree-rh")
                    .flex_shrink_0()
                    .w(px(5.0))
                    .h_full()
                    .bg(th().handle_bg)
                    .hover(|s| s.bg(th().handle_hover))
                    .cursor_col_resize()
                    .on_drag(DraggedTreeHandle, |_, _, _, cx| cx.new(|_| Empty)),
            )
        } else {
            None
        };
        let show_tree = self.show_tree;

        // フッター (左: タイトル / 右端: ⚙ 設定)
        let footer = div()
            .flex()
            .flex_row()
            .items_center()
            .flex_shrink_0()
            .gap_2()
            .px_3()
            .h(px(34.0))
            .bg(th().tab_bar_bg)
            .border_t_1()
            .border_color(th().separator)
            .child(div().text_color(th().text_soft).child("FastFiler"))
            .child(div().flex_1())
            .child(
                div()
                    .id("open-settings")
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(th().surface_bg)
                    .hover(|s| s.bg(th().button_bg))
                    .text_color(th().text_soft)
                    .child("⚙ 設定")
                    .on_click(
                        cx.listener(|this, _e, window, cx| this.open_settings(window, cx)),
                    ),
            );

        div()
            .flex()
            .flex_col()
            .size_full()
            .relative()
            .bg(th().app_bg)
            .child(
                div().flex_1().flex().flex_row().overflow_hidden()
            // 左: 縦タブバー
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(px(200.0))
                    .h_full()
                    .bg(th().tab_bar_bg)
                    .p_1()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_1()
                            .child(
                                div()
                                    .id("add-tab")
                                    .flex_1()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(th().surface_bg)
                                    .hover(|s| s.bg(th().button_bg))
                                    .text_color(th().text_soft)
                                    .child("＋ 新規タブ")
                                    .on_click(cx.listener(|this, _e, _w, cx| this.add_tab(cx))),
                            )
                            .child(
                                div()
                                    .id("toggle-tree")
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(if show_tree { th().sel_bg } else { th().surface_bg })
                                    .hover(|s| s.bg(th().menu_hover))
                                    .text_color(th().text_soft)
                                    .child("ツリー")
                                    .on_click(cx.listener(|this, _e, _w, cx| this.toggle_tree(cx))),
                            ),
                    )
                    .children(tab_items)
                    .child(div().flex_1())
                    // デバッグ: 生存ペイン数 (タブ/ペイン開閉でベースラインへ戻るかの可視化)
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .text_color(th().text_disabled)
                            .child(SharedString::from(format!("live panes: {alive}"))),
                    ),
            )
            // 中央〜右: ツリー (トグル) + リサイズハンドル + ペイン群
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    // ツリー幅ハンドルのドラッグ移動 (bounds 起点で幅算出)。
                    .on_drag_move(cx.listener(
                        |this, e: &DragMoveEvent<DraggedTreeHandle>, _w, cx| {
                            this.on_tree_handle_drag(e, cx);
                        },
                    ))
                    .children(tree_panel)
                    .children(tree_handle)
                    .child(div().flex_1().h_full().overflow_hidden().child(body)),
            ),
            )
            .child(footer)
            // 設定画面 (開いている時のみ)
            .children(self.render_settings(cx))
    }
}

/// 分割境界のドラッグハンドル。
fn render_handle(split_id: u64, ix: usize, dir: SplitDir) -> AnyElement {
    let base = div()
        .id(SharedString::from(format!("rh-{split_id}-{ix}")))
        .flex_shrink_0()
        .bg(th().handle_bg)
        .hover(|s| s.bg(th().handle_hover))
        .on_drag(DraggedHandle { split_id, ix }, |_, _, _, cx| {
            cx.new(|_| Empty)
        });
    match dir {
        SplitDir::Row => base.w(px(5.0)).h_full().cursor_col_resize(),
        SplitDir::Column => base.h(px(5.0)).w_full().cursor_row_resize(),
    }
    .into_any_element()
}

/// ペインツリー → セッションデータ (保存用)。
fn node_data(node: &PaneNode, focused: Option<EntityId>, cx: &App) -> NodeData {
    match node {
        PaneNode::Leaf(p) => NodeData::Leaf {
            path: p.read(cx).cur_path().to_string_lossy().to_string(),
            focused: focused == Some(p.entity_id()),
        },
        PaneNode::Split {
            dir,
            ratios,
            children,
            ..
        } => NodeData::Split {
            dir: match dir {
                SplitDir::Row => "row".to_string(),
                SplitDir::Column => "column".to_string(),
            },
            ratios: ratios.clone(),
            children: children
                .iter()
                .map(|c| node_data(c, focused, cx))
                .collect(),
        },
    }
}

// ── ツリー操作ヘルパ ──────────────────────────────────────────────

fn find_pane(node: &PaneNode, id: EntityId) -> Option<&Entity<PaneView>> {
    match node {
        PaneNode::Leaf(e) => (e.entity_id() == id).then_some(e),
        PaneNode::Split { children, .. } => children.iter().find_map(|c| find_pane(c, id)),
    }
}

fn first_pane(node: &PaneNode) -> Option<&Entity<PaneView>> {
    match node {
        PaneNode::Leaf(e) => Some(e),
        PaneNode::Split { children, .. } => children.iter().find_map(first_pane),
    }
}

fn count_leaves(node: &PaneNode) -> usize {
    match node {
        PaneNode::Leaf(_) => 1,
        PaneNode::Split { children, .. } => children.iter().map(count_leaves).sum(),
    }
}

/// ツリー内の全ペイン Entity を集める (テーマ変更時の一括再描画用)。
fn collect_pane_entities(node: &PaneNode, out: &mut Vec<Entity<PaneView>>) {
    match node {
        PaneNode::Leaf(e) => out.push(e.clone()),
        PaneNode::Split { children, .. } => {
            for c in children {
                collect_pane_entities(c, out);
            }
        }
    }
}

/// ツリー順に leaf の EntityId を集める (F6 のフォーカス巡回用)。
fn collect_leaves(node: &PaneNode, out: &mut Vec<EntityId>) {
    match node {
        PaneNode::Leaf(e) => out.push(e.entity_id()),
        PaneNode::Split { children, .. } => {
            for c in children {
                collect_leaves(c, out);
            }
        }
    }
}

/// split_id の Split を探し、(比率, 方向) を返す。
fn find_split_mut(node: &mut PaneNode, id: u64) -> Option<(&mut Vec<f32>, SplitDir)> {
    match node {
        PaneNode::Leaf(_) => None,
        PaneNode::Split {
            id: sid,
            dir,
            ratios,
            children,
        } => {
            if *sid == id {
                Some((ratios, *dir))
            } else {
                children.iter_mut().find_map(|c| find_split_mut(c, id))
            }
        }
    }
}

/// target の leaf を見つけ、2 分割 (old, new) に置き換える。
fn split_node(
    node: &mut PaneNode,
    target: EntityId,
    dir: SplitDir,
    new_pane: Entity<PaneView>,
    split_id: u64,
) -> bool {
    match node {
        PaneNode::Leaf(e) if e.entity_id() == target => {
            // node を空 Split に差し替えつつ元の Leaf を取り出す。
            let old = std::mem::replace(
                node,
                PaneNode::Split {
                    id: split_id,
                    dir,
                    ratios: vec![0.5, 0.5],
                    children: Vec::new(),
                },
            );
            if let PaneNode::Split { children, .. } = node {
                children.push(old);
                children.push(PaneNode::Leaf(new_pane));
            }
            true
        }
        PaneNode::Leaf(_) => false,
        PaneNode::Split { children, .. } => {
            let idx = children.iter().position(|c| find_pane(c, target).is_some());
            match idx {
                Some(i) => split_node(&mut children[i], target, dir, new_pane, split_id),
                None => false,
            }
        }
    }
}

/// target の leaf を削除し、子が 1 つになった Split は畳む。
/// 単独 Leaf (タブ最後のペイン) は削除しない (false)。
fn remove_node(node: &mut PaneNode, target: EntityId) -> bool {
    let PaneNode::Split {
        children, ratios, ..
    } = node
    else {
        return false;
    };
    let direct = children
        .iter()
        .position(|c| matches!(c, PaneNode::Leaf(e) if e.entity_id() == target));
    if let Some(pos) = direct {
        children.remove(pos);
        // 対応する比率も除去して残りを正規化。
        if pos < ratios.len() {
            ratios.remove(pos);
        }
        let sum: f32 = ratios.iter().sum();
        if sum > 0.0 {
            for r in ratios.iter_mut() {
                *r /= sum;
            }
        }
    } else {
        let mut removed = false;
        for c in children.iter_mut() {
            if remove_node(c, target) {
                removed = true;
                break;
            }
        }
        if !removed {
            return false;
        }
    }
    // 子が 1 つだけになったら自分をその子で置き換える (collapse)。
    if children.len() == 1 {
        let only = children.pop().unwrap();
        *node = only;
    }
    true
}

/// 起点フォルダ (ユーザープロファイル、取得失敗時は C:\)。
pub fn default_start() -> PathBuf {
    let s = fastfiler_domain::fs::home_dir().unwrap_or_else(|_| "C:\\".to_string());
    PathBuf::from(s)
}
