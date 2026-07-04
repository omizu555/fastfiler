//! 縦タブバー (F-101〜F-106)。直描きのカスタムウィジェット。
//!
//! - クリック = 選択 / 中クリック = ロック切替 / × = 閉じる (ロック中は非表示)
//! - 左ドラッグ (閾値超) = 並べ替え。ドロップ先はセル中心で判定し、離した時に
//!   `Reorder { from, to }` を publish
//! - 列数 1〜4 (行優先 — USAGE.md §2)

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::Quad;
use iced::advanced::text::{
    Alignment as TextAlignment, LineHeight, Renderer as _, Shaping, Text, Wrapping,
};
use iced::advanced::widget::{tree, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::alignment::Vertical;
use iced::mouse;
use iced::{Border, Color, Element, Event, Length, Pixels, Point, Rectangle, Shadow, Size, Theme};

pub const CELL_H: f32 = 30.0;
const CLOSE_W: f32 = 22.0;
const DRAG_THRESHOLD: f32 = 6.0;

#[derive(Debug, Clone)]
pub enum TabBarEvent {
    Select(usize),
    Close(usize),
    ToggleLock(usize),
    Reorder { from: usize, to: usize },
}

/// 1 タブの表示情報 (App の view が AppModel から作る)。
#[derive(Debug, Clone)]
pub struct TabItem {
    pub title: String,
    pub locked: bool,
}

pub struct TabBar<'a, Message> {
    tabs: Vec<TabItem>,
    active: usize,
    columns: usize,
    on_event: Box<dyn Fn(TabBarEvent) -> Message + 'a>,
}

#[derive(Default)]
struct State {
    /// 押下中 (開始位置, タブ index)。閾値を超えるとドラッグ並べ替えへ。
    pressed: Option<(Point, usize, Instant)>,
    dragging: bool,
    /// ドラッグ中のドロップ先候補。
    drop_to: Option<usize>,
}

impl<'a, Message> TabBar<'a, Message> {
    pub fn new(
        tabs: Vec<TabItem>,
        active: usize,
        columns: usize,
        on_event: impl Fn(TabBarEvent) -> Message + 'a,
    ) -> Self {
        Self {
            tabs,
            active,
            columns: columns.clamp(1, 4),
            on_event: Box::new(on_event),
        }
    }

    fn cell(&self, bounds: &Rectangle, ix: usize) -> Rectangle {
        let cols = self.columns;
        let w = bounds.width / cols as f32;
        let row = ix / cols;
        let col = ix % cols;
        Rectangle {
            x: bounds.x + col as f32 * w,
            y: bounds.y + row as f32 * CELL_H,
            width: w,
            height: CELL_H,
        }
    }

    fn hit(&self, bounds: &Rectangle, pos: Point) -> Option<usize> {
        (0..self.tabs.len()).find(|&ix| self.cell(bounds, ix).contains(pos))
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for TabBar<'_, Message> {
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
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(pos) = cursor.position_over(bounds) else {
                    return;
                };
                if let Some(ix) = self.hit(&bounds, pos) {
                    // × 領域 (ロック中は閉じられない — F-104)
                    let cell = self.cell(&bounds, ix);
                    let close_zone = Rectangle {
                        x: cell.x + cell.width - CLOSE_W,
                        ..cell
                    };
                    if !self.tabs[ix].locked && close_zone.contains(pos) && self.tabs.len() > 1 {
                        shell.publish((self.on_event)(TabBarEvent::Close(ix)));
                    } else {
                        state.pressed = Some((pos, ix, Instant::now()));
                    }
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                let Some(pos) = cursor.position_over(bounds) else {
                    return;
                };
                if let Some(ix) = self.hit(&bounds, pos) {
                    shell.publish((self.on_event)(TabBarEvent::ToggleLock(ix)));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some((start, _ix, _)) = state.pressed {
                    if let Some(pos) = cursor.position() {
                        if state.dragging {
                            state.drop_to = cursor
                                .position_over(bounds)
                                .and_then(|p| self.hit(&bounds, p));
                            shell.request_redraw();
                        } else if pos.distance(start) > DRAG_THRESHOLD {
                            state.dragging = true;
                        }
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some((_, from, _)) = state.pressed.take() {
                    if state.dragging {
                        if let Some(to) = state.drop_to.take() {
                            if to != from {
                                shell.publish((self.on_event)(TabBarEvent::Reorder { from, to }));
                            }
                        }
                        state.dragging = false;
                    } else {
                        // ドラッグに至らなければ選択クリック
                        shell.publish((self.on_event)(TabBarEvent::Select(from)));
                    }
                    shell.capture_event();
                }
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let palette = theme.extended_palette();

        for (ix, tab) in self.tabs.iter().enumerate() {
            let cell = self.cell(&bounds, ix);
            let is_active = ix == self.active;
            let hovered = cursor.position_over(cell).is_some_and(|_| !state.dragging);
            let bg = if is_active {
                palette.primary.weak.color
            } else if hovered {
                palette.background.weak.color
            } else {
                Color::TRANSPARENT
            };
            if bg != Color::TRANSPARENT {
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle {
                            x: cell.x + 2.0,
                            y: cell.y + 2.0,
                            width: cell.width - 4.0,
                            height: cell.height - 4.0,
                        },
                        border: Border {
                            radius: 4.0.into(),
                            ..Border::default()
                        },
                        shadow: Shadow::default(),
                        snap: true,
                    },
                    bg,
                );
            }
            // ドロップ先の示唆 (上辺に線)
            if state.dragging && state.drop_to == Some(ix) {
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle {
                            x: cell.x + 2.0,
                            y: cell.y,
                            width: cell.width - 4.0,
                            height: 2.0,
                        },
                        border: Border::default(),
                        shadow: Shadow::default(),
                        snap: true,
                    },
                    palette.primary.strong.color,
                );
            }
            let fg = style.text_color;
            let text_w = if tab.locked {
                cell.width - 12.0
            } else {
                cell.width - CLOSE_W - 8.0
            };
            renderer.fill_text(
                Text {
                    content: tab.title.clone(),
                    // レイアウト幅を実幅に制限する: ソフトウェアレンダラは
                    // clip 引数を完全なマスクとして使わないため、無限幅だと
                    // 文字がセル外へ実際に描かれてしまう
                    bounds: Size::new(text_w.max(0.0), CELL_H),
                    size: Pixels(13.0),
                    line_height: LineHeight::default(),
                    font: crate::theme::ui_font(),
                    align_x: TextAlignment::Left,
                    align_y: Vertical::Center,
                    shaping: Shaping::Advanced,
                    wrapping: Wrapping::None,
                },
                Point::new(cell.x + 8.0, cell.y + CELL_H / 2.0),
                fg,
                Rectangle {
                    x: cell.x + 8.0,
                    y: cell.y,
                    width: text_w.max(0.0),
                    height: CELL_H,
                },
            );
            // × (ロック中は出さない — F-104)
            if !tab.locked && self.tabs.len() > 1 && (is_active || hovered) {
                renderer.fill_text(
                    Text {
                        content: "×".into(),
                        bounds: Size::new(CLOSE_W, CELL_H),
                        size: Pixels(14.0),
                        line_height: LineHeight::default(),
                        font: crate::theme::ui_font(),
                        align_x: TextAlignment::Center,
                        align_y: Vertical::Center,
                        shaping: Shaping::Basic,
                        wrapping: Wrapping::None,
                    },
                    Point::new(
                        cell.x + cell.width - CLOSE_W / 2.0 - 4.0,
                        cell.y + CELL_H / 2.0,
                    ),
                    Color { a: 0.7, ..fg },
                    cell,
                );
            }
        }
    }
}

impl<'a, Message: 'a> From<TabBar<'a, Message>> for Element<'a, Message, Theme, iced::Renderer> {
    fn from(w: TabBar<'a, Message>) -> Self {
        Element::new(w)
    }
}
