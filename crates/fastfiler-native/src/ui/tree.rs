// Tree pane — フォルダツリー (遅延展開)

use std::path::PathBuf;

use fastfiler_domain::fs as ffs;
use floem::prelude::*;
use floem::reactive::{Scope, SignalGet, SignalUpdate};
use floem::style::CursorStyle;
use floem::views::{container, dyn_container, h_stack, label, scroll, v_stack, Decorators};

use crate::fs_model::list_drives;
use crate::state::AppState;
use crate::theme;
#[derive(Clone)]
pub struct TreeNode {
    path: PathBuf,
    name: String,
    expanded: RwSignal<bool>,
    loaded: RwSignal<bool>,
    children: RwSignal<im::Vector<TreeNode>>,
}

impl TreeNode {
    fn new(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        // TreeNode は load_children/expand_to_path 経由で effect 内で生成され得るため、
        // signal を untethered scope に置いて effect 再走時の自動 dispose を回避する。
        let s = Scope::new();
        Self {
            path,
            name,
            expanded: s.create_rw_signal(false),
            loaded: s.create_rw_signal(false),
            children: s.create_rw_signal(im::Vector::new()),
        }
    }

    /// 子フォルダを 1 階層だけロード。既存子の expanded/children 状態は path 一致で保持する。
    fn load_children(&self) {
        let s = self.path.to_string_lossy().into_owned();
        if let Ok(dirs) = ffs::list_dirs(s, Some(false)) {
            let existing: std::collections::HashMap<PathBuf, TreeNode> = self
                .children
                .get_untracked()
                .into_iter()
                .map(|c| (c.path.clone(), c))
                .collect();
            let mut tmp: Vec<TreeNode> = dirs
                .into_iter()
                .map(|e| {
                    let p = self.path.join(&e.name);
                    if let Some(node) = existing.get(&p) {
                        node.clone()
                    } else {
                        TreeNode::new(p)
                    }
                })
                .collect();
            tmp.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            self.children.set(tmp.into_iter().collect());
        }
        self.loaded.set(true);
    }
}

/// 展開済みノードを再帰的に reload。戻り値: reload した数。
fn reload_expanded_recursive(node: &TreeNode) -> u32 {
    let mut n = 0;
    if node.expanded.get_untracked() {
        node.load_children();
        n += 1;
        for c in node.children.get_untracked().iter() {
            n += reload_expanded_recursive(c);
        }
    }
    n
}

/// 展開状態を踏まえ、target パスのノードが画面上で何番目の row かを返す (0-based, pre-order DFS)。
/// 見つからなければ None。Roots と各 expanded children を再帰的に列挙する。
fn visual_index(roots: &im::Vector<TreeNode>, target: &std::path::Path) -> Option<usize> {
    fn walk(node: &TreeNode, target: &std::path::Path, idx: &mut usize) -> bool {
        if node.path == target {
            return true;
        }
        *idx += 1;
        if node.expanded.get_untracked() {
            for c in node.children.get_untracked().iter() {
                if walk(c, target, idx) {
                    return true;
                }
            }
        }
        false
    }
    let mut idx = 0usize;
    for r in roots.iter() {
        if walk(r, target, &mut idx) {
            return Some(idx);
        }
    }
    None
}

/// 既に展開済みでも load_children を呼んで最新化。戻り値: 到達できた末端ノード深さ。
fn expand_to_path(roots: &im::Vector<TreeNode>, target: &std::path::Path) -> u32 {
    use std::path::{Component, PathBuf};
    // target を正規化された絶対パスにし、ドライブから順に各セグメントを accumulate
    let mut acc = PathBuf::new();
    let mut segs: Vec<PathBuf> = Vec::new();
    for c in target.components() {
        match c {
            Component::Prefix(p) => {
                acc.push(p.as_os_str());
                // Windows: "C:" の後に "\\" を付けて "C:\\" にしないと list_drives() の root と一致しない
                let mut with_sep = acc.clone();
                with_sep.push("\\");
                segs.push(with_sep.clone());
                acc = with_sep;
            }
            Component::RootDir => { /* prefix で済 */ }
            Component::Normal(n) => {
                acc.push(n);
                segs.push(acc.clone());
            }
            _ => {}
        }
    }
    let Some(first) = segs.first() else { return 0 };
    let Some(root) = roots.iter().find(|r| r.path == *first) else { return 0 };

    let mut depth = 0u32;
    let mut cur = root.clone();
    cur.load_children();
    if !cur.expanded.get_untracked() {
        cur.expanded.set(true);
    }
    depth += 1;

    for seg in segs.iter().skip(1) {
        let kids = cur.children.get_untracked();
        let Some(child) = kids.iter().find(|c| c.path == *seg).cloned() else { break };
        child.load_children();
        if !child.expanded.get_untracked() {
            child.expanded.set(true);
        }
        cur = child;
        depth += 1;
    }
    depth
}

pub fn render_tree_node(app: AppState, node: TreeNode, depth: usize) -> floem::AnyView {
    let expanded = node.expanded;
    let children = node.children;
    let path_for_nav = node.path.clone();
    let name_text = node.name.clone();

    let app_for_click = app.clone();
    let node_for_toggle = node.clone();

    let arrow = label(move || {
        if expanded.get() { String::from("▼") } else { String::from("▶") }
    })
    .style(|s| {
        s.width(14)
            .padding_horiz(2)
            .color(theme::text_dim())
            .cursor(CursorStyle::Pointer)
    })
    .on_click_stop(move |_| {
        let was = expanded.get();
        if !was {
            node_for_toggle.load_children();
        }
        expanded.set(!was);
    });

    let name_lbl = label(move || name_text.clone())
        .style(|s| {
            s.flex_grow(1.0)
                .padding_horiz(4)
                .cursor(CursorStyle::Pointer)
                .color(theme::text_normal())
        })
        .on_click_stop(move |_| {
            if let Some(p) = app_for_click.active_pane() {
                p.navigate(path_for_nav.clone(), true);
            }
        });

    let indent = (depth as f32) * 14.0 + 4.0;
    let row = h_stack((arrow, name_lbl)).style(move |s| {
        s.height(22).items_center().padding_left(indent)
    });

    let app_for_kids = app.clone();
    let kids = dyn_container(
        move || (expanded.get(), children.get()),
        move |(open, kids)| {
            if !open {
                return container(label(|| String::new()))
                    .style(|s| s.height(0))
                    .into_any();
            }
            let app = app_for_kids.clone();
            let items: Vec<floem::AnyView> = kids
                .into_iter()
                .map(|c| render_tree_node(app.clone(), c, depth + 1))
                .collect();
            floem::views::stack_from_iter(items)
                .style(|s| s.flex_col())
                .into_any()
        },
    );

    v_stack((row, kids)).style(|s| s.flex_col()).into_any()
}

pub fn tree_pane(app: AppState) -> impl IntoView {
    let roots: im::Vector<TreeNode> = list_drives()
        .into_iter()
        .map(|d| TreeNode::new(PathBuf::from(d)))
        .collect();
    let roots_sig = RwSignal::new(roots);
    // スクロール先 Y 座標 (フォローエフェクトが set、scroll が読む)
    let scroll_target: RwSignal<Option<floem::kurbo::Point>> =
        floem::reactive::Scope::new().create_rw_signal(None);

    // tree_tick を track し、展開済みノードを再帰的に reload する単一 effect。
    // tree_pane の scope に置くことで、レンダリング再生成で破棄されないようにする。
    let tree_tick = app.tree_tick;
    let roots_for_effect = roots_sig;
    floem::reactive::create_effect(move |_| {
        let t = tree_tick.get();
        let roots = roots_for_effect.get_untracked();
        let mut count = 0u32;
        for r in roots.iter() {
            count += reload_expanded_recursive(r);
        }
        if count > 0 {
            crate::flog!("[tree] tick={} reloaded {} expanded nodes", t, count);
        }
    });

    // アクティブペインの cur_path に追従してツリーを自動展開する effect。
    // active タブ → そのタブの active_pane → そのペインの cur_path を全て track。
    let app_for_follow = app.clone();
    let roots_for_follow = roots_sig;
    floem::reactive::create_effect(move |_| {
        // active タブ id を track
        let id = app_for_follow.active.get();
        let tabs = app_for_follow.tabs.get();
        let Some(tab) = tabs.iter().find(|t| t.id == id).cloned() else { return };
        // active_pane id を track
        let active_pane_id = tab.active_pane.get();
        // root を track して再描画にも追従
        tab.root.with(|_| {});
        let panes = tab.all_panes();
        let pane = panes
            .iter()
            .find(|p| p.id == active_pane_id)
            .cloned()
            .or_else(|| panes.first().cloned());
        let Some(pane) = pane else { return };
        // cur_path を track
        let path = pane.cur_path.get();
        let roots = roots_for_follow.get_untracked();
        let depth = expand_to_path(&roots, &path);
        // 表示インデックス → Y 座標 (row 22px、上下に少し余白)
        if let Some(idx) = visual_index(&roots, &path) {
            let y = (idx as f64) * 22.0;
            // 上に少しマージンを残してスクロール
            let target_y = (y - 44.0).max(0.0);
            scroll_target.set(Some(floem::kurbo::Point::new(0.0, target_y)));
            crate::flog!("[tree] follow pane={} path={} depth={} idx={} scroll_y={}",
                pane.id, path.display(), depth, idx, target_y);
        } else {
            crate::flog!("[tree] follow pane={} path={} depth={} (no idx)",
                pane.id, path.display(), depth);
        }
    });

    let app_for_render = app.clone();
    let tree = dyn_container(
        move || roots_sig.get(),
        move |roots| {
            let app = app_for_render.clone();
            let items: Vec<floem::AnyView> = roots
                .into_iter()
                .map(|n| render_tree_node(app.clone(), n, 0))
                .collect();
            floem::views::stack_from_iter(items)
                .style(|s| s.flex_col())
                .into_any()
        },
    );

    let header = label(|| String::from("Tree"))
        .style(|s| s.padding(6).font_bold().color(theme::text_label()));

    let body = v_stack((
        header,
        scroll(tree)
            .scroll_to(move || scroll_target.get())
            .style(|s| s.flex_grow(1.0).min_height(0).width_full()),
    ))
    .style(|s| s.flex_col().size_full());

    let tree_width_sig = app.settings.tree_width;
    container(body).style(move |s| {
        let w = tree_width_sig
            .get()
            .parse::<f32>()
            .unwrap_or(240.0)
            .clamp(120.0, 600.0);
        s.width(w)
            .height_full()
            .background(theme::bg_panel())
            .border_right(1)
            .border_color(theme::border_default())
    })
}




