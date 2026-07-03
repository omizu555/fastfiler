//! 分割境界 / パネル幅のドラッグハンドル (F-203 / F-106)。
//!
//! 自分の矩形で押下を受け、ドラッグ中はウィンドウ全体の CursorMoved から
//! px 差分を拾って publish する (FileList の列リサイズと同じ方式)。
//! px→比率の換算は呼び出し側が `per_px` (1px あたりの値) で指定する。

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::Quad;
use iced::advanced::widget::{tree, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::mouse;
use iced::{Border, Color, Element, Event, Length, Point, Rectangle, Shadow, Size, Theme};

pub struct DragHandle<'a, Message> {
    /// true = 縦の仕切り (左右ドラッグ) / false = 横の仕切り (上下ドラッグ)。
    vertical: bool,
    thickness: f32,
    /// 1px の移動が意味する値 (比率差分など)。
    per_px: f32,
    on_drag: Box<dyn Fn(f32) -> Message + 'a>,
}

#[derive(Default)]
struct State {
    dragging: bool,
    last: Option<Point>,
}

impl<'a, Message> DragHandle<'a, Message> {
    pub fn new(vertical: bool, per_px: f32, on_drag: impl Fn(f32) -> Message + 'a) -> Self {
        Self {
            vertical,
            thickness: 6.0,
            per_px,
            on_drag: Box::new(on_drag),
        }
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for DragHandle<'_, Message> {
    fn size(&self) -> Size<Length> {
        if self.vertical {
            Size::new(Length::Fixed(self.thickness), Length::Fill)
        } else {
            Size::new(Length::Fill, Length::Fixed(self.thickness))
        }
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
        let max = limits.max();
        let size = if self.vertical {
            Size::new(self.thickness, max.height)
        } else {
            Size::new(max.width, self.thickness)
        };
        layout::Node::new(size)
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
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.position_over(layout.bounds()).is_some() =>
            {
                state.dragging = true;
                state.last = cursor.position();
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.dragging => {
                if let (Some(prev), Some(now)) = (state.last, cursor.position()) {
                    let d = if self.vertical {
                        now.x - prev.x
                    } else {
                        now.y - prev.y
                    };
                    if d.abs() > f32::EPSILON {
                        shell.publish((self.on_drag)(d * self.per_px));
                        state.last = Some(now);
                    }
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.dragging => {
                state.dragging = false;
                state.last = None;
                shell.capture_event();
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
        if state.dragging || cursor.position_over(layout.bounds()).is_some() {
            if self.vertical {
                mouse::Interaction::ResizingHorizontally
            } else {
                mouse::Interaction::ResizingVertically
            }
        } else {
            mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        _style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let bounds = layout.bounds();
        let hovered = cursor.position_over(bounds).is_some();
        let palette = theme.extended_palette();
        renderer.fill_quad(
            Quad {
                bounds,
                border: Border::default(),
                shadow: Shadow::default(),
                snap: true,
            },
            if hovered {
                palette.primary.weak.color
            } else {
                Color {
                    a: 0.15,
                    ..palette.background.strong.color
                }
            },
        );
    }
}

impl<'a, Message: 'a> From<DragHandle<'a, Message>>
    for Element<'a, Message, Theme, iced::Renderer>
{
    fn from(w: DragHandle<'a, Message>) -> Self {
        Element::new(w)
    }
}
