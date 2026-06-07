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

use crate::theme::{self, th};
use gpui::{
    AnyElement, Context, EventEmitter, IntoElement, MouseButton, ScrollStrategy, SharedString,
    UniformListScrollHandle, Window, div, prelude::*, px, uniform_list,
};

/// ツリーからコンテナへのイベント。
pub enum TreeEvent {
    /// このフォルダをフォーカスペインで開いてほしい。
    OpenDir(PathBuf),
    /// UNC 登録内容が変わった (セッション保存してほしい)。
    UncChanged,
}

impl EventEmitter<TreeEvent> for TreeView {}

/// 平坦化された 1 行分。
struct TreeItem {
    path: PathBuf,
    name: String,
    depth: usize,
    expanded: bool,
    /// UNC サーバノード (実在しないコンテナ。クリック無効・右クリックで削除)。
    is_server: bool,
}

pub struct TreeView {
    /// (ドライブパス "C:\", 表示名)
    drives: Vec<(String, String)>,
    /// 登録済み UNC share (`\\server\share`)。ペインで UNC を開くと自動登録・永続化。
    unc_shares: Vec<String>,
    expanded: HashSet<PathBuf>,
    /// 子フォルダ名のキャッシュ (展開時に取得、再展開で読み直し)。
    children: HashMap<PathBuf, Vec<String>>,
    /// 表示用の平坦化リスト。
    items: Vec<TreeItem>,
    /// 追従ハイライト中のパス (reveal で設定)。
    selected: Option<PathBuf>,
    scroll: UniformListScrollHandle,
}

impl TreeView {
    pub fn new() -> Self {
        let mut this = Self {
            drives: load_drives(),
            unc_shares: Vec::new(),
            expanded: HashSet::new(),
            children: HashMap::new(),
            items: Vec::new(),
            selected: None,
            scroll: UniformListScrollHandle::new(),
        };
        this.rebuild();
        this
    }

    /// セッション復元: 登録済み UNC share を設定。
    pub fn set_unc_shares(&mut self, shares: Vec<String>) {
        self.unc_shares = shares;
        self.unc_shares.sort();
        self.unc_shares.dedup();
        self.rebuild();
    }

    /// 保存用: 登録済み UNC share。
    pub fn unc_shares(&self) -> &[String] {
        &self.unc_shares
    }

    /// UNC パスから `\\server\share` を抽出して登録する。変化したら true。
    pub fn register_unc(&mut self, path: &std::path::Path, cx: &mut Context<Self>) -> bool {
        let Some(share) = unc_share_of(path) else {
            return false;
        };
        if self.unc_shares.contains(&share) {
            return false;
        }
        self.unc_shares.push(share);
        self.unc_shares.sort();
        self.rebuild();
        cx.notify();
        cx.emit(TreeEvent::UncChanged);
        true
    }

    /// サーバノードの右クリック: そのサーバの share をまとめて登録解除。
    fn remove_server(&mut self, server: String, cx: &mut Context<Self>) {
        let prefix = format!(r"\\{server}\");
        let before = self.unc_shares.len();
        self.unc_shares.retain(|s| !s.starts_with(&prefix));
        if self.unc_shares.len() != before {
            self.rebuild();
            cx.notify();
            cx.emit(TreeEvent::UncChanged);
        }
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
        // UNC: サーバごとにグルーピング (サーバノードはコンテナ、share が通常ノード)。
        let shares = self.unc_shares.clone();
        let mut cur_server: Option<String> = None;
        for share in &shares {
            let Some((server, share_name)) = split_unc(share) else {
                continue;
            };
            if cur_server.as_deref() != Some(server.as_str()) {
                items.push(TreeItem {
                    path: PathBuf::from(format!(r"\\{server}")),
                    name: format!(r"\\{server}"),
                    depth: 0,
                    expanded: false,
                    is_server: true,
                });
                cur_server = Some(server.clone());
            }
            self.push_item(&mut items, PathBuf::from(share), share_name, 1);
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
            is_server: false,
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

    /// 指定パスの祖先を展開し、その行を選択ハイライト + 中央へスクロールする
    /// (タブ切替やペインのフォルダ移動でツリーを追従させる)。
    /// パス照合は大文字小文字を無視し、ツリー側の表記でノードを辿る。
    /// 子に見つからない階層 (隠しフォルダ等) は、辿れた最深の祖先で止める。
    pub fn reveal(&mut self, target: &std::path::Path, cx: &mut Context<Self>) {
        // ルート → target の祖先チェーン。
        let mut chain: Vec<PathBuf> = target.ancestors().map(|p| p.to_path_buf()).collect();
        chain.reverse();
        chain.retain(|p| !p.as_os_str().is_empty());
        let Some(root_want) = chain.first() else {
            return;
        };

        // 末尾 `\` の有無と大文字小文字を無視したパス比較。
        let eq = |a: &std::path::Path, b: &std::path::Path| {
            let a = a.to_string_lossy().to_ascii_lowercase();
            let b = b.to_string_lossy().to_ascii_lowercase();
            a.trim_end_matches('\\') == b.trim_end_matches('\\')
        };

        // ルートノード (ドライブ / UNC share) を解決。無ければ何もしない。
        let root = self
            .drives
            .iter()
            .map(|(letter, _)| PathBuf::from(letter))
            .chain(self.unc_shares.iter().map(PathBuf::from))
            .find(|p| eq(p, root_want));
        let Some(mut cur) = root else {
            return;
        };

        // 子へ降りながら親を展開していく (名前は大文字小文字無視で照合)。
        for want in &chain[1..] {
            let Some(want_name) = want
                .file_name()
                .map(|s| s.to_string_lossy().to_ascii_lowercase())
            else {
                break;
            };
            self.expanded.insert(cur.clone());
            let kids = self.children_of(&cur);
            let Some(k) = kids.iter().find(|k| k.to_ascii_lowercase() == want_name) else {
                break;
            };
            cur = cur.join(k);
        }

        self.rebuild();
        if let Some(ix) = self
            .items
            .iter()
            .position(|it| !it.is_server && it.path == cur)
        {
            self.selected = Some(cur);
            self.scroll.scroll_to_item(ix, ScrollStrategy::Center);
            cx.notify();
        }
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

        // サーバノード: 実在しないコンテナ。クリック無効・右クリックで削除 (CONTEXT.md)。
        if item.is_server {
            let server = item.name.trim_start_matches('\\').to_string();
            return div()
                .id(ix)
                .flex()
                .flex_row()
                .items_center()
                .h(theme::row_h())
                .pl(px(6.0))
                .pr_1()
                .gap_1()
                .text_color(th().text_dim)
                .child(div().flex_1().overflow_hidden().child(item.name.clone()))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, _e, _w, cx| {
                        this.remove_server(server.clone(), cx);
                    }),
                )
                .into_any_element();
        }

        let arrow = if item.expanded { "▼" } else { "▶" };
        let indent = px(6.0 + item.depth as f32 * 14.0);
        let path = item.path.clone();
        let name = item.name.clone();
        // 追従ハイライト (reveal で設定された現在地)。
        let is_sel = self.selected.as_ref() == Some(&item.path);

        div()
            .id(ix)
            .flex()
            .flex_row()
            .items_center()
            .h(theme::row_h())
            .pl(indent)
            .pr_1()
            .gap_1()
            // ▶/▼: 展開トグル
            .child(
                div()
                    .id(SharedString::from(format!("ta-{ix}")))
                    .w(px(16.0))
                    .text_color(th().text_faint)
                    .cursor_pointer()
                    .hover(|s| s.text_color(th().text_soft))
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
                    .truncate()
                    .rounded(theme::radius_sm())
                    .px_1()
                    .cursor_pointer()
                    .when(is_sel, |d| d.bg(th().sel_bg).text_color(th().text))
                    .hover(|s| s.bg(th().hover_bg))
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
            .bg(th().tree_bg)
            .text_color(th().text_soft)
            // ヘッダ
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .px_2()
                    .h(px(theme::font_px() + 12.0))
                    .bg(th().header_bg)
                    .text_color(th().text_dim)
                    .child(div().flex_1().child("ツリー"))
                    .child(
                        div()
                            .id("tree-refresh")
                            .px_1()
                            .rounded(theme::radius_sm())
                            .cursor_pointer()
                            .hover(|s| s.bg(th().button_bg).text_color(th().text))
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

/// `\\server\share` → (server, share)。
fn split_unc(share: &str) -> Option<(String, String)> {
    let s = share.strip_prefix(r"\\")?;
    let mut it = s.splitn(2, '\\');
    let server = it.next()?.to_string();
    let share_name = it.next()?.to_string();
    if server.is_empty() || share_name.is_empty() {
        return None;
    }
    Some((server, share_name))
}

/// パスから `\\server\share` プレフィックスを抽出。
fn unc_share_of(path: &std::path::Path) -> Option<String> {
    let s = path.to_string_lossy();
    let rest = s.strip_prefix(r"\\")?;
    let mut it = rest.split('\\');
    let server = it.next()?;
    let share = it.next()?;
    if server.is_empty() || share.is_empty() {
        return None;
    }
    Some(format!(r"\\{server}\{share}"))
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
