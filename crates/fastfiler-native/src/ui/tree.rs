// Tree pane — フォルダツリー (遅延展開)

use std::path::PathBuf;

use fastfiler_domain::fs as ffs;
use floem::prelude::*;
use floem::reactive::{SignalGet, SignalUpdate};
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
        Self {
            path,
            name,
            expanded: RwSignal::new(false),
            loaded: RwSignal::new(false),
            children: RwSignal::new(im::Vector::new()),
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

    let body = v_stack((header, scroll(tree).style(|s| s.flex_grow(1.0).width_full())))
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




