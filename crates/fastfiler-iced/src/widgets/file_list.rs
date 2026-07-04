//! FileList — 可視範囲だけを直描きする仮想リスト (計画書 §6、GPUI uniform_list の代替)。
//!
//! 設計:
//! - 行を子ウィジェットにしない。可視行だけを `draw` で直接描画する
//!   (Phase 0 S-2 スパイクで 10 万行 60fps を実証済み)。
//! - 状態 (entries / 選択 / ソート / 列幅 / スクロール) は core の `PaneState` が所有し、
//!   本ウィジェットは**借用して描くだけ**。入力は `ListEvent` として publish し、
//!   解釈 (選択・活性化・ナビゲーション) は core の update に任せる。
//! - 列構成: 名前 (残り幅を吸収) | 更新日時 | サイズ | 種類。仕切りは各列の**左端**にあり、
//!   右端基準で幅を逆算する (GPUI 版 pane.rs on_col_handle_drag と同一)。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use fastfiler_core::model::{Column, Entry, SortState};
use iced::advanced::image::Renderer as _;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::Quad;
use iced::advanced::text::{
    Alignment as TextAlignment, LineHeight, Renderer as _, Shaping, Text, Wrapping,
};
use iced::advanced::widget::{tree, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::alignment::Vertical;
use iced::mouse::{self, ScrollDelta};
use iced::widget::image;
use iced::{Border, Color, Element, Event, Length, Pixels, Point, Rectangle, Shadow, Size, Theme};

/// ヘッダ行の高さ。
pub const HEADER_H: f32 = 26.0;
/// アイコン描画サイズ。
const ICON: f32 = 16.0;
/// 列仕切りのつかみ判定幅 (±px)。
const GRIP: f32 = 4.0;
/// ダブルクリック判定間隔。
const DOUBLE_CLICK: Duration = Duration::from_millis(500);
/// 列間ギャップ (GPUI 版 gap_2 相当)。
const GAP: f32 = 8.0;
/// 右端パディング。
const PAD_R: f32 = 8.0;
/// 左パディング。
const PAD_L: f32 = 8.0;

/// FileList が発行するイベント。解釈は core (update_pane) が行う。
#[derive(Debug, Clone)]
pub enum ListEvent {
    RowPressed {
        ix: usize,
        ctrl: bool,
        shift: bool,
    },
    RowDoubleClicked {
        ix: usize,
    },
    BlankPressed,
    HeaderClicked(Column),
    ColResized {
        col: Column,
        width: f32,
    },
    Scrolled(f32),
    ViewportChanged {
        height: f32,
    },
    /// 右クリック (行 / 背景)。pos はウィンドウ座標 (メニュー表示位置)。
    RowRightClicked {
        ix: usize,
        x: f32,
        y: f32,
    },
    BlankRightClicked {
        x: f32,
        y: f32,
    },
    // ---- 内部 D&D (F-601/F-605) ----
    /// 行ドラッグ開始 (閾値超)。
    DragStarted {
        ix: usize,
        right_button: bool,
    },
    /// ドラッグ中のホバー (このペイン上。row = カーソル下の行)。
    DragHover {
        row: Option<usize>,
    },
    /// このペインへのドロップ。pos はメニュー表示用ウィンドウ座標。
    DragDropped {
        row: Option<usize>,
        x: f32,
        y: f32,
    },
    /// 行ドラッグ中にカーソルがウィンドウ外へ出た (外部への OLE 送信起点 — F-603)。
    DragExitedWindow,
    /// 一覧部分のウィンドウ内矩形 (外部 D&D のペインヒットテスト用 — F-602)。
    BoundsChanged {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
}

/// ウィジェット内部状態 (フレーム間で保持したい非・アプリ状態のみ)。
#[derive(Default)]
struct State {
    /// (時刻, 行, 一覧の世代)。世代が変わったクリックはダブルクリックにしない
    /// (別フォルダへ移動した直後の同座標クリックが誤爆する事故の防止)。
    last_click: Option<(Instant, usize, u64)>,
    /// ドラッグ中の列仕切り。
    dragging: Option<Column>,
    /// 行の押下 (D&D 開始判定)。(開始位置, 行, 右ボタンか)。
    row_pressed: Option<(Point, usize, bool)>,
    /// このペイン発の行ドラッグが進行中か (DragStarted 発行済み)。
    row_dragging: bool,
    /// 通知済みの一覧矩形。
    notified_rect: Option<Rectangle>,
    /// 通知したときのペイン (タブ切替で同じ位置に別ペインが来たら再通知する)。
    notified_pane: u64,
    /// 通知済みのビューポート高 (変化時のみ publish)。
    notified_h: f32,
}

pub struct FileList<'a, Message> {
    entries: &'a [Entry],
    icons: &'a HashMap<String, image::Handle>,
    cursor: Option<usize>,
    selected: &'a std::collections::BTreeSet<usize>,
    sort: SortState,
    col_widths: [f32; 3],
    offset: f32,
    row_h: f32,
    /// 一覧の世代 (pane.load_gen)。ダブルクリック判定の同一性に使う。
    list_gen: u64,
    /// このリストが表示しているペインのトークン (タブ切替の検知)。
    pane_token: u64,
    /// アプリ全体で内部 D&D が進行中か (ドロップ受け入れモード)。
    drag_active: bool,
    /// ドロップ先ハイライト行 (core drag.over から)。
    drop_highlight: Option<usize>,
    on_event: Box<dyn Fn(ListEvent) -> Message + 'a>,
}

impl<'a, Message> FileList<'a, Message> {
    pub fn new(
        pane: &'a fastfiler_core::PaneState,
        icons: &'a HashMap<String, image::Handle>,
        on_event: impl Fn(ListEvent) -> Message + 'a,
    ) -> Self {
        Self {
            entries: &pane.entries,
            icons,
            cursor: pane.cursor,
            selected: &pane.selected,
            sort: pane.sort,
            col_widths: pane.col_widths,
            offset: pane.scroll_offset,
            row_h: pane.row_h,
            list_gen: pane.load_gen,
            pane_token: 0,
            drag_active: false,
            drop_highlight: None,
            on_event: Box::new(on_event),
        }
    }

    /// 内部 D&D の受け入れモード (ドラッグ中のみ true。ハイライト行は core から)。
    pub fn drag_context(mut self, active: bool, highlight: Option<usize>) -> Self {
        self.drag_active = active;
        self.drop_highlight = highlight;
        self
    }

    /// 表示中ペインのトークン (タブ切替時に矩形通知を再発行するための識別)。
    pub fn pane_token(mut self, token: u64) -> Self {
        self.pane_token = token;
        self
    }

    /// 検索結果リスト表示 (F-701): entries を差し替える。
    /// 世代も切り替えてダブルクリック判定の取り違えを防ぐ。
    pub fn entries_override(mut self, entries: &'a [Entry], generation: u64) -> Self {
        self.entries = entries;
        self.list_gen = generation;
        self
    }

    fn max_offset(&self, list_h: f32) -> f32 {
        (self.entries.len() as f32 * self.row_h - list_h).max(0.0)
    }

    /// 各列の x 座標 (左端)。戻り値: [名前, 更新日時, サイズ, 種類] と右端。
    ///
    /// 通常は右端基準 (GPUI 版と同じ)。ウィンドウが狭く名前列が潰れる場合は
    /// 名前列の最小幅を確保して右へ押し出す (はみ出しは clip される)。
    /// これをしないと名前が非表示になり、ヘッダクリックも誤判定になる。
    fn col_x(&self, bounds: &Rectangle) -> ([f32; 4], f32) {
        const NAME_MIN: f32 = 80.0;
        let right = bounds.x + bounds.width - PAD_R;
        let name_x = bounds.x + PAD_L;
        let [dw, sw, tw] = self.col_widths;
        let date_x = (right - tw - GAP - sw - GAP - dw).max(name_x + NAME_MIN + GAP);
        let size_x = date_x + dw + GAP;
        let type_x = size_x + sw + GAP;
        ([name_x, date_x, size_x, type_x], right)
    }

    /// ヘッダ上の x 座標から仕切り (列) を判定。仕切りは各列の左端。
    fn grip_at(&self, bounds: &Rectangle, x: f32) -> Option<Column> {
        let ([_, date_x, size_x, type_x], _) = self.col_x(bounds);
        if (x - date_x).abs() <= GRIP {
            Some(Column::Modified)
        } else if (x - size_x).abs() <= GRIP {
            Some(Column::Size)
        } else if (x - type_x).abs() <= GRIP {
            Some(Column::Type)
        } else {
            None
        }
    }

    /// ヘッダ上の x 座標からクリック対象の列を判定。
    fn header_col_at(&self, bounds: &Rectangle, x: f32) -> Column {
        let ([_, date_x, size_x, type_x], _) = self.col_x(bounds);
        if x >= type_x {
            Column::Type
        } else if x >= size_x {
            Column::Size
        } else if x >= date_x {
            Column::Modified
        } else {
            Column::Name
        }
    }

    /// ドラッグ中の仕切り位置 x から新しい列幅を逆算 (GPUI 版と同一の右端基準)。
    fn resize_width(&self, bounds: &Rectangle, col: Column, x: f32) -> f32 {
        let right = bounds.x + bounds.width - PAD_R;
        let [dw, sw, tw] = self.col_widths;
        match col {
            Column::Type => right - x,
            Column::Size => right - tw - GAP - x,
            Column::Modified => right - tw - GAP - sw - GAP - x,
            Column::Name => dw, // 到達しない
        }
    }

    fn list_bounds(&self, bounds: Rectangle) -> Rectangle {
        Rectangle {
            y: bounds.y + HEADER_H,
            height: (bounds.height - HEADER_H).max(0.0),
            ..bounds
        }
    }

    fn row_at(&self, list: &Rectangle, pos: Point) -> Option<usize> {
        if pos.y < list.y {
            return None;
        }
        let ix = ((pos.y - list.y + self.offset) / self.row_h) as usize;
        (ix < self.entries.len()).then_some(ix)
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for FileList<'_, Message> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        let list = self.list_bounds(bounds);

        // ビューポート高の変化を core へ (PageUp/Dn・ensure_visible 用)
        if (list.height - state.notified_h).abs() > 0.5 {
            state.notified_h = list.height;
            shell.publish((self.on_event)(ListEvent::ViewportChanged {
                height: list.height,
            }));
        }
        // 一覧矩形の変化を App へ (外部 D&D のヒットテスト表)。
        // 矩形が同じでも「別のペインに切り替わった」ときは再通知する
        // (widget state は画面位置ベースで引き継がれるため — タブ切替で
        //  通知が止まり、ドロップ先の解決が古いペインのままになるバグの修正)
        if state.notified_pane != self.pane_token
            || state
                .notified_rect
                .map(|r| {
                    (r.x - list.x).abs() > 0.5
                        || (r.y - list.y).abs() > 0.5
                        || (r.width - list.width).abs() > 0.5
                        || (r.height - list.height).abs() > 0.5
                })
                .unwrap_or(true)
        {
            state.notified_rect = Some(list);
            state.notified_pane = self.pane_token;
            shell.publish((self.on_event)(ListEvent::BoundsChanged {
                x: list.x,
                y: list.y,
                w: list.width,
                h: list.height,
            }));
        }

        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta })
                if cursor.position_over(bounds).is_some() =>
            {
                let dy = match delta {
                    ScrollDelta::Lines { y, .. } => -y * self.row_h * 3.0,
                    ScrollDelta::Pixels { y, .. } => -y,
                };
                let new = (self.offset + dy).clamp(0.0, self.max_offset(list.height));
                if (new - self.offset).abs() > f32::EPSILON {
                    shell.publish((self.on_event)(ListEvent::Scrolled(new)));
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(pos) = cursor.position_over(bounds) else {
                    return;
                };
                if pos.y < bounds.y + HEADER_H {
                    // ヘッダ: 仕切り優先、外れたらソートクリック
                    if let Some(col) = self.grip_at(&bounds, pos.x) {
                        state.dragging = Some(col);
                    } else {
                        shell.publish((self.on_event)(ListEvent::HeaderClicked(
                            self.header_col_at(&bounds, pos.x),
                        )));
                    }
                    shell.capture_event();
                    return;
                }
                match self.row_at(&list, pos) {
                    Some(ix) => {
                        let now = Instant::now();
                        let double = matches!(
                            state.last_click,
                            Some((t, last_ix, g))
                                if last_ix == ix && g == self.list_gen && now - t <= DOUBLE_CLICK
                        );
                        if double {
                            state.last_click = None;
                            shell.publish((self.on_event)(ListEvent::RowDoubleClicked { ix }));
                        } else {
                            state.last_click = Some((now, ix, self.list_gen));
                            state.row_pressed = cursor.position().map(|p| (p, ix, false));
                            // 修飾キーはイベントに含まれないため winit 経由の
                            // keyboard::Event::ModifiersChanged を App が保持して
                            // core へ渡す方式もあるが、iced 0.14 はカーソルイベントに
                            // modifiers を持たない。App 側で追跡した修飾キーを
                            // ListEvent へ焼き込むため、ここでは押下だけ通知する。
                            shell.publish((self.on_event)(ListEvent::RowPressed {
                                ix,
                                ctrl: false,
                                shift: false,
                            }));
                        }
                    }
                    None => {
                        shell.publish((self.on_event)(ListEvent::BlankPressed));
                    }
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                let Some(pos) = cursor.position_over(bounds) else {
                    return;
                };
                if pos.y >= bounds.y + HEADER_H {
                    // メニューは Released で出す (右ボタンドラッグと区別するため —
                    // 閾値を超えたらドラッグ、超えなければメニュー)
                    match self.row_at(&list, pos) {
                        // 左ドラッグ進行中の右押下は無視 (ボタン種別の取り違え防止)
                        Some(_) if state.row_pressed.is_some() => {}
                        Some(ix) => {
                            state.row_pressed = Some((pos, ix, true));
                        }
                        None => shell.publish((self.on_event)(ListEvent::BlankRightClicked {
                            x: pos.x,
                            y: pos.y,
                        })),
                    }
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right))
                if state.row_pressed.is_some() || self.drag_active =>
            {
                // 発生元ペインでの「ドラッグに至らない右クリック」= メニュー。
                // (drag_active 中は全ペインが Released を受けるため、メニュー判定は
                //  押下を記録した発生元かつ非ドラッグ時のみ)
                if let Some((pos, ix, true)) = state.row_pressed.take() {
                    if !state.row_dragging && !self.drag_active {
                        shell.publish((self.on_event)(ListEvent::RowRightClicked {
                            ix,
                            x: pos.x,
                            y: pos.y,
                        }));
                        shell.capture_event();
                        return;
                    }
                }
                // 右ボタンドラッグのドロップ: 左と同じく「アプリ全体のドラッグ状態」で
                // 判定する — 発生元の state だけ見ると分割相手のペインで離したとき
                // 誰も反応しない (実機報告「分割時の右ドラッグ無反応」の原因)
                let was_dragging = state.row_dragging;
                state.row_dragging = false;
                if self.drag_active || was_dragging {
                    if let Some(now) = cursor.position() {
                        if cursor.position_over(bounds).is_some() {
                            let row = self.row_at(&list, now);
                            shell.publish((self.on_event)(ListEvent::DragDropped {
                                row,
                                x: now.x,
                                y: now.y,
                            }));
                            shell.capture_event();
                        }
                    }
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. })
                if state.row_pressed.is_some() || self.drag_active =>
            {
                if let (Some((start, ix, right)), Some(now)) =
                    (state.row_pressed, cursor.position())
                {
                    if !state.row_dragging && now.distance(start) > 6.0 {
                        state.row_dragging = true;
                        shell.publish((self.on_event)(ListEvent::DragStarted {
                            ix,
                            right_button: right,
                        }));
                    }
                }
                // ドラッグ受け入れモード: 自ペイン上のホバー行を通知
                if self.drag_active && cursor.position_over(bounds).is_some() {
                    if let Some(pos) = cursor.position() {
                        let row = self.row_at(&list, pos);
                        shell.publish((self.on_event)(ListEvent::DragHover { row }));
                    }
                }
            }
            Event::Mouse(mouse::Event::CursorLeft) if state.row_pressed.is_some() => {
                // 閾値未満のままウィンドウ外へ出た場合も押下状態を破棄する
                // (外での Released は届かないため、残すと幽霊ドラッグが始まる)
                let was_dragging = state.row_dragging;
                state.row_dragging = false;
                state.row_pressed = None;
                if was_dragging {
                    shell.publish((self.on_event)(ListEvent::DragExitedWindow));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.dragging.is_some() => {
                if let (Some(col), Some(pos)) = (state.dragging, cursor.position()) {
                    let width = self.resize_width(&bounds, col, pos.x);
                    shell.publish((self.on_event)(ListEvent::ColResized { col, width }));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.dragging.is_some() =>
            {
                state.dragging = None;
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.row_pressed.is_some() || self.drag_active =>
            {
                state.row_pressed = None;
                let was_dragging = state.row_dragging;
                state.row_dragging = false;
                if self.drag_active || was_dragging {
                    if let Some(pos) = cursor.position() {
                        if cursor.position_over(bounds).is_some() {
                            let row = self.row_at(&list, pos);
                            shell.publish((self.on_event)(ListEvent::DragDropped {
                                row,
                                x: pos.x,
                                y: pos.y,
                            }));
                            shell.capture_event();
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if state.dragging.is_some() {
            return mouse::Interaction::ResizingHorizontally;
        }
        let bounds = layout.bounds();
        if let Some(pos) = cursor.position_over(bounds) {
            if pos.y < bounds.y + HEADER_H && self.grip_at(&bounds, pos.x).is_some() {
                return mouse::Interaction::ResizingHorizontally;
            }
        }
        mouse::Interaction::None
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;

        let bounds = layout.bounds();
        let list = self.list_bounds(bounds);
        let palette = theme.extended_palette();
        let text_color = style.text_color;
        let dim = Color {
            a: 0.65,
            ..text_color
        };
        let ([name_x, date_x, size_x, type_x], right) = self.col_x(&bounds);

        let cell = |content: String, size: f32| Text {
            content,
            bounds: Size::new(f32::MAX, self.row_h),
            size: Pixels(size),
            line_height: LineHeight::default(),
            font: crate::theme::ui_font(),
            align_x: TextAlignment::Left,
            align_y: Vertical::Center,
            shaping: Shaping::Advanced,
            wrapping: Wrapping::None,
        };
        // 列のクリップ矩形 (幅を列に制限して `…` 相当の切り落とし)。
        // widget 境界とも交差させ、最下行の部分表示がフッタへはみ出さないようにする
        // (start_layer によるクリップはテキストアトラスと干渉して文字化けするため
        //  使わない — 実機で 1 行目の文字化け・重なりを確認済み)
        let clip = move |x: f32, w: f32, y: f32, h: f32| {
            Rectangle {
                x,
                y,
                width: w.max(0.0),
                height: h,
            }
            .intersection(&bounds)
            .unwrap_or(Rectangle {
                x,
                y,
                width: 0.0,
                height: 0.0,
            })
        };

        // ---- ヘッダ ----
        renderer.fill_quad(
            Quad {
                bounds: Rectangle {
                    height: HEADER_H,
                    ..bounds
                },
                border: Border::default(),
                shadow: Shadow::default(),
                snap: true,
            },
            palette.background.weak.color,
        );
        let arrow = |col: Column| {
            if self.sort.col == col {
                if self.sort.asc {
                    " ▲"
                } else {
                    " ▼"
                }
            } else {
                ""
            }
        };
        let hy = bounds.y + HEADER_H / 2.0;
        let headers = [
            (name_x, date_x - GAP - name_x, "名前", Column::Name),
            (date_x, size_x - GAP - date_x, "更新日時", Column::Modified),
            (size_x, type_x - GAP - size_x, "サイズ", Column::Size),
            (type_x, right - type_x, "種類", Column::Type),
        ];
        for (x, w, label, col) in headers {
            renderer.fill_text(
                cell(format!("{label}{}", arrow(col)), 13.0),
                Point::new(x, hy),
                dim,
                clip(x, w, bounds.y, HEADER_H),
            );
        }
        // 仕切り線
        for x in [date_x, size_x, type_x] {
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle {
                        x: x - GAP / 2.0,
                        y: bounds.y + 4.0,
                        width: 1.0,
                        height: HEADER_H - 8.0,
                    },
                    border: Border::default(),
                    shadow: Shadow::default(),
                    snap: true,
                },
                palette.background.strong.color,
            );
        }

        // ---- 行 (可視範囲のみ) ----
        let offset = self.offset.clamp(0.0, self.max_offset(list.height));
        let first = (offset / self.row_h) as usize;
        let visible = (list.height / self.row_h).ceil() as usize + 1;
        let last = (first + visible).min(self.entries.len());

        for ix in first..last {
            let entry = &self.entries[ix];
            let y = list.y + (ix as f32 * self.row_h - offset);
            let is_sel = self.selected.contains(&ix);
            let is_cur = self.cursor == Some(ix);
            let is_drop = self.drop_highlight == Some(ix);

            // 選択=強 / カーソル=弱 / 縞 (GPUI 版の優先順位と同じ)
            let row_bg = if is_drop {
                Some(palette.success.weak.color)
            } else if is_sel {
                Some(if is_cur {
                    palette.primary.strong.color
                } else {
                    palette.primary.base.color
                })
            } else if is_cur {
                Some(palette.background.strong.color)
            } else if ix % 2 == 1 {
                Some(Color {
                    a: 0.03,
                    ..palette.background.base.text
                })
            } else {
                None
            };
            if let Some(bg) = row_bg {
                let row_rect = Rectangle {
                    x: list.x,
                    y,
                    width: list.width,
                    height: self.row_h,
                };
                if let Some(visible) = row_rect.intersection(&bounds) {
                    renderer.fill_quad(
                        Quad {
                            bounds: visible,
                            border: Border::default(),
                            shadow: Shadow::default(),
                            snap: true,
                        },
                        bg,
                    );
                }
            }
            let fg = if is_sel {
                palette.primary.base.text
            } else {
                text_color
            };
            let fg_dim = Color { a: 0.75, ..fg };
            let cy = y + self.row_h / 2.0;

            // アイコン
            if let Some(handle) = self.icons.get(&entry.icon_key) {
                renderer.draw_image(
                    iced::advanced::image::Image {
                        handle: handle.clone(),
                        filter_method: image::FilterMethod::Linear,
                        rotation: iced::Radians(0.0),
                        opacity: 1.0,
                        snap: true,
                        border_radius: 0.0.into(),
                    },
                    Rectangle {
                        x: name_x,
                        y: cy - ICON / 2.0,
                        width: ICON,
                        height: ICON,
                    },
                    list,
                );
            }
            // 名前 / 更新日時 / サイズ / 種類
            let name_text_x = name_x + ICON + 6.0;
            renderer.fill_text(
                cell(entry.name.clone(), 14.0),
                Point::new(name_text_x, cy),
                fg,
                clip(name_text_x, date_x - GAP - name_text_x, y, self.row_h),
            );
            renderer.fill_text(
                cell(entry.modified_text.clone(), 13.0),
                Point::new(date_x, cy),
                fg_dim,
                clip(date_x, size_x - GAP - date_x, y, self.row_h),
            );
            renderer.fill_text(
                cell(entry.size_text.clone(), 13.0),
                Point::new(size_x, cy),
                fg_dim,
                clip(size_x, type_x - GAP - size_x, y, self.row_h),
            );
            renderer.fill_text(
                cell(entry.kind_text.clone(), 13.0),
                Point::new(type_x, cy),
                fg_dim,
                clip(type_x, right - type_x, y, self.row_h),
            );
        }

        // ---- スクロールバー ----
        let total = self.entries.len() as f32 * self.row_h;
        if total > list.height && list.height > 0.0 {
            let thumb_h = (list.height / total * list.height).max(24.0);
            let thumb_y = list.y + (offset / (total - list.height)) * (list.height - thumb_h);
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle {
                        x: list.x + list.width - 8.0,
                        y: thumb_y,
                        width: 6.0,
                        height: thumb_h,
                    },
                    border: Border {
                        radius: 3.0.into(),
                        ..Border::default()
                    },
                    shadow: Shadow::default(),
                    snap: true,
                },
                Color {
                    a: 0.35,
                    ..palette.background.base.text
                },
            );
        }
    }
}

impl<'a, Message: 'a> From<FileList<'a, Message>> for Element<'a, Message, Theme, iced::Renderer> {
    fn from(w: FileList<'a, Message>) -> Self {
        Element::new(w)
    }
}
