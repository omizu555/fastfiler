//! ワークスペースツリーの描画 (F-801〜F-803)。FileList と同じ直描き方式。
//!
//! - 行 = インデント + 展開矢印 + ラベル。フォルダのみ (CONTEXT.md)
//! - 左クリック: 矢印域 = 展開切替 / それ以外 = フォーカスペインに開く
//! - サーバノード (`\\server`): クリックで展開切替のみ (開けない)。右クリック = 登録解除
//! - 自動追従: `reveal_ix` が変わったら該当行を中央へスクロール

use fastfiler_core::tree::{TreeRow, TREE_ROW_H};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::Quad;
use iced::advanced::text::{
    Alignment as TextAlignment, LineHeight, Renderer as _, Shaping, Text, Wrapping,
};
use iced::advanced::widget::{tree, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::alignment::Vertical;
use iced::mouse::{self, ScrollDelta};
use iced::{Border, Color, Element, Event, Length, Pixels, Point, Rectangle, Shadow, Size, Theme};

const INDENT: f32 = 14.0;
const ARROW_W: f32 = 16.0;

/// ラベルを概算幅で「収まる分 + …」に切り詰める (エクスプローラ風)。
/// 正確なグリフ幅は描画前に取れないため、ASCII ≈ 0.55em / それ以外 ≈ 1.0em で
/// 見積もる (ツリーは 13px 固定なので誤差は数 px)。
fn ellipsize(label: &str, max_w: f32, font_px: f32) -> String {
    let char_w = |c: char| {
        if c.is_ascii() {
            font_px * 0.55
        } else {
            font_px
        }
    };
    let total: f32 = label.chars().map(char_w).sum();
    if total <= max_w {
        return label.to_string();
    }
    let ell_w = font_px; // "…"
    let mut used = 0.0;
    let mut out = String::new();
    for c in label.chars() {
        let w = char_w(c);
        if used + w + ell_w > max_w {
            break;
        }
        used += w;
        out.push(c);
    }
    out.push('…');
    out
}

#[derive(Debug, Clone)]
pub enum TreeEvent {
    /// パスをフォーカスペインに開く (F-306)。
    Open(std::path::PathBuf),
    /// 展開/折りたたみ。
    Toggle(std::path::PathBuf),
    /// サーバノードの登録解除 (右クリック — F-802)。
    RemoveServer(String),
    Scrolled(f32),
    ViewportChanged {
        height: f32,
    },
}

pub struct TreeList<'a, Message> {
    rows: Vec<TreeRow>,
    offset: f32,
    reveal_ix: Option<usize>,
    on_event: Box<dyn Fn(TreeEvent) -> Message + 'a>,
}

#[derive(Default)]
struct State {
    notified_h: f32,
    /// 処理済みの reveal (同じ値の再スクロールを防ぐ)。
    last_reveal: Option<usize>,
}

impl<'a, Message> TreeList<'a, Message> {
    pub fn new(
        tree_state: &fastfiler_core::tree::TreeState,
        on_event: impl Fn(TreeEvent) -> Message + 'a,
    ) -> Self {
        Self {
            rows: tree_state.rows(),
            offset: tree_state.scroll,
            reveal_ix: tree_state.reveal_ix,
            on_event: Box::new(on_event),
        }
    }

    fn max_offset(&self, h: f32) -> f32 {
        (self.rows.len() as f32 * TREE_ROW_H - h).max(0.0)
    }

    fn row_at(&self, bounds: &Rectangle, pos: Point) -> Option<usize> {
        let ix = ((pos.y - bounds.y + self.offset) / TREE_ROW_H) as usize;
        (ix < self.rows.len()).then_some(ix)
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for TreeList<'_, Message> {
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

        if (bounds.height - state.notified_h).abs() > 0.5 {
            state.notified_h = bounds.height;
            shell.publish((self.on_event)(TreeEvent::ViewportChanged {
                height: bounds.height,
            }));
        }
        // 自動追従の中央スクロール (F-803)
        if self.reveal_ix != state.last_reveal {
            state.last_reveal = self.reveal_ix;
            if let Some(ix) = self.reveal_ix {
                let target = (ix as f32 * TREE_ROW_H - bounds.height / 2.0 + TREE_ROW_H / 2.0)
                    .clamp(0.0, self.max_offset(bounds.height));
                if (target - self.offset).abs() > 1.0 {
                    shell.publish((self.on_event)(TreeEvent::Scrolled(target)));
                }
            }
        }

        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta })
                if cursor.position_over(bounds).is_some() =>
            {
                let dy = match delta {
                    ScrollDelta::Lines { y, .. } => -y * TREE_ROW_H * 3.0,
                    ScrollDelta::Pixels { y, .. } => -y,
                };
                let new = (self.offset + dy).clamp(0.0, self.max_offset(bounds.height));
                if (new - self.offset).abs() > f32::EPSILON {
                    shell.publish((self.on_event)(TreeEvent::Scrolled(new)));
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(pos) = cursor.position_over(bounds) else {
                    return;
                };
                if let Some(ix) = self.row_at(&bounds, pos) {
                    let row = &self.rows[ix];
                    let arrow_x = bounds.x + 4.0 + row.depth as f32 * INDENT;
                    let in_arrow = pos.x >= arrow_x && pos.x <= arrow_x + ARROW_W;
                    match (&row.path, in_arrow && row.expandable, &row.server) {
                        // サーバノードは全域で展開切替
                        (None, _, Some(server)) => {
                            shell.publish((self.on_event)(TreeEvent::Toggle(
                                std::path::PathBuf::from(format!("\\\\{server}")),
                            )));
                        }
                        (Some(p), true, _) => {
                            shell.publish((self.on_event)(TreeEvent::Toggle(p.clone())));
                        }
                        (Some(p), false, _) => {
                            shell.publish((self.on_event)(TreeEvent::Open(p.clone())));
                        }
                        _ => {}
                    }
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                let Some(pos) = cursor.position_over(bounds) else {
                    return;
                };
                if let Some(ix) = self.row_at(&bounds, pos) {
                    if let Some(server) = &self.rows[ix].server {
                        shell.publish((self.on_event)(TreeEvent::RemoveServer(server.clone())));
                        shell.capture_event();
                    }
                }
            }
            _ => {}
        }
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
        let palette = theme.extended_palette();
        let offset = self.offset.clamp(0.0, self.max_offset(bounds.height));
        let first = (offset / TREE_ROW_H) as usize;
        let visible = (bounds.height / TREE_ROW_H).ceil() as usize + 1;
        let last = (first + visible).min(self.rows.len());

        for ix in first..last {
            let row = &self.rows[ix];
            let y = bounds.y + (ix as f32 * TREE_ROW_H - offset);
            if row.is_current {
                let hl = Rectangle {
                    x: bounds.x,
                    y,
                    width: bounds.width,
                    height: TREE_ROW_H,
                }
                .intersection(&bounds)
                .unwrap_or_default();
                renderer.fill_quad(
                    Quad {
                        bounds: hl,
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
            let x0 = bounds.x + 4.0 + row.depth as f32 * INDENT;
            // 完全に見えている行だけ文字を描く (部分行は背景のみ)。
            // ソフトウェアレンダラは clip を完全マスクにしないため、
            // はみ出しはレイアウト段階で断つ
            let fully_visible =
                y >= bounds.y - 0.5 && y + TREE_ROW_H <= bounds.y + bounds.height + 0.5;
            if !fully_visible {
                continue;
            }
            let label_w = (bounds.x + bounds.width - x0 - ARROW_W - 8.0).max(0.0);
            let cell = |content: String, size: f32, w: f32| Text {
                content,
                bounds: Size::new(w, TREE_ROW_H),
                size: Pixels(size),
                line_height: LineHeight::default(),
                font: crate::theme::ui_font(),
                align_x: TextAlignment::Left,
                align_y: Vertical::Center,
                shaping: Shaping::Advanced,
                wrapping: Wrapping::None,
            };
            let cy = y + TREE_ROW_H / 2.0;
            let row_clip = |x: f32, w: f32| {
                Rectangle {
                    x,
                    y,
                    width: w.max(0.0),
                    height: TREE_ROW_H,
                }
                .intersection(&bounds)
                .unwrap_or(Rectangle {
                    x,
                    y,
                    width: 0.0,
                    height: 0.0,
                })
            };
            if row.expandable {
                renderer.fill_text(
                    cell(if row.expanded { "▼" } else { "▶" }.into(), 9.0, ARROW_W),
                    Point::new(x0 + 3.0, cy),
                    Color {
                        a: 0.6,
                        ..style.text_color
                    },
                    row_clip(x0, ARROW_W + 6.0),
                );
            }
            renderer.fill_text(
                cell(ellipsize(&row.label, label_w, 13.0), 13.0, label_w),
                Point::new(x0 + ARROW_W, cy),
                style.text_color,
                row_clip(x0 + ARROW_W, label_w),
            );
        }
        // スクロールバー
        let total = self.rows.len() as f32 * TREE_ROW_H;
        if total > bounds.height && bounds.height > 0.0 {
            let thumb_h = (bounds.height / total * bounds.height).max(20.0);
            let thumb_y = bounds.y + (offset / (total - bounds.height)) * (bounds.height - thumb_h);
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle {
                        x: bounds.x + bounds.width - 6.0,
                        y: thumb_y,
                        width: 4.0,
                        height: thumb_h,
                    },
                    border: Border {
                        radius: 2.0.into(),
                        ..Border::default()
                    },
                    shadow: Shadow::default(),
                    snap: true,
                },
                Color {
                    a: 0.3,
                    ..palette.background.base.text
                },
            );
        }
    }
}

impl<'a, Message: 'a> From<TreeList<'a, Message>> for Element<'a, Message, Theme, iced::Renderer> {
    fn from(w: TreeList<'a, Message>) -> Self {
        Element::new(w)
    }
}

/// スタイル (形状プリセット) 連動の角丸 (F-1102)。
fn fastfiler_iced_radius_sm() -> iced::border::Radius {
    crate::theme::ui_style().radius_sm.into()
}
