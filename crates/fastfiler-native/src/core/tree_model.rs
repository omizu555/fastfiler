//! ツリーペインの状態モデル (TreeNode と再帰ヘルパ)。
//!
//! 元は `ui/tree.rs` に置いていたが、`AppState` から直接 roots を保持する
//! 必要が出たため `core/` 側に分離した。レンダリング (`render_tree_node`) は
//! 引き続き `ui/tree.rs` 側にある。

use std::path::{Component, Path, PathBuf, Prefix};

use fastfiler_domain::fs as ffs;
use floem::reactive::{RwSignal, Scope, SignalGet, SignalUpdate};

/// ツリーノードの種別。UI 振る舞い (展開, ナビ可否, アイコン, 右クリック) の分岐用。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeNodeKind {
    /// ローカルドライブ root (`C:\` 等)。load_children で list_dirs 可。
    LocalRoot,
    /// 通常フォルダ。
    Directory,
    /// UNC サーバノード (仮想 root, `\\server`)。load_children しない、ナビゲートしない。
    UncServer,
    /// UNC share root (`\\server\share`)。load_children で list_dirs 可。
    UncShare,
}

#[derive(Clone)]
pub struct TreeNode {
    pub path: PathBuf,
    pub name: String,
    pub kind: TreeNodeKind,
    pub expanded: RwSignal<bool>,
    pub loaded: RwSignal<bool>,
    pub children: RwSignal<im::Vector<TreeNode>>,
}

impl TreeNode {
    pub fn new(path: PathBuf) -> Self {
        Self::with_kind(path, TreeNodeKind::Directory)
    }

    pub fn with_kind(path: PathBuf, kind: TreeNodeKind) -> Self {
        let name = display_name_for(&path, kind);
        // TreeNode は load_children/expand_to_path 経由で effect 内で生成され得るため、
        // signal を untethered scope に置いて effect 再走時の自動 dispose を回避する。
        let s = Scope::new();
        // 仮想サーバは「これ以上 load しない」マーカとして loaded=true で生成。
        let loaded_init = matches!(kind, TreeNodeKind::UncServer);
        Self {
            path,
            name,
            kind,
            expanded: s.create_rw_signal(false),
            loaded: s.create_rw_signal(loaded_init),
            children: s.create_rw_signal(im::Vector::new()),
        }
    }

    /// 子フォルダを 1 階層だけロード。既存子の expanded/children 状態は path 一致で保持する。
    /// `UncServer` (仮想) では何もしない (children は外から build_tree_roots で植える)。
    pub fn load_children(&self) {
        if matches!(self.kind, TreeNodeKind::UncServer) {
            return;
        }
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
                        TreeNode::with_kind(p, TreeNodeKind::Directory)
                    }
                })
                .collect();
            tmp.sort_by_key(|a| a.name.to_lowercase());
            self.children.set(tmp.into_iter().collect());
        }
        self.loaded.set(true);
    }
}

fn display_name_for(path: &Path, kind: TreeNodeKind) -> String {
    match kind {
        TreeNodeKind::UncServer => {
            // `\\server` → "🖥️ server"
            let s = path.to_string_lossy();
            let server = s.trim_start_matches('\\');
            format!("🖥️ {server}")
        }
        TreeNodeKind::UncShare => {
            // `\\server\share` → "share"
            if let Some((_, share)) = parse_unc(path) {
                share
            } else {
                path.to_string_lossy().into_owned()
            }
        }
        _ => path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned()),
    }
}

/// 展開済みノードを再帰的に reload。戻り値: reload した数。
pub fn reload_expanded_recursive(node: &TreeNode) -> u32 {
    let mut n = 0;
    if node.expanded.get_untracked() && !matches!(node.kind, TreeNodeKind::UncServer) {
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
    let target = normalize_for_compare(target);
    fn walk(node: &TreeNode, target: &Path, idx: &mut usize) -> bool {
        if normalize_for_compare(&node.path) == target {
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
        if walk(r, &target, &mut idx) {
            return Some(idx);
        }
    }
    None
}

/// 既に展開済みでも load_children を呼んで最新化。戻り値: 到達できた末端ノード深さ。
/// UNC (`\\server\share\...`) は `[\\server, \\server\share, \\server\share\sub, ...]` で辿る。
pub fn expand_to_path(roots: &im::Vector<TreeNode>, target: &Path) -> u32 {
    let segs = path_segments(target);
    let Some(first) = segs.first() else { return 0 };
    let Some(root) = roots
        .iter()
        .find(|r| normalize_for_compare(&r.path) == normalize_for_compare(first))
    else {
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
        let seg_norm = normalize_for_compare(seg);
        let Some(child) = kids
            .iter()
            .find(|c| normalize_for_compare(&c.path) == seg_norm)
            .cloned()
        else {
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

/// target を root から末端までのパスセグメント列に分解する。
/// - ローカル: `C:\a\b\c` → `[C:\, C:\a, C:\a\b, C:\a\b\c]`
/// - UNC: `\\server\share\a\b` → `[\\server, \\server\share, \\server\share\a, \\server\share\a\b]`
fn path_segments(target: &Path) -> Vec<PathBuf> {
    let mut segs: Vec<PathBuf> = Vec::new();
    // UNC を先に判定
    if let Some((server, share)) = parse_unc(target) {
        let server_root = PathBuf::from(format!("\\\\{server}"));
        let share_root = PathBuf::from(format!("\\\\{server}\\{share}"));
        segs.push(server_root);
        segs.push(share_root.clone());
        // share より下の sub path を抽出
        let s = target.to_string_lossy().replace('/', "\\");
        let prefix_normal = format!("\\\\{server}\\{share}");
        let prefix_verbatim = format!("\\\\?\\UNC\\{server}\\{share}");
        let rest = s
            .strip_prefix(&prefix_normal)
            .or_else(|| s.strip_prefix(&prefix_verbatim))
            .unwrap_or("");
        let rest = rest.trim_start_matches('\\');
        if !rest.is_empty() {
            let mut acc = share_root;
            for part in rest.split('\\').filter(|p| !p.is_empty()) {
                acc.push(part);
                segs.push(acc.clone());
            }
        }
        return segs;
    }
    // 非 UNC (ローカル)
    let mut acc = PathBuf::new();
    for c in target.components() {
        match c {
            Component::Prefix(p) => {
                acc.push(p.as_os_str());
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
    segs
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

/// 指定パスのノードを roots から探す (DFS、可視性に関係なく)。比較は大文字小文字無視。
pub fn find_node(roots: &im::Vector<TreeNode>, path: &Path) -> Option<TreeNode> {
    let target = normalize_for_compare(path);
    fn walk(node: &TreeNode, target: &Path) -> Option<TreeNode> {
        if normalize_for_compare(&node.path) == target {
            return Some(node.clone());
        }
        for c in node.children.get_untracked().iter() {
            if let Some(found) = walk(c, target) {
                return Some(found);
            }
        }
        None
    }
    for r in roots.iter() {
        if let Some(n) = walk(r, &target) {
            return Some(n);
        }
    }
    None
}

/// 親パスを返す。root の場合は None。UNC では `\\server\share` の親を `\\server` に。
pub fn parent_in_tree(roots: &im::Vector<TreeNode>, path: &Path) -> Option<PathBuf> {
    let target_norm = normalize_for_compare(path);
    if roots
        .iter()
        .any(|r| normalize_for_compare(&r.path) == target_norm)
    {
        return None;
    }
    // UNC: `\\server\share` の親は `\\server`
    if let Some((server, share)) = parse_unc(path) {
        let s = path.to_string_lossy().replace('/', "\\");
        let prefix_normal = format!("\\\\{server}\\{share}");
        let prefix_verbatim = format!("\\\\?\\UNC\\{server}\\{share}");
        let rest_empty = match s
            .strip_prefix(&prefix_normal)
            .or_else(|| s.strip_prefix(&prefix_verbatim))
        {
            Some(r) => r.trim_matches('\\').is_empty(),
            None => false,
        };
        if rest_empty {
            return Some(PathBuf::from(format!("\\\\{server}")));
        }
    }
    path.parent().map(|p| p.to_path_buf())
}

// ─────────────────────────────────────────────────────────────
//  UNC 関連ヘルパ
// ─────────────────────────────────────────────────────────────

/// UNC パスから `(server, share)` を取り出す。`\\server\share[\…]` と
/// `\\?\UNC\server\share[\…]` (verbatim) の両方を受け付ける。
/// server/share が取れない場合 (`\\server` 単独や非 UNC) は `None`。
pub fn parse_unc(path: &Path) -> Option<(String, String)> {
    for c in path.components() {
        if let Component::Prefix(pc) = c {
            return match pc.kind() {
                Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => Some((
                    server.to_string_lossy().into_owned(),
                    share.to_string_lossy().into_owned(),
                )),
                _ => None,
            };
        }
    }
    None
}

/// UNC share root を正規形に揃える。
/// `\\?\UNC\Server\Share`, `\\Server\Share\`, `\\server\share` → `\\server\share`。
/// UNC でない場合は None。
pub fn normalize_unc_share(path: &Path) -> Option<PathBuf> {
    let (server, share) = parse_unc(path)?;
    Some(PathBuf::from(format!(
        "\\\\{}\\{}",
        server.to_lowercase(),
        share.to_lowercase()
    )))
}

/// パス比較用の正規化。UNC は server/share を lowercase、verbatim を normal に揃える。
/// ローカルパスは drive letter のみ大文字化。
fn normalize_for_compare(path: &Path) -> PathBuf {
    if let Some((server, share)) = parse_unc(path) {
        let s = path.to_string_lossy().replace('/', "\\");
        let prefix_normal = format!("\\\\{server}\\{share}");
        let prefix_verbatim = format!("\\\\?\\UNC\\{server}\\{share}");
        let rest = s
            .strip_prefix(&prefix_normal)
            .or_else(|| s.strip_prefix(&prefix_verbatim))
            .unwrap_or("");
        let normalized = format!(
            "\\\\{}\\{}{}",
            server.to_lowercase(),
            share.to_lowercase(),
            rest.trim_end_matches('\\').to_lowercase()
        );
        return PathBuf::from(normalized);
    }
    // ローカル: 単に小文字化 (Windows の case-insensitive 比較)
    PathBuf::from(path.to_string_lossy().to_lowercase().replace('/', "\\"))
}

/// ローカルドライブ群 + UNC share 群から tree_roots を再構築する。
/// 既存 roots を引数に取り、path 一致でノードを再利用して expanded/loaded を保持する。
/// 並び順は「ローカル(ABC) → サーバ(ABC、share も ABC)」固定。
pub fn reconcile_tree_roots(
    existing: &im::Vector<TreeNode>,
    drives: &[String],
    unc_shares: &[String],
) -> im::Vector<TreeNode> {
    // 既存ノードを path-norm → ノード のマップへ
    let mut existing_map: std::collections::HashMap<PathBuf, TreeNode> =
        std::collections::HashMap::new();
    fn collect(node: &TreeNode, out: &mut std::collections::HashMap<PathBuf, TreeNode>) {
        out.insert(normalize_for_compare(&node.path), node.clone());
        for c in node.children.get_untracked().iter() {
            collect(c, out);
        }
    }
    for r in existing.iter() {
        collect(r, &mut existing_map);
    }

    // ローカルドライブ (ABC 順)
    let mut local_roots: Vec<TreeNode> = drives
        .iter()
        .map(|d| {
            let path = PathBuf::from(d);
            let key = normalize_for_compare(&path);
            if let Some(node) = existing_map.get(&key) {
                node.clone()
            } else {
                TreeNode::with_kind(path, TreeNodeKind::LocalRoot)
            }
        })
        .collect();
    local_roots.sort_by_key(|n| n.path.to_string_lossy().to_lowercase());

    // UNC: server → [share] にグループ化
    let mut servers: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for s in unc_shares {
        let p = PathBuf::from(s);
        if let Some((server, share)) = parse_unc(&p) {
            servers
                .entry(server.to_lowercase())
                .or_default()
                .push(share.to_lowercase());
        }
    }

    let mut server_roots: Vec<TreeNode> = Vec::with_capacity(servers.len());
    for (server, mut shares) in servers {
        shares.sort();
        shares.dedup();
        let server_path = PathBuf::from(format!("\\\\{server}"));
        let server_key = normalize_for_compare(&server_path);
        let server_node = if let Some(node) = existing_map.get(&server_key) {
            // 既存サーバノードを使い回す (expanded 状態を保持)
            node.clone()
        } else {
            TreeNode::with_kind(server_path.clone(), TreeNodeKind::UncServer)
        };

        let share_nodes: im::Vector<TreeNode> = shares
            .into_iter()
            .map(|share| {
                let share_path = PathBuf::from(format!("\\\\{server}\\{share}"));
                let key = normalize_for_compare(&share_path);
                if let Some(node) = existing_map.get(&key) {
                    node.clone()
                } else {
                    TreeNode::with_kind(share_path, TreeNodeKind::UncShare)
                }
            })
            .collect();
        // 仮想サーバノードの children を上書き
        server_node.children.set(share_nodes);
        server_roots.push(server_node);
    }

    let mut out: im::Vector<TreeNode> = im::Vector::new();
    for n in local_roots {
        out.push_back(n);
    }
    for n in server_roots {
        out.push_back(n);
    }
    out
}
