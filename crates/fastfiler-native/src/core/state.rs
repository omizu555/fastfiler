//! アプリの状態オブジェクト群。
//!
//! ここに置くもの:
//!   - `CounterSink` (ファイル監視の通知カウント)
//!   - `ModalKind` (新規/リネームのモーダル種別)
//!   - `DragState` (内部 D&D 状態)
//!   - `SplitterTarget` (どのスプリッタをドラッグ中か)
//!   - `PaneState` (1 タブ = 1 ペインの全シグナル + アクション)
//!   - `Tab` (1 つのタブ = 2D ペインレイアウト)
//!   - `AppState` (タブ集合 + 設定 + D&D / スプリッタ等のグローバル状態)
//!
//! 値は `RwSignal` / `Arc` で Clone 可能なので、ビュー間で安全に共有できる。

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use fastfiler_domain::events::EventSink;
use fastfiler_domain::watcher::WatcherCore;
use floem::kurbo::{Point, Rect};
use floem::reactive::{RwSignal, Scope, SignalGet, SignalUpdate, SignalWith};
use parking_lot::Mutex;

use crate::fs_model::{
    initial_path, pretty_title, read_folder, sort_rows, FileRow, History, SortKey,
    Stats,
};
use crate::settings::AppSettings;

// ────────────────────────────────────────────────────────────────
// CounterSink
// ────────────────────────────────────────────────────────────────

pub struct CounterSink {
    pub counter: Mutex<u32>,
    pub tx: crossbeam_channel::Sender<()>,
}
impl EventSink for CounterSink {
    fn emit_json(&self, _event: &str, _payload: serde_json::Value) {
        *self.counter.lock() += 1;
        let _ = self.tx.try_send(());
    }
}

// ────────────────────────────────────────────────────────────────
// ModalKind / SplitterTarget / DragState
// ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum ModalKind {
    None,
    NewFolder,
    #[allow(dead_code)]
    NewFile,
    /// 元の名前 (リネーム対象)
    Rename(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitterTarget {
    Tabs,
    Tree,
}

#[derive(Clone, Debug)]
pub struct DragState {
    pub source_pane: u64,
    pub paths: Vec<PathBuf>,
    pub start_window: Option<Point>,
    pub current_window: Point,
    pub active: bool,
}

// ────────────────────────────────────────────────────────────────
// PaneState
// ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PaneState {
    pub id: u64,
    pub title: RwSignal<String>,
    pub cur_path: RwSignal<PathBuf>,
    pub path_input: RwSignal<String>,
    pub rows: RwSignal<im::Vector<FileRow>>,
    pub stats: RwSignal<Stats>,
    pub selected: RwSignal<im::OrdSet<usize>>,
    /// 最後にクリックした行 (Shift+Click のアンカー / キーボード操作の起点)
    pub anchor: RwSignal<Option<usize>>,
    pub status_msg: RwSignal<String>,
    pub history: RwSignal<History>,
    pub watcher: Arc<WatcherCore>,
    pub sink: Arc<CounterSink>,
    /// 監視スレッドからのイベント受信用 (UI 側で signal 化)
    pub fs_rx: crossbeam_channel::Receiver<()>,
    pub watched: Arc<Mutex<Option<String>>>,
    pub fs_change_tick: RwSignal<u32>,
    pub show_hidden: RwSignal<bool>,
    /// モーダル種別 (新規フォルダ / リネーム)
    pub modal_kind: RwSignal<ModalKind>,
    pub modal_input: RwSignal<String>,
    pub sort_key: RwSignal<SortKey>,
    pub sort_desc: RwSignal<bool>,
    /// ファイル名フィルタ用クエリ (空なら全件表示)
    pub search_query: RwSignal<String>,
    /// 検索バー表示中か
    pub search_open: RwSignal<bool>,
    /// Everything 検索結果 (Some なら rows の代わりに表示。None なら通常表示)
    pub search_results: RwSignal<Option<im::Vector<FileRow>>>,
    /// Everything リクエスト世代 (古い結果を捨てる)
    pub search_request_gen: RwSignal<u64>,
}

static NEXT_PANE_ID: AtomicU64 = AtomicU64::new(1);

impl PaneState {
    pub fn new(start: PathBuf, show_hidden: RwSignal<bool>) -> Self {
        let mut initial_rows = read_folder(&start, show_hidden.get()).unwrap_or_default();
        sort_rows(&mut initial_rows, SortKey::Name, false);
        let initial_count = initial_rows.len();
        let (fs_tx, fs_rx) = crossbeam_channel::unbounded::<()>();
        // 全シグナルを untethered な Scope で生成する。これは
        // PaneState が click ハンドラやエフェクト内でも生成され得るため、
        // 親 scope の dispose に巻き込まれて signal が死ぬのを防ぐ目的。
        let s = Scope::new();
        Self {
            id: NEXT_PANE_ID.fetch_add(1, Ordering::Relaxed),
            title: s.create_rw_signal(pretty_title(&start)),
            cur_path: s.create_rw_signal(start.clone()),
            path_input: s.create_rw_signal(start.to_string_lossy().into_owned()),
            rows: s.create_rw_signal(initial_rows),
            stats: s.create_rw_signal(Stats {
                load_ms: 0.0,
                count: initial_count,
            }),
            selected: s.create_rw_signal(im::OrdSet::new()),
            anchor: s.create_rw_signal(None),
            status_msg: s.create_rw_signal(String::from("ready")),
            history: s.create_rw_signal(History::default()),
            watcher: Arc::new(WatcherCore::default()),
            sink: Arc::new(CounterSink {
                counter: Mutex::new(0),
                tx: fs_tx,
            }),
            fs_rx,
            watched: Arc::new(Mutex::new(None)),
            fs_change_tick: s.create_rw_signal(0),
            show_hidden,
            modal_kind: s.create_rw_signal(ModalKind::None),
            modal_input: s.create_rw_signal(String::new()),
            sort_key: s.create_rw_signal(SortKey::Name),
            sort_desc: s.create_rw_signal(false),
            search_query: s.create_rw_signal(String::new()),
            search_open: s.create_rw_signal(false),
            search_results: s.create_rw_signal(None),
            search_request_gen: s.create_rw_signal(0),
        }
    }

    /// 別フォルダへナビゲーションする。push_history=true で現在パスを back に積む。
    pub fn navigate(&self, target: PathBuf, push_history: bool) {
        if !target.is_dir() {
            self.status_msg
                .set(format!("not a directory: {}", target.display()));
            return;
        }
        let t = Instant::now();
        match read_folder(&target, self.show_hidden.get_untracked()) {
            Ok(mut v) => {
                sort_rows(&mut v, self.sort_key.get_untracked(), self.sort_desc.get_untracked());
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                let len = v.len();
                let prev_path = self.cur_path.get_untracked();
                let same_path = prev_path == target;
                if push_history && !same_path {
                    self.history.update(|h| {
                        h.back.push_back(prev_path.clone());
                        h.forward.clear();
                    });
                }
                // 変化時のみ set してシグナル通知の連鎖を抑える
                if !same_path {
                    self.cur_path.set(target.clone());
                    let s = target.to_string_lossy().into_owned();
                    if self.path_input.with_untracked(|p| p != &s) {
                        self.path_input.set(s);
                    }
                    let title_new = pretty_title(&target);
                    if self.title.with_untracked(|p| p != &title_new) {
                        self.title.set(title_new);
                    }
                }
                self.rows.set(v);
                if !self.selected.with_untracked(|s| s.is_empty()) {
                    self.selected.set(im::OrdSet::new());
                }
                if self.anchor.get_untracked().is_some() {
                    self.anchor.set(None);
                }
                self.stats.set(Stats {
                    load_ms: ms,
                    count: len,
                });
                if self.status_msg.with_untracked(|m| m != "ok") {
                    self.status_msg.set(String::from("ok"));
                }

                let s = target.to_string_lossy().into_owned();
                let mut wp = self.watched.lock();
                let need_rewatch = wp.as_deref() != Some(s.as_str());
                if need_rewatch {
                    if let Some(old) = wp.as_ref() {
                        self.watcher.unwatch(old);
                    }
                    *wp = Some(s.clone());
                    *self.sink.counter.lock() = 0;
                    if self.fs_change_tick.get_untracked() != 0 {
                        self.fs_change_tick.set(0);
                    }
                    let sd: Arc<dyn EventSink> = self.sink.clone();
                    let _ = self.watcher.watch_with_sink(s, sd);
                }
            }
            Err(e) => self.status_msg.set(format!("read failed: {}", e)),
        }
    }

    /// ファイル監視イベントによる軽量再読込。
    /// navigate と違い cur_path/title/path_input/history/watcher を触らず、
    /// rows と stats のみを差分検出して更新する。シグナル更新の連鎖を抑える。
    pub fn refresh_rows_only(&self) {
        let cur = self.cur_path.get_untracked();
        let show_hidden = self.show_hidden.get_untracked();
        let Ok(mut v) = read_folder(&cur, show_hidden) else { return; };
        sort_rows(&mut v, self.sort_key.get_untracked(), self.sort_desc.get_untracked());
        let new_len = v.len();
        // 簡易差分: 件数 + name + size + mtime が同じなら更新スキップ
        let same = self.rows.with_untracked(|r| {
            r.len() == new_len
                && r.iter().zip(v.iter()).all(|(a, b)| {
                    a.name == b.name && a.size == b.size && a.modified == b.modified
                })
        });
        if same {
            return;
        }
        // 削除等で行数が減った場合に備えて選択をクリア
        let cur_sel_max = self.selected.with_untracked(|s| s.iter().copied().max());
        if let Some(mx) = cur_sel_max {
            if mx >= new_len {
                self.selected.set(im::OrdSet::new());
                self.anchor.set(None);
            }
        }
        self.rows.set(v);
        self.stats.update(|s| s.count = new_len);
    }
}

// ────────────────────────────────────────────────────────────────
// Tab
// ────────────────────────────────────────────────────────────────

static NEXT_TAB_ID: AtomicU64 = AtomicU64::new(1);

/// 分割方向。Horizontal = 子要素が左右に並ぶ (⇔分割) / Vertical = 上下に並ぶ (⇕分割)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDir {
    Horizontal,
    Vertical,
}

/// BSP ペイン木。Leaf が個別ペイン、Split が分割ノード。
#[derive(Clone)]
pub enum SplitNode {
    Leaf(PaneState),
    Split {
        dir: SplitDir,
        children: Vec<SplitNode>,
    },
}

/// SplitNode を JSON に書き出すための Serializable 形 (PaneState の代わりに path のみ保持)
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum SavedSplit {
    Leaf { path: String },
    Split { dir: String, children: Vec<SavedSplit> },
}

impl SplitNode {
    pub fn to_saved(&self) -> SavedSplit {
        match self {
            SplitNode::Leaf(p) => SavedSplit::Leaf {
                path: p.cur_path.get_untracked().to_string_lossy().into_owned(),
            },
            SplitNode::Split { dir, children } => SavedSplit::Split {
                dir: match dir {
                    SplitDir::Horizontal => "h".to_string(),
                    SplitDir::Vertical => "v".to_string(),
                },
                children: children.iter().map(|c| c.to_saved()).collect(),
            },
        }
    }

    /// SavedSplit から SplitNode を構築。各 Leaf に新規 PaneState を作成する。
    pub fn from_saved(saved: &SavedSplit, show_hidden: RwSignal<bool>) -> Self {
        match saved {
            SavedSplit::Leaf { path } => {
                let p = PathBuf::from(path);
                let p = if p.exists() { p } else { PathBuf::from("C:\\") };
                SplitNode::Leaf(PaneState::new(p, show_hidden))
            }
            SavedSplit::Split { dir, children } => {
                let dir = if dir == "v" { SplitDir::Vertical } else { SplitDir::Horizontal };
                let mut kids = Vec::with_capacity(children.len());
                for c in children {
                    kids.push(SplitNode::from_saved(c, show_hidden));
                }
                if kids.is_empty() {
                    SplitNode::Leaf(PaneState::new(PathBuf::from("C:\\"), show_hidden))
                } else {
                    SplitNode::Split { dir, children: kids }
                }
            }
        }
    }
}

impl SplitNode {
    /// 全 leaf を deep-first で収集
    pub fn collect_leaves(&self, out: &mut Vec<PaneState>) {
        match self {
            SplitNode::Leaf(p) => out.push(p.clone()),
            SplitNode::Split { children, .. } => {
                for c in children {
                    c.collect_leaves(out);
                }
            }
        }
    }

    pub fn leaf_count(&self) -> usize {
        match self {
            SplitNode::Leaf(_) => 1,
            SplitNode::Split { children, .. } => children.iter().map(|c| c.leaf_count()).sum(),
        }
    }

    /// 指定 pane を含む Leaf を、同方向 Split に置換 (BSP分割)。
    /// 既に同方向の親 Split に属している場合は、その親の children に追加する (細切れ防止)。
    /// 戻り値: 分割成功なら true。
    pub fn split_leaf(&mut self, pane_id: u64, dir: SplitDir, new_pane: PaneState) -> bool {
        // 1. 自身が Split で、その子に対象 Leaf があり同方向なら子に追加
        if let SplitNode::Split { dir: my_dir, children } = self {
            if *my_dir == dir {
                for i in 0..children.len() {
                    if let SplitNode::Leaf(p) = &children[i] {
                        if p.id == pane_id {
                            children.insert(i + 1, SplitNode::Leaf(new_pane));
                            return true;
                        }
                    }
                }
            }
            // 子の中を再帰
            for c in children.iter_mut() {
                if c.split_leaf(pane_id, dir, new_pane.clone()) {
                    return true;
                }
            }
            return false;
        }
        // 2. 自身が Leaf で id 一致なら Split に変身
        if let SplitNode::Leaf(p) = self {
            if p.id == pane_id {
                let original = p.clone();
                *self = SplitNode::Split {
                    dir,
                    children: vec![SplitNode::Leaf(original), SplitNode::Leaf(new_pane)],
                };
                return true;
            }
        }
        false
    }

    /// 指定 pane を木から削除。子が 1 つになった Split は平坦化。
    /// 戻り値: 削除成功なら true。root が消滅する場合 (= leaf が pane_id しか無い) は false を返さず削除側で判定。
    pub fn remove_leaf(&mut self, pane_id: u64) -> bool {
        if let SplitNode::Split { children, .. } = self {
            // 直下の Leaf を検査
            if let Some(idx) = children.iter().position(|c| {
                matches!(c, SplitNode::Leaf(p) if p.id == pane_id)
            }) {
                children.remove(idx);
                self.collapse_if_single();
                return true;
            }
            // 再帰
            for c in children.iter_mut() {
                if c.remove_leaf(pane_id) {
                    self.collapse_if_single();
                    return true;
                }
            }
        }
        false
    }

    /// Split の子が 1 つだけになったら、その子で自身を置き換える
    fn collapse_if_single(&mut self) {
        if let SplitNode::Split { children, .. } = self {
            if children.len() == 1 {
                let only = children.remove(0);
                *self = only;
                // collapse 後さらに親 Split→Split のような連鎖を簡単化
                if let SplitNode::Split { children: sub, .. } = self {
                    if sub.len() == 1 {
                        let only = sub.remove(0);
                        *self = only;
                    }
                }
            }
        }
    }

    /// 最初の leaf (= タブタイトル取得用)
    pub fn first_leaf(&self) -> Option<PaneState> {
        match self {
            SplitNode::Leaf(p) => Some(p.clone()),
            SplitNode::Split { children, .. } => children.iter().find_map(|c| c.first_leaf()),
        }
    }
}

/// 1 タブ = 単一の SplitNode tree (BSP)。
/// タブ名は first_leaf のフォルダ名から派生する。
#[derive(Clone)]
pub struct Tab {
    pub id: u64,
    /// 分割木のルート
    pub root: RwSignal<SplitNode>,
    /// 現在フォーカスされているペイン id (split_active や close_pane の起点)
    pub active_pane: RwSignal<u64>,
}

impl Tab {
    pub fn new(start: PathBuf, show_hidden: RwSignal<bool>) -> Self {
        let p = PaneState::new(start, show_hidden);
        let pid = p.id;
        // Tab の signal も untethered scope に置く (click ハンドラから生成されることがあるため)
        let s = Scope::new();
        Self {
            id: NEXT_TAB_ID.fetch_add(1, Ordering::Relaxed),
            root: s.create_rw_signal(SplitNode::Leaf(p)),
            active_pane: s.create_rw_signal(pid),
        }
    }

    /// 永続化された SplitNode から Tab を復元
    pub fn from_saved(saved: &SavedSplit, show_hidden: RwSignal<bool>) -> Self {
        let node = SplitNode::from_saved(saved, show_hidden);
        let mut leaves = Vec::new();
        node.collect_leaves(&mut leaves);
        let pid = leaves.first().map(|p| p.id).unwrap_or(0);
        let s = Scope::new();
        Self {
            id: NEXT_TAB_ID.fetch_add(1, Ordering::Relaxed),
            root: s.create_rw_signal(node),
            active_pane: s.create_rw_signal(pid),
        }
    }

    pub fn primary(&self) -> PaneState {
        self.root.with(|r| r.first_leaf()).expect("tab must have at least one pane")
    }

    pub fn all_panes(&self) -> Vec<PaneState> {
        self.root.with(|r| {
            let mut out = Vec::new();
            r.collect_leaves(&mut out);
            out
        })
    }

    pub fn pane_count(&self) -> usize {
        self.root.with(|r| r.leaf_count())
    }
}

// ────────────────────────────────────────────────────────────────
// AppState
// ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub tabs: RwSignal<im::Vector<Tab>>,
    /// active タブ id
    pub active: RwSignal<u64>,
    pub tab_cols: RwSignal<usize>,
    pub settings: AppSettings,
    pub settings_open: RwSignal<bool>,
    pub pane_rects: RwSignal<im::HashMap<u64, Rect>>,
    pub dragging: RwSignal<Option<DragState>>,
    /// スプリッタドラッグ中のターゲット (タブペイン / ツリーペイン 右端)
    pub splitter_drag: RwSignal<Option<SplitterTarget>>,
    /// FS 変化通知 (ツリーペイン等が track して再ロードするためのグローバルティック)
    pub tree_tick: RwSignal<u64>,
    /// テーマ/プリセット/アクセント変更時にインクリメントするリビジョン。
    /// app_view ルートの dyn_container がこれを track して全 UI を再構築し
    /// theme:: の関数評価を全 view で再走させる (即時反映)。
    pub theme_rev: RwSignal<u32>,
}

impl AppState {
    pub fn new(start: PathBuf) -> Self {
        let settings = AppSettings::new();

        // 起動時タブ復元: tab_layouts (BSP 構造) を最優先、次に open_tabs (パスのみ)
        let saved_paths = settings.open_tabs.get_untracked();
        let saved_layouts = settings.tab_layouts.get_untracked();
        let mut tabs_vec: im::Vector<Tab> = im::Vector::new();
        if !saved_layouts.is_empty() {
            for json in &saved_layouts {
                if let Ok(saved) = serde_json::from_str::<SavedSplit>(json) {
                    tabs_vec.push_back(Tab::from_saved(&saved, settings.show_hidden));
                }
            }
        }
        if tabs_vec.is_empty() {
            for p in &saved_paths {
                let path = PathBuf::from(p);
                if path.exists() {
                    tabs_vec.push_back(Tab::new(path, settings.show_hidden));
                }
            }
        }
        if tabs_vec.is_empty() {
            tabs_vec.push_back(Tab::new(start, settings.show_hidden));
        }
        let id = tabs_vec.front().map(|t| t.id).unwrap_or(0);

        let initial_cols = settings
            .tab_columns
            .get_untracked()
            .parse::<usize>()
            .unwrap_or(1)
            .clamp(1, 4);
        Self {
            tabs: RwSignal::new(tabs_vec),
            active: RwSignal::new(id),
            tab_cols: RwSignal::new(initial_cols),
            settings,
            settings_open: RwSignal::new(false),
            pane_rects: RwSignal::new(im::HashMap::new()),
            dragging: RwSignal::new(None),
            splitter_drag: RwSignal::new(None),
            tree_tick: RwSignal::new(0),
            theme_rev: RwSignal::new(0),
        }
    }

    pub fn active_tab(&self) -> Option<Tab> {
        let id = self.active.get();
        self.tabs.get().iter().find(|t| t.id == id).cloned()
    }

    /// 互換用: アクティブタブの primary ペイン
    pub fn active_pane(&self) -> Option<PaneState> {
        self.active_tab().map(|t| t.primary())
    }

    pub fn add_tab(&self, start: PathBuf) {
        let tab = Tab::new(start, self.settings.show_hidden);
        let id = tab.id;
        self.tabs.update(|t| t.push_back(tab));
        if self.active.get_untracked() != id {
            self.active.set(id);
        }
    }

    /// active タブを delta 個分シフトする (-1=上/左, +1=下/右)。範囲外ならクランプ。
    pub fn shift_active_tab(&self, delta: i32) {
        let active_id = self.active.get_untracked();
        self.tabs.update(|t| {
            if let Some(idx) = t.iter().position(|x| x.id == active_id) {
                let new_idx = (idx as i32 + delta).clamp(0, t.len() as i32 - 1) as usize;
                if new_idx != idx {
                    let item = t.remove(idx);
                    t.insert(new_idx, item);
                }
            }
        });
    }

    /// from_id のタブを to_id の位置に並び替える。同じなら何もしない。
    pub fn close_tab(&self, id: u64) {
        let prev_active = self.active.get_untracked();
        self.tabs.update(|t| {
            if let Some(idx) = t.iter().position(|x| x.id == id) {
                t.remove(idx);
            }
        });
        let remaining = self.tabs.get_untracked();
        if remaining.is_empty() {
            self.add_tab(initial_path());
        } else if !remaining.iter().any(|t| t.id == prev_active) {
            if let Some(last) = remaining.last() {
                if last.id != prev_active {
                    self.active.set(last.id);
                }
            }
        }
    }

    /// 指定 pane id を保持するペインを全タブから検索
    pub fn find_pane(&self, pane_id: u64) -> Option<PaneState> {
        self.tabs.with(|tabs| {
            for t in tabs.iter() {
                for p in t.all_panes() {
                    if p.id == pane_id {
                        return Some(p);
                    }
                }
            }
            None
        })
    }

    /// アクティブペインを BSP 分割。
    /// vertical=false → ⇔分割 (Horizontal: 選択ペインが左右に分かれる)
    /// vertical=true  → ⇕分割 (Vertical:   選択ペインが上下に分かれる)
    pub fn split_active(&self, vertical: bool) {
        if let Some(tab) = self.active_tab() {
            if tab.pane_count() >= 4 {
                crate::flog!("[split] denied (>=4 panes)");
                return;
            }
            let active_id = tab.active_pane.get_untracked();
            let dir = if vertical { SplitDir::Vertical } else { SplitDir::Horizontal };
            let base = tab
                .all_panes()
                .into_iter()
                .find(|p| p.id == active_id)
                .map(|p| p.cur_path.get_untracked())
                .unwrap_or_else(|| tab.primary().cur_path.get_untracked());
            let show_hidden = self.settings.show_hidden;
            let new_pane = PaneState::new(base, show_hidden);
            let new_id = new_pane.id;

            let mut ok = false;
            tab.root.update(|r| {
                ok = r.split_leaf(active_id, dir, new_pane.clone());
            });
            crate::flog!("[split] dir={:?} active={} ok={} new_id={}", dir, active_id, ok, new_id);
            if ok {
                tab.active_pane.set(new_id);
            }
        }
    }

    /// 指定 pane を木から削除 (タブ最後の 1 ペインは削除不可)
    pub fn close_pane(&self, pane_id: u64) {
        self.tabs.with(|tabs| {
            for t in tabs.iter() {
                if t.pane_count() <= 1 {
                    continue;
                }
                let mut found = false;
                t.root.update(|r| {
                    found = r.remove_leaf(pane_id);
                });
                if found {
                    if let Some(first) = t.all_panes().first() {
                        if t.active_pane.get_untracked() == pane_id {
                            t.active_pane.set(first.id);
                        }
                    }
                    return;
                }
            }
        });
    }
}
