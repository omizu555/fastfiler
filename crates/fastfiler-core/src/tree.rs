//! ワークスペースツリー (F-801〜F-803)。ドライブ + UNC サーバ/share 起点、
//! 遅延展開、フォルダのみ。現在地への自動追従 (展開 + ハイライト + 中央スクロール)。

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::effect::Effect;

pub const DEFAULT_TREE_W: f32 = 220.0;
pub const TREE_W_MIN: f32 = 120.0;
pub const TREE_W_MAX: f32 = 500.0;
/// ツリーの行高。
pub const TREE_ROW_H: f32 = 22.0;

#[derive(Debug, Clone, PartialEq)]
pub struct TreeChild {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug)]
pub struct TreeState {
    pub visible: bool,
    pub width: f32,
    /// 展開中のパス集合。
    pub expanded: BTreeSet<PathBuf>,
    /// ロード済みの子 (フォルダのみ)。key = 親パス。
    pub children: HashMap<PathBuf, Vec<TreeChild>>,
    /// ドライブ行 (表示名 例 "OS (C:)" / "D:", ルートパス 例 "C:\")。
    /// DrivesLoaded 時に前計算する (rows() は描画のたびに呼ばれるため)。
    pub drives: Vec<(String, PathBuf)>,
    /// 登録済み UNC share (`\\server\share`)。セッション永続化対象。
    pub unc_shares: Vec<String>,
    /// 現在地 (フォーカスペインのフォルダ — ハイライト対象)。
    pub current: Option<PathBuf>,
    pub scroll: f32,
    pub viewport_h: f32,
    /// 次の描画で中央スクロールする行 (自動追従)。
    pub reveal_ix: Option<usize>,
}

impl Default for TreeState {
    fn default() -> Self {
        Self {
            visible: true,
            width: DEFAULT_TREE_W,
            expanded: BTreeSet::new(),
            children: HashMap::new(),
            drives: Vec::new(),
            unc_shares: Vec::new(),
            current: None,
            scroll: 0.0,
            viewport_h: 0.0,
            reveal_ix: None,
        }
    }
}

/// 表示 1 行 (フラット化済み)。
#[derive(Debug, Clone, PartialEq)]
pub struct TreeRow {
    pub depth: u16,
    pub label: String,
    /// クリックで開くパス。サーバノードは None (コンテナのみ — CONTEXT.md)。
    pub path: Option<PathBuf>,
    pub expanded: bool,
    /// 展開矢印を出すか (サーバ/ドライブ/フォルダ = true、空と判明したら false)。
    pub expandable: bool,
    /// サーバノード (`\\server`) のとき Some(サーバ名) — 右クリック削除の対象。
    pub server: Option<String>,
    pub is_current: bool,
}

#[derive(Debug, Clone)]
pub enum TreeMsg {
    ToggleVisible,
    SetWidth(f32),
    /// 展開/折りたたみ (矢印クリック)。
    Toggle(PathBuf),
    DrivesLoaded(Vec<(String, String)>),
    ChildrenLoaded {
        path: PathBuf,
        dirs: Vec<TreeChild>,
    },
    Scrolled(f32),
    ViewportChanged {
        height: f32,
    },
    /// サーバノードの登録解除 (配下の share をまとめて削除)。
    RemoveServer(String),
}

impl TreeState {
    /// ツリーの入力処理 (ノードクリック = ペインへ開く、は update_app が担当)。
    pub fn update(&mut self, msg: TreeMsg) -> Vec<Effect> {
        match msg {
            TreeMsg::ToggleVisible => {
                self.visible = !self.visible;
                vec![Effect::ScheduleSessionSave]
            }
            TreeMsg::SetWidth(w) => {
                self.width = w.clamp(TREE_W_MIN, TREE_W_MAX);
                vec![Effect::ScheduleSessionSave]
            }
            TreeMsg::Toggle(path) => {
                if self.expanded.contains(&path) {
                    self.expanded.remove(&path);
                    vec![]
                } else {
                    self.expanded.insert(path.clone());
                    if self.children.contains_key(&path) {
                        vec![]
                    } else {
                        vec![Effect::LoadTreeChildren { path }]
                    }
                }
            }
            TreeMsg::DrivesLoaded(drives) => {
                // domain の list_drives は letter を "C:\" (末尾 \ 付き) で返す。
                // "C:" 前提で "\" を付け足すと "C:\\" 起点の二重区切りが
                // 子孫パス全体へ伝播するため、ここで剥がしてから表示名と
                // ルートパスを前計算する
                self.drives = drives
                    .into_iter()
                    .map(|(letter, label)| {
                        let letter = letter.trim_end_matches('\\');
                        let display = if label.is_empty() {
                            letter.to_string()
                        } else {
                            format!("{label} ({letter})")
                        };
                        (display, PathBuf::from(format!("{letter}\\")))
                    })
                    .collect();
                vec![]
            }
            TreeMsg::ChildrenLoaded { path, dirs } => {
                self.children.insert(path, dirs);
                // 遅延ロード完了後に現在地の行位置を再計算 (深いパスへの初回追従)
                if let Some(cur) = self.current.clone() {
                    self.reveal_ix = self
                        .rows()
                        .iter()
                        .position(|r| r.path.as_deref() == Some(cur.as_path()));
                }
                vec![]
            }
            TreeMsg::Scrolled(o) => {
                self.scroll = o.max(0.0);
                vec![]
            }
            TreeMsg::ViewportChanged { height } => {
                self.viewport_h = height;
                vec![]
            }
            TreeMsg::RemoveServer(server) => {
                let prefix = format!("\\\\{server}\\");
                self.unc_shares.retain(|s| !s.starts_with(&prefix));
                vec![Effect::ScheduleSessionSave]
            }
        }
    }

    /// UNC パスを開いたときの自動登録 (F-802)。`\\server\share\…` → `\\server\share`。
    pub fn register_unc(&mut self, path: &Path) -> bool {
        let s = path.to_string_lossy();
        let Some(rest) = s.strip_prefix("\\\\") else {
            return false;
        };
        let mut it = rest.split('\\');
        let (Some(server), Some(share)) = (it.next(), it.next()) else {
            return false;
        };
        if server.is_empty() || share.is_empty() {
            return false;
        }
        let root = format!("\\\\{server}\\{share}");
        if self
            .unc_shares
            .iter()
            .any(|x| x.eq_ignore_ascii_case(&root))
        {
            return false;
        }
        self.unc_shares.push(root);
        true
    }

    /// 現在地への追従 (F-803): 祖先を展開し、未ロードの子を読み込み要求、
    /// reveal_ix を立てる。フォルダ移動のたびに update_app から呼ばれる。
    pub fn reveal(&mut self, path: &Path) -> Vec<Effect> {
        self.current = Some(path.to_path_buf());
        let mut effects = Vec::new();
        // UNC はサーバコンテナノード (\\server) も展開しないと share 行が現れない
        let s = path.to_string_lossy();
        if let Some(rest) = s.strip_prefix("\\\\") {
            if let Some((server, _)) = rest.split_once('\\') {
                self.expanded.insert(PathBuf::from(format!("\\\\{server}")));
            }
        }
        // 祖先チェーン (ルート → path の親まで) を展開
        let mut anc = Vec::new();
        let mut cur = path.to_path_buf();
        while let Some(parent) = cur.parent() {
            anc.push(parent.to_path_buf());
            cur = parent.to_path_buf();
        }
        for a in anc.into_iter().rev() {
            if a.as_os_str().is_empty() {
                continue;
            }
            self.expanded.insert(a.clone());
            if !self.children.contains_key(&a) {
                effects.push(Effect::LoadTreeChildren { path: a });
            }
        }
        // reveal 位置は rows() から算出 (ロード完了後に再計算されるまで近似で良い)
        self.reveal_ix = self
            .rows()
            .iter()
            .position(|r| r.path.as_deref() == Some(path));
        effects
    }

    /// フラット化した表示行。ドライブ群 → サーバ/share 群 の順 (CONTEXT.md)。
    pub fn rows(&self) -> Vec<TreeRow> {
        let mut out = Vec::new();
        for (display, path) in &self.drives {
            self.push_dir_row(&mut out, 0, display.clone(), path.clone());
        }
        // UNC: サーバ毎にグルーピング (サーバノードは開けないコンテナ)
        let mut servers: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for share in &self.unc_shares {
            if let Some(rest) = share.strip_prefix("\\\\") {
                if let Some((server, _)) = rest.split_once('\\') {
                    servers
                        .entry(server.to_string())
                        .or_default()
                        .push(share.clone());
                }
            }
        }
        for (server, mut shares) in servers {
            shares.sort();
            let server_key = PathBuf::from(format!("\\\\{server}"));
            let expanded = self.expanded.contains(&server_key);
            out.push(TreeRow {
                depth: 0,
                label: format!("🌐 {server}"),
                path: None,
                expanded,
                expandable: true,
                server: Some(server.clone()),
                is_current: false,
            });
            if expanded {
                for share in shares {
                    let name = share
                        .rsplit('\\')
                        .next()
                        .unwrap_or(share.as_str())
                        .to_string();
                    self.push_dir_row(&mut out, 1, name, PathBuf::from(&share));
                }
            }
        }
        out
    }

    fn push_dir_row(&self, out: &mut Vec<TreeRow>, depth: u16, label: String, path: PathBuf) {
        let expanded = self.expanded.contains(&path);
        let expandable = match self.children.get(&path) {
            Some(c) => !c.is_empty(),
            None => true, // 未ロードは矢印を出す (展開時にロード)
        };
        out.push(TreeRow {
            depth,
            label,
            path: Some(path.clone()),
            expanded,
            expandable,
            server: None,
            is_current: self.current.as_deref() == Some(path.as_path()),
        });
        if expanded {
            if let Some(children) = self.children.get(&path) {
                for c in children {
                    self.push_dir_row(out, depth + 1, c.name.clone(), c.path.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> TreeState {
        let mut t = TreeState::default();
        t.update(TreeMsg::DrivesLoaded(vec![
            ("C:".into(), "OS".into()),
            ("D:".into(), String::new()),
        ]));
        t
    }

    #[test]
    fn toggle_loads_children_once_and_flattens() {
        let mut t = tree();
        let c = PathBuf::from("C:\\");
        let fx = t.update(TreeMsg::Toggle(c.clone()));
        assert_eq!(fx, vec![Effect::LoadTreeChildren { path: c.clone() }]);
        t.update(TreeMsg::ChildrenLoaded {
            path: c.clone(),
            dirs: vec![
                TreeChild {
                    name: "Users".into(),
                    path: PathBuf::from("C:\\Users"),
                },
                TreeChild {
                    name: "Windows".into(),
                    path: PathBuf::from("C:\\Windows"),
                },
            ],
        });
        let rows = t.rows();
        let labels: Vec<_> = rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["OS (C:)", "Users", "Windows", "D:"]);
        assert_eq!(rows[1].depth, 1);
        // 再トグルは追加ロードなしで折りたたみ
        assert!(t.update(TreeMsg::Toggle(c.clone())).is_empty());
        assert_eq!(t.rows().len(), 2);
        // ロード済みの再展開もロードなし
        assert!(t.update(TreeMsg::Toggle(c)).is_empty());
    }

    #[test]
    fn drives_loaded_strips_trailing_separator() {
        // domain の list_drives は "D:\" 形式 — そのまま受けると rows() が
        // "D:\\" を作り、二重区切りが子孫へ伝播する (実機報告)
        let mut t = TreeState::default();
        t.update(TreeMsg::DrivesLoaded(vec![
            ("D:\\".into(), String::new()),
            ("C:\\".into(), "OS".into()),
        ]));
        let rows = t.rows();
        assert_eq!(rows[0].label, "D:");
        assert_eq!(
            rows[0].path.as_ref().unwrap().to_string_lossy(),
            "D:\\" // ルートは "D:\" ちょうど (二重にならない)
        );
        assert_eq!(rows[1].label, "OS (C:)");
    }

    #[test]
    fn unc_register_and_grouping_and_remove() {
        let mut t = tree();
        assert!(t.register_unc(Path::new("\\\\nas\\media\\videos\\a.mp4")));
        assert!(!t.register_unc(Path::new("\\\\nas\\media\\other"))); // 同一 share は一度だけ
        assert!(t.register_unc(Path::new("\\\\nas\\docs")));
        assert!(!t.register_unc(Path::new("C:\\local"))); // UNC でない
        let rows = t.rows();
        let server = rows.iter().find(|r| r.server.is_some()).unwrap();
        assert_eq!(server.label, "🌐 nas");
        assert!(server.path.is_none()); // サーバノードは開けない
                                        // 展開すると share が並ぶ
        t.update(TreeMsg::Toggle(PathBuf::from("\\\\nas")));
        let rows = t.rows();
        let shares: Vec<_> = rows
            .iter()
            .filter(|r| r.depth == 1)
            .map(|r| r.label.as_str())
            .collect();
        assert_eq!(shares, ["docs", "media"]);
        // サーバ削除で share がまとめて消える
        t.update(TreeMsg::RemoveServer("nas".into()));
        assert!(t.unc_shares.is_empty());
    }

    #[test]
    fn reveal_expands_ancestors_and_requests_loads() {
        let mut t = tree();
        let fx = t.reveal(Path::new("C:\\Users\\me\\docs"));
        // C:\ と C:\Users と C:\Users\me の 3 階層をロード要求
        assert_eq!(fx.len(), 3);
        assert!(t.expanded.contains(Path::new("C:\\")));
        assert!(t.expanded.contains(Path::new("C:\\Users")));
        assert!(t.expanded.contains(Path::new("C:\\Users\\me")));
        assert_eq!(t.current.as_deref(), Some(Path::new("C:\\Users\\me\\docs")));
        // 2 回目は追加ロードなし (ロード済みなら)
        t.children.insert(PathBuf::from("C:\\"), vec![]);
        t.children.insert(PathBuf::from("C:\\Users"), vec![]);
        t.children.insert(PathBuf::from("C:\\Users\\me"), vec![]);
        assert!(t.reveal(Path::new("C:\\Users\\me\\docs")).is_empty());
    }
}
