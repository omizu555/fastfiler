//! S-2: 仮想リストスパイク (計画書 §3 S-2 / §6)。
//!
//! GPUI `uniform_list` の代替となる自前ウィジェットの原型。
//! 10 万行を固定行高で保持し、**可視範囲の行だけ** を `draw` で直描きする
//! (行を子ウィジェットにしない = レイアウト/ツリー構築を O(可視行数) に抑える)。
//!
//! 検証モード:
//! - 手動: `cargo run -p fastfiler-iced --example spike_vlist` → ホイールでスクロール
//! - 自動: `SPIKE_AUTO=1` で毎フレーム大きくスクロールしながら 600 フレーム計測し、
//!   フレーム統計 (avg/p50/p95/max, fps) を stdout に出して自動終了。
//!   合格目安 (計画書 §3): p95 が 60fps 相当 (vsync ゆらぎ込みで ~20ms 以下)。

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::Quad;
use iced::advanced::text::{
    Alignment as TextAlignment, LineHeight, Renderer as _, Shaping, Text, Wrapping,
};
use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::alignment::Vertical;
use iced::mouse::{self, ScrollDelta};
use iced::widget::{column, text};
use iced::{
    window, Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Shadow, Size,
    Subscription, Task,
};

const ROW_H: f32 = 24.0;
const ROWS: usize = 100_000;
const WARMUP_FRAMES: usize = 30;
const MEASURE_FRAMES: usize = 600;

pub fn main() -> iced::Result {
    iced::application(boot, update, view)
        .title("S-2: 仮想リストスパイク (10 万行)")
        .subscription(subscription)
        .window_size((900.0, 700.0))
        .run()
}

#[derive(Debug, Clone)]
enum Message {
    Scrolled(f32),
    Frame(Instant),
}

struct App {
    rows: Vec<String>,
    offset: f32,
    auto: bool,
    gen_ms: f64,
    booted: Instant,
    first_frame: Option<Instant>,
    last_frame: Option<Instant>,
    frame_ms: Vec<f32>,
}

fn boot() -> (App, Task<Message>) {
    let t0 = Instant::now();
    // 実データ風の行 (連番 + ファイル名 + サイズ + 日時)。日本語も混ぜて shaping を負荷に含める。
    let rows: Vec<String> = (0..ROWS)
        .map(|i| {
            format!(
                "{i:06}  ファイル_{i}.txt  {:>6} KB  2026/07/03 12:{:02}",
                (i * 37) % 9_999,
                i % 60
            )
        })
        .collect();
    let gen_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let auto = std::env::var("SPIKE_AUTO").is_ok_and(|v| v == "1");
    (
        App {
            rows,
            offset: 0.0,
            auto,
            gen_ms,
            booted: Instant::now(),
            first_frame: None,
            last_frame: None,
            frame_ms: Vec::with_capacity(WARMUP_FRAMES + MEASURE_FRAMES + 8),
        },
        Task::none(),
    )
}

fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Scrolled(offset) => {
            app.offset = offset;
            Task::none()
        }
        Message::Frame(now) => {
            if app.first_frame.is_none() {
                app.first_frame = Some(now);
                println!(
                    "GEN_MS {:.1}\nFIRST_FRAME_MS {:.1}",
                    app.gen_ms,
                    (now - app.booted).as_secs_f64() * 1000.0
                );
            }
            if let Some(last) = app.last_frame {
                app.frame_ms.push((now - last).as_secs_f32() * 1000.0);
            }
            app.last_frame = Some(now);

            if app.auto {
                // 毎フレーム大きく飛ばして仮想化の worst case (全行入れ替え) を叩く
                let total = ROWS as f32 * ROW_H;
                app.offset = (app.offset + 1_237.0) % (total - 900.0).max(1.0);

                if app.frame_ms.len() >= WARMUP_FRAMES + MEASURE_FRAMES {
                    let mut m: Vec<f32> = app.frame_ms[WARMUP_FRAMES..].to_vec();
                    m.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    let avg: f32 = m.iter().sum::<f32>() / m.len() as f32;
                    let p50 = m[m.len() / 2];
                    let p95 = m[m.len() * 95 / 100];
                    let max = *m.last().unwrap();
                    println!(
                        "SPIKE_VLIST_RESULT rows={ROWS} avg_ms={avg:.2} p50_ms={p50:.2} \
                         p95_ms={p95:.2} max_ms={max:.2} fps={:.0}",
                        1000.0 / avg
                    );
                    return iced::exit();
                }
            }
            Task::none()
        }
    }
}

fn subscription(_app: &App) -> Subscription<Message> {
    window::frames().map(Message::Frame)
}

fn view(app: &App) -> Element<'_, Message> {
    column![
        text(format!(
            "10 万行 仮想リスト — offset {:.0}px / 行生成 {:.0}ms / ホイールでスクロール{}",
            app.offset,
            app.gen_ms,
            if app.auto {
                " (自動計測中…)"
            } else {
                ""
            }
        ))
        .size(14),
        Element::new(VirtualList {
            rows: &app.rows,
            offset: app.offset,
            on_scroll: Box::new(Message::Scrolled),
        }),
    ]
    .spacing(4)
    .padding(8)
    .into()
}

/// 可視範囲だけを直描きする仮想リスト (FileList の原型。計画書 §6)。
struct VirtualList<'a, Message> {
    rows: &'a [String],
    offset: f32,
    on_scroll: Box<dyn Fn(f32) -> Message + 'a>,
}

impl<'a, Message> VirtualList<'a, Message> {
    fn max_offset(&self, viewport_h: f32) -> f32 {
        (self.rows.len() as f32 * ROW_H - viewport_h).max(0.0)
    }
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for VirtualList<'_, Message> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
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
        if let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event {
            if cursor.position_over(bounds).is_some() {
                let dy = match delta {
                    ScrollDelta::Lines { y, .. } => -y * ROW_H * 3.0,
                    ScrollDelta::Pixels { y, .. } => -y,
                };
                let new_offset = (self.offset + dy).clamp(0.0, self.max_offset(bounds.height));
                shell.publish((self.on_scroll)(new_offset));
                shell.capture_event();
            }
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &iced::Theme,
        style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;

        let bounds = layout.bounds();
        let offset = self.offset.clamp(0.0, self.max_offset(bounds.height));
        let first = (offset / ROW_H) as usize;
        let visible = (bounds.height / ROW_H).ceil() as usize + 1;
        let last = (first + visible).min(self.rows.len());

        for i in first..last {
            let y = bounds.y + (i as f32 * ROW_H - offset);
            // 縞背景 (行の視認用)
            if i % 2 == 1 {
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle {
                            x: bounds.x,
                            y,
                            width: bounds.width,
                            height: ROW_H,
                        },
                        border: Border::default(),
                        shadow: Shadow::default(),
                        snap: true,
                    },
                    Color::from_rgba(0.5, 0.5, 0.5, 0.08),
                );
            }
            renderer.fill_text(
                Text {
                    content: self.rows[i].clone(),
                    bounds: Size::new(bounds.width - 24.0, ROW_H),
                    size: Pixels(14.0),
                    line_height: LineHeight::default(),
                    font: Font::default(),
                    align_x: TextAlignment::Left,
                    align_y: Vertical::Center,
                    shaping: Shaping::Advanced,
                    wrapping: Wrapping::None,
                },
                Point::new(bounds.x + 8.0, y + ROW_H / 2.0),
                style.text_color,
                bounds,
            );
        }

        // 簡易スクロールバー
        let total = self.rows.len() as f32 * ROW_H;
        if total > bounds.height {
            let thumb_h = (bounds.height / total * bounds.height).max(24.0);
            let thumb_y = bounds.y + (offset / (total - bounds.height)) * (bounds.height - thumb_h);
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle {
                        x: bounds.x + bounds.width - 8.0,
                        y: thumb_y,
                        width: 6.0,
                        height: thumb_h,
                    },
                    border: Border::default(),
                    shadow: Shadow::default(),
                    snap: true,
                },
                Color::from_rgba(0.5, 0.5, 0.5, 0.5),
            );
        }
    }
}
