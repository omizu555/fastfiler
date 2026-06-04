//! Phase 4: ワークスペースツリー (ドライブ起点のフォルダツリーパネル)。
//!
//! - ルートはローカルドライブ (`domain::fs::list_drives`)。フォルダのみ表示。
//! - 展開は遅延読み込み (`domain::fs::list_dirs`)。展開時に最新を読み直す。
//! - 表示は展開状態から平坦化した `items` を `uniform_list` で仮想化描画。
//! - ノード名クリックで `TreeEvent::OpenDir` を emit → FastFilerApp が
//!   **フォーカスペイン**に開く (CONTEXT.md の定義どおり)。
//! - UNC サーバノードは未対応 (今後)。

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::PathBuf;

use fastfiler_domain::fs;
use gpui::{
    AnyElement, Context, EventEmitter, IntoElement, SharedString, UniformListScrollHandle, Window,
    div, prelude::*, px, rgb, uniform_list,
};

/// ツリーからコンテナへのイベント。
pub enum TreeEvent {
    /// このフォルダをフォーカスペインで開いてほしい。
    OpenDir(PathBuf),
}

impl EventEmitter<TreeEvent> for TreeView {}

/// 平坦化された 1 行分。
struct TreeItem {
    path: PathBuf,
    name: String,
    depth: usize,
    expanded: bool,
}

pub struct TreeView {
    /// (ドライブパス "C:\", 表示名)
    drives: Vec<(String, String)>,
    expanded: HashSet<PathBuf>,
    /// 子フォルダ名のキャッシュ (展開時に取得、再展開で読み直し)。
    children: HashMap<PathBuf, Vec<String>>,
    /// 表示用の平坦化リスト。
    items: Vec<TreeItem>,
    scroll: UniformListScrollHandle,
}

impl TreeView {
    pub fn new() -> Self {
        let mut this = Self {
            drives: load_drives(),
            expanded: HashSet::new(),
            children: HashMap::new(),
            items: Vec::new(),
            scroll: UniformListScrollHandle::new(),
        };
        this.rebuild();
        this
    }

    fn children_of(&mut self, path: &PathBuf) -> Vec<String> {
        if let Some(c) = self.children.get(path) {
            return c.clone();
        }
        let names: Vec<String> = fs::list_dirs(path.to_string_lossy().to_string(), Some(false))
            .map(|v| v.into_iter().map(|e| e.name).collect())
            .unwrap_or_default();
        self.children.insert(path.clone(), names.clone());
        names
    }

    /// 展開状態から表示リストを作り直す。
    fn rebuild(&mut self) {
        let drives = self.drives.clone();
        let mut items = Vec::new();
        for (letter, display) in &drives {
            self.push_item(&mut items, PathBuf::from(letter), display.clone(), 0);
        }
        self.items = items;
    }

    fn push_item(&mut self, items: &mut Vec<TreeItem>, path: PathBuf, name: String, depth: usize) {
        let expanded = self.expanded.contains(&path);
        items.push(TreeItem {
            path: path.clone(),
            name,
            depth,
            expanded,
        });
        if expanded {
            for child in self.children_of(&path) {
                let cpath = path.join(&child);
                self.push_item(items, cpath, child, depth + 1);
            }
        }
    }

    fn toggle(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(item) = self.items.get(ix) else {
            return;
        };
        let path = item.path.clone();
        if !self.expanded.remove(&path) {
            self.expanded.insert(path.clone());
            // 開くたびに子を読み直す (外部変更を拾う)。
            self.children.remove(&path);
        }
        self.rebuild();
        cx.notify();
    }

    /// ドライブ一覧と子キャッシュを更新して再構築。
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.drives = load_drives();
        self.children.clear();
        self.rebuild();
        cx.notify();
    }

    fn render_item(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let item = &self.items[ix];
        let arrow = if item.expanded { "▼" } else { "▶" };
        let indent = px(6.0 + item.depth as f32 * 14.0);
        let path = item.path.clone();
        let name = item.name.clone();

        div()
            .id(ix)
            .flex()
            .flex_row()
            .items_center()
            .h(px(24.0))
            .pl(indent)
            .pr_1()
            .gap_1()
            // ▶/▼: 展開トグル
            .child(
                div()
                    .id(SharedString::from(format!("ta-{ix}")))
                    .w(px(16.0))
                    .text_color(rgb(0x777777))
                    .cursor_pointer()
                    .hover(|s| s.text_color(rgb(0xcccccc)))
                    .child(arrow)
                    .on_click(cx.listener(move |this, _e, _w, cx| {
                        this.toggle(ix, cx);
                    })),
            )
            // 名前: フォーカスペインで開く
            .child(
                div()
                    .id(SharedString::from(format!("tn-{ix}")))
                    .flex_1()
                    .overflow_hidden()
                    .rounded_sm()
                    .px_1()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(0x2d3a48)))
                    .child(name)
                    .on_click(cx.listener(move |_this, _e, _w, cx| {
                        cx.emit(TreeEvent::OpenDir(path.clone()));
                    })),
            )
            .into_any_element()
    }
}

impl Render for TreeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.items.len();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x191919))
            .text_color(rgb(0xc8c8c8))
            // ヘッダ
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .px_2()
                    .h(px(28.0))
                    .bg(rgb(0x202020))
                    .text_color(rgb(0x9a9a9a))
                    .child(div().flex_1().child("ツリー"))
                    .child(
                        div()
                            .id("tree-refresh")
                            .px_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x3a3a3a)).text_color(rgb(0xdddddd)))
                            .child("⟳")
                            .on_click(cx.listener(|this, _e, _w, cx| this.refresh(cx))),
                    ),
            )
            // ツリー本体 (仮想化)
            .child(
                div().flex_1().overflow_hidden().child(
                    uniform_list(
                        "ws-tree",
                        count,
                        cx.processor(|this, range: Range<usize>, _w, cx| {
                            range.map(|ix| this.render_item(ix, cx)).collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(&self.scroll)
                    .size_full(),
                ),
            )
    }
}

/// ドライブ一覧 → (パス, 表示名)。
fn load_drives() -> Vec<(String, String)> {
    fs::list_drives()
        .map(|v| {
            v.into_iter()
                .map(|d| {
                    let display = if d.label.is_empty() {
                        d.letter.clone()
                    } else {
                        format!("{} ({})", d.label, d.letter.trim_end_matches('\\'))
                    };
                    (d.letter, display)
                })
                .collect()
        })
        .unwrap_or_default()
}
