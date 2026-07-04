//! 右クリックメニューの描画 (F-904)。stack 最上段の全画面レイヤ (直描き)。
//!
//! - パネルは指定座標に表示 (画面端では内側へフリップ)
//! - サブメニュー ▸ はクリック開閉 (USAGE.md §2 — hover では開かない)
//! - パネル外クリックで閉じる。項目クリックで index チェーンを publish

use fastfiler_core::menu::MenuItem;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::Quad;
use iced::advanced::text::{
    Alignment as TextAlignment, LineHeight, Renderer as _, Shaping, Text, Wrapping,
};
use iced::advanced::widget::{tree, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::alignment::Vertical;
use iced::mouse;
use iced::{
    Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Shadow, Size, Theme,
};

const ITEM_H: f32 = 26.0;
const PANEL_W: f32 = 220.0;

#[derive(Debug, Clone)]
pub enum MenuEvent {
    Clicked(Vec<usize>),
    Close,
}

pub struct ContextMenu<'a, Message> {
    items: Vec<MenuItem>,
    at: (f32, f32),
    open_path: Vec<usize>,
    on_event: Box<dyn Fn(MenuEvent) -> Message + 'a>,
}

impl<'a, Message> ContextMenu<'a, Message> {
    pub fn new(
        items: Vec<MenuItem>,
        at: (f32, f32),
        open_path: Vec<usize>,
        on_event: impl Fn(MenuEvent) -> Message + 'a,
    ) -> Self {
        Self {
            items,
            at,
            open_path,
            on_event: Box::new(on_event),
        }
    }

    /// 開いている各パネル (ルート + open_path のサブ) の矩形と項目列。
    /// 戻り値: (パネル矩形, 項目列, このパネルへの index チェーン接頭辞)。
    fn panels(&self, viewport: &Rectangle) -> Vec<(Rectangle, &[MenuItem], Vec<usize>)> {
        let mut out = Vec::new();
        let mut items: &[MenuItem] = &self.items;
        let mut prefix: Vec<usize> = Vec::new();
        let (mut x, mut y) = self.at;
        loop {
            let h = items.len() as f32 * ITEM_H + 8.0;
            // 画面端でフリップ (右端 → 左へ、下端 → 上へ)
            let px = if x + PANEL_W > viewport.width {
                (x - PANEL_W).max(0.0)
            } else {
                x
            };
            let py = if y + h > viewport.height {
                (viewport.height - h).max(0.0)
            } else {
                y
            };
            let rect = Rectangle {
                x: px,
                y: py,
                width: PANEL_W,
                height: h,
            };
            out.push((rect, items, prefix.clone()));
            // open_path が伸びていればサブパネルを続ける (最大 3 階層 — F-904)
            if prefix.len() >= 3 || prefix.len() >= self.open_path.len() {
                break;
            }
            let ix = self.open_path[prefix.len()];
            let Some(item) = items.get(ix) else { break };
            if item.children.is_empty() {
                break;
            }
            x = rect.x + PANEL_W - 4.0;
            y = rect.y + 4.0 + ix as f32 * ITEM_H;
            prefix.push(ix);
            items = &item.children;
        }
        out
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for ContextMenu<'_, Message> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::stateless()
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
        _tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        // メニュー表示中は最上位レイヤがすべてのマウス操作を受ける (常に 1 か所)。
        // CursorMoved はホバーの再描画要求も兼ねる (差分描画の残像対策)
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                shell.request_redraw();
                shell.capture_event();
                return;
            }
            Event::Mouse(mouse::Event::WheelScrolled { .. }) => {
                shell.capture_event();
                return;
            }
            _ => {}
        }
        if let Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left | mouse::Button::Right,
        )) = event
        {
            {
                let Some(pos) = cursor.position_over(bounds) else {
                    shell.publish((self.on_event)(MenuEvent::Close));
                    shell.capture_event();
                    return;
                };
                // 後ろのパネル (サブ) が優先
                for (rect, items, prefix) in self.panels(&bounds).into_iter().rev() {
                    if rect.contains(pos) {
                        let ix = ((pos.y - rect.y - 4.0) / ITEM_H) as usize;
                        if ix < items.len() {
                            let mut path = prefix;
                            path.push(ix);
                            shell.publish((self.on_event)(MenuEvent::Clicked(path)));
                        }
                        shell.capture_event();
                        return;
                    }
                }
                // 全パネル外 → 閉じる (このクリックは他へ渡さない — 常に 1 か所のみ表示)
                shell.publish((self.on_event)(MenuEvent::Close));
                shell.capture_event();
            }
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let bounds = layout.bounds();
        let palette = theme.extended_palette();
        // 画面全域の薄い覆い: メニュー外を淡くしてモーダル感を出しつつ、
        // 全域を毎フレーム塗ることで差分描画の残像 (ハイライトの引きずり) を防ぐ
        renderer.fill_quad(
            Quad {
                bounds,
                border: Border::default(),
                shadow: Shadow::default(),
                snap: true,
            },
            Color::from_rgba(0.0, 0.0, 0.0, 0.10),
        );
        for (rect, items, _prefix) in self.panels(&bounds) {
            renderer.fill_quad(
                Quad {
                    bounds: rect,
                    border: Border {
                        color: palette.background.strong.color,
                        width: 1.0,
                        radius: fastfiler_iced_radius_md(),
                    },
                    shadow: Shadow {
                        color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
                        offset: iced::Vector::new(0.0, 1.0),
                        blur_radius: 4.0,
                    },
                    snap: true,
                },
                palette.background.base.color,
            );
            for (i, item) in items.iter().enumerate() {
                let y = rect.y + 4.0 + i as f32 * ITEM_H;
                let row = Rectangle {
                    x: rect.x + 4.0,
                    y,
                    width: rect.width - 8.0,
                    height: ITEM_H,
                };
                if item.enabled && cursor.position_over(row).is_some() {
                    renderer.fill_quad(
                        Quad {
                            bounds: row,
                            border: Border {
                                radius: fastfiler_iced_radius_sm(),
                                ..Border::default()
                            },
                            shadow: Shadow::default(),
                            snap: true,
                        },
                        palette.primary.weak.color,
                    );
                }
                let fg = if item.enabled {
                    style.text_color
                } else {
                    Color {
                        a: 0.4,
                        ..style.text_color
                    }
                };
                let cell = |content: String, align: TextAlignment| Text {
                    content,
                    bounds: Size::new(f32::MAX, ITEM_H),
                    size: Pixels(13.0),
                    line_height: LineHeight::default(),
                    font: Font::default(),
                    align_x: align,
                    align_y: Vertical::Center,
                    shaping: Shaping::Advanced,
                    wrapping: Wrapping::None,
                };
                renderer.fill_text(
                    cell(item.label.clone(), TextAlignment::Left),
                    Point::new(row.x + 8.0, y + ITEM_H / 2.0),
                    fg,
                    row,
                );
                if !item.children.is_empty() {
                    renderer.fill_text(
                        cell("▸".into(), TextAlignment::Left),
                        Point::new(row.x + row.width - 16.0, y + ITEM_H / 2.0),
                        Color { a: 0.6, ..fg },
                        row,
                    );
                }
            }
        }
    }
}

impl<'a, Message: 'a> From<ContextMenu<'a, Message>>
    for Element<'a, Message, Theme, iced::Renderer>
{
    fn from(w: ContextMenu<'a, Message>) -> Self {
        Element::new(w)
    }
}

/// スタイル (形状プリセット) 連動の角丸 (F-1102)。
fn fastfiler_iced_radius_md() -> iced::border::Radius {
    crate::theme::ui_style().radius_md.into()
}
fn fastfiler_iced_radius_sm() -> iced::border::Radius {
    crate::theme::ui_style().radius_sm.into()
}
