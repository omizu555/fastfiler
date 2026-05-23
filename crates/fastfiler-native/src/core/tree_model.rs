//! ツリーペインの状態モデル (TreeNode と再帰ヘルパ)。
//!
//! 元は `ui/tree.rs` に置いていたが、`AppState` から直接 roots を保持する
//! 必要が出たため `core/` 側に分離した。レンダリング (`render_tree_node`) は
//! 引き続き `ui/tree.rs` 側にある。

use std::path::{Path, PathBuf};

use fastfiler_domain::fs as ffs;
use floem::reactive::{RwSignal, Scope, SignalGet, SignalUpdate};

#[derive(Clone)]
pub struct TreeNode {
    pub path: PathBuf,
    pub name: String,
    pub expanded: RwSignal<bool>,
    pub loaded: RwSignal<bool>,
    pub children: RwSignal<im::Vector<TreeNode>>,
}

impl TreeNode {
    pub fn new(path: PathBuf) -> Self {
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
    pub fn load_children(&self) {
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
pub fn reload_expanded_recursive(node: &TreeNode) -> u32 {
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
pub fn visual_index(roots: &im::Vector<TreeNode>, target: &Path) -> Option<usize> {
    fn walk(node: &TreeNode, target: &Path, idx: &mut usize) -> bool {
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
pub fn expand_to_path(roots: &im::Vector<TreeNode>, target: &Path) -> u32 {
    use std::path::Component;
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
    let Some(root) = roots.iter().find(|r| r.path == *first) else {
        return 0;
    };

    let mut depth = 0u32;
    let mut cur = root.clone();
    cur.load_children();
    if !cur.expanded.get_untracked() {
        cur.expanded.set(true);
    }
    depth += 1;

    for seg in segs.iter().skip(1) {
        let kids = cur.children.get_untracked();
        let Some(child) = kids.iter().find(|c| c.path == *seg).cloned() else {
            break;
        };
        child.load_children();
        if !child.expanded.get_untracked() {
            child.expanded.set(true);
        }
        cur = child;
        depth += 1;
    }
    depth
}

/// 展開状態を踏まえた可視ノード列 (pre-order DFS)。キーボード操作の上下移動用。
pub fn visible_paths(roots: &im::Vector<TreeNode>) -> Vec<PathBuf> {
    fn walk(node: &TreeNode, out: &mut Vec<PathBuf>) {
        out.push(node.path.clone());
        if node.expanded.get_untracked() {
            for c in node.children.get_untracked().iter() {
                walk(c, out);
            }
        }
    }
    let mut out = Vec::new();
    for r in roots.iter() {
        walk(r, &mut out);
    }
    out
}

/// 指定パスのノードを roots から探す (DFS、可視性に関係なく)。
pub fn find_node(roots: &im::Vector<TreeNode>, path: &Path) -> Option<TreeNode> {
    fn walk(node: &TreeNode, path: &Path) -> Option<TreeNode> {
        if node.path == path {
            return Some(node.clone());
        }
        for c in node.children.get_untracked().iter() {
            if let Some(found) = walk(c, path) {
                return Some(found);
            }
        }
        None
    }
    for r in roots.iter() {
        if let Some(n) = walk(r, path) {
            return Some(n);
        }
    }
    None
}

/// 親パスを返す。root の場合は None。
pub fn parent_in_tree(roots: &im::Vector<TreeNode>, path: &Path) -> Option<PathBuf> {
    // roots に含まれる root path なら親なし
    if roots.iter().any(|r| r.path == path) {
        return None;
    }
    path.parent().map(|p| p.to_path_buf())
}
