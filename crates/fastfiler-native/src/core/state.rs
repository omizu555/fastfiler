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

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use fastfiler_domain::events::EventSink;
use fastfiler_domain::watcher::WatcherCore;
use floem::kurbo::{Point, Rect};
use floem::reactive::{RwSignal, Scope, SignalGet, SignalUpdate, SignalWith};
use parking_lot::Mutex;

use crate::fs_model::{
    initial_path, pretty_title, read_folder, sort_rows, FileRow, History, SortKey, Stats,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModalKind {
    None,
    NewFolder,
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
    /// 右ボタンドラッグ。drop 時に context menu を出す。
    pub right_button: bool,
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
    pub name_col_width: RwSignal<f32>,
    pub size_col_width: RwSignal<f32>,
    pub mtime_col_width: RwSignal<f32>,
}

static NEXT_PANE_ID: AtomicU64 = AtomicU64::new(1);

impl PaneState {
    pub fn new(start: PathBuf, show_hidden: RwSignal<bool>) -> Self {
        Self::new_with_columns(
            start,
            show_hidden,
            def_name_col_width(),
            def_size_col_width(),
            def_mtime_col_width(),
        )
    }

    pub fn new_with_columns(
        start: PathBuf,
        show_hidden: RwSignal<bool>,
        name_col_width: f32,
        size_col_width: f32,
        mtime_col_width: f32,
    ) -> Self {
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
            name_col_width: s.create_rw_signal(name_col_width.clamp(120.0, 1200.0)),
            size_col_width: s.create_rw_signal(size_col_width.clamp(70.0, 600.0)),
            mtime_col_width: s.create_rw_signal(mtime_col_width.clamp(90.0, 600.0)),
        }
    }

    /// 別フォルダへナビゲーションする。push_history=true で現在パスを back に積む。
    pub fn navigate(&self, target: PathBuf, push_history: bool) {
        if !target.is_dir() {
            self.status_msg
                .set(format!("not a directory: {}", target.display()));
            return;
        }
        let _g_nav = crate::core::perf::scope(
            crate::core::perf::MetricKind::Navigate,
            target.to_string_lossy().into_owned(),
        );
        let t = Instant::now();
        let v = match self.load_sorted_rows(&target) {
            Ok(v) => v,
            Err(e) => {
                self.status_msg.set(format!("read failed: {}", e));
                return;
            }
        };
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let len = v.len();
        crate::core::perf::record_manual(
            crate::core::perf::MetricKind::ListDir,
            format!("{} rows={}", target.display(), len),
            ms,
        );

        let prev_path = self.cur_path.get_untracked();
        let same_path = prev_path == target;
        self.commit_path_change(&target, &prev_path, push_history, same_path);

        // 行・選択・統計を更新する (変化時のみ set してシグナル通知の連鎖を抑える)
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

        self.rewatch(&target);
    }

    /// 現在のソート設定で `path` を読み込み、ソート済みの行を返す。
    /// navigate / refresh_rows_only から共有する。
    fn load_sorted_rows(&self, path: &Path) -> Result<im::Vector<FileRow>, String> {
        let mut v = read_folder(path, self.show_hidden.get_untracked())?;
        sort_rows(
            &mut v,
            self.sort_key.get_untracked(),
            self.sort_desc.get_untracked(),
        );
        Ok(v)
    }

    /// ナビゲーション時のパス系シグナル更新 (history / cur_path / path_input / title)。
    /// 変化時のみ set してシグナル通知の連鎖を抑える。
    fn commit_path_change(
        &self,
        target: &Path,
        prev_path: &Path,
        push_history: bool,
        same_path: bool,
    ) {
        if push_history && !same_path {
            self.history.update(|h| {
                h.back.push_back(prev_path.to_path_buf());
                h.forward.clear();
            });
        }
        if same_path {
            return;
        }
        self.cur_path.set(target.to_path_buf());
        let s = target.to_string_lossy().into_owned();
        if self.path_input.with_untracked(|p| p != &s) {
            self.path_input.set(s);
        }
        let title_new = pretty_title(target);
        if self.title.with_untracked(|p| p != &title_new) {
            self.title.set(title_new);
        }
    }

    /// `path` に対する FS 監視を張り直す。監視中パスと同一なら何もしない。
    fn rewatch(&self, path: &Path) {
        let s = path.to_string_lossy().into_owned();
        let mut wp = self.watched.lock();
        if wp.as_deref() == Some(s.as_str()) {
            return;
        }
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

    /// ファイル監視イベントによる軽量再読込。
    /// navigate と違い cur_path/title/path_input/history/watcher を触らず、
    /// rows と stats のみを差分検出して更新する。シグナル更新の連鎖を抑える。
    pub fn refresh_rows_only(&self) {
        let cur = self.cur_path.get_untracked();
        let Ok(v) = self.load_sorted_rows(&cur) else {
            return;
        };
        let new_len = v.len();
        // 簡易差分: 件数 + name + size + mtime が同じなら更新スキップ
        let same = self.rows.with_untracked(|r| {
            r.len() == new_len
                && r.iter()
                    .zip(v.iter())
                    .all(|(a, b)| a.name == b.name && a.size == b.size && a.modified == b.modified)
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

/// ペイン分割内スプリッタのドラッグ状態
#[derive(Clone)]
pub struct PaneSplitterDrag {
    pub dir: SplitDir,
    pub ratios: RwSignal<Vec<f32>>,
    /// 隣接 2 ペインのインデックス (左/上 が idx_a、右/下 が idx_a + 1)
    pub idx_a: usize,
}

/// BSP ペイン木。Leaf が個別ペイン、Split が分割ノード。
#[derive(Clone)]
pub enum SplitNode {
    Leaf(PaneState),
    Split {
        dir: SplitDir,
        children: Vec<SplitNode>,
        /// 各子の表示比率 (合計 1.0 を維持)。リアクティブに更新可能。
        ratios: RwSignal<Vec<f32>>,
    },
}

/// children 数に応じた均等 ratios を生成 (untethered スコープで生成して自動 dispose 回避)
fn make_equal_ratios(n: usize) -> RwSignal<Vec<f32>> {
    let v = if n == 0 {
        Vec::new()
    } else {
        let r = 1.0 / n as f32;
        vec![r; n]
    };
    floem::reactive::Scope::new().create_rw_signal(v)
}

/// ratios を合計 1.0 に正規化
fn normalize_ratios(v: &mut [f32]) {
    let sum: f32 = v.iter().sum();
    if sum > 0.0001 {
        for r in v.iter_mut() {
            *r /= sum;
        }
    } else if !v.is_empty() {
        let r = 1.0 / v.len() as f32;
        for x in v.iter_mut() {
            *x = r;
        }
    }
}

fn def_name_col_width() -> f32 {
    280.0
}

fn def_size_col_width() -> f32 {
    110.0
}

fn def_mtime_col_width() -> f32 {
    140.0
}

/// SplitNode を JSON に書き出すための Serializable 形
/// (PaneState の代わりに path + 列幅のみ保持)
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum SavedSplit {
    Leaf {
        path: String,
        #[serde(default = "def_name_col_width")]
        name_col_width: f32,
        #[serde(default = "def_size_col_width")]
        size_col_width: f32,
        #[serde(default = "def_mtime_col_width")]
        mtime_col_width: f32,
    },
    Split {
        dir: String,
        children: Vec<SavedSplit>,
        #[serde(default)]
        ratios: Vec<f32>,
    },
}

impl SplitNode {
    pub fn to_saved(&self) -> SavedSplit {
        match self {
            SplitNode::Leaf(p) => SavedSplit::Leaf {
                path: p.cur_path.get_untracked().to_string_lossy().into_owned(),
                name_col_width: p.name_col_width.get_untracked(),
                size_col_width: p.size_col_width.get_untracked(),
                mtime_col_width: p.mtime_col_width.get_untracked(),
            },
            SplitNode::Split {
                dir,
                children,
                ratios,
            } => SavedSplit::Split {
                dir: match dir {
                    SplitDir::Horizontal => "h".to_string(),
                    SplitDir::Vertical => "v".to_string(),
                },
                children: children.iter().map(|c| c.to_saved()).collect(),
                ratios: ratios.get_untracked(),
            },
        }
    }

    /// SavedSplit から SplitNode を構築。各 Leaf に新規 PaneState を作成する。
    pub fn from_saved(saved: &SavedSplit, show_hidden: RwSignal<bool>) -> Self {
        match saved {
            SavedSplit::Leaf {
                path,
                name_col_width,
                size_col_width,
                mtime_col_width,
            } => {
                let p = PathBuf::from(path);
                let p = if p.exists() { p } else { PathBuf::from("C:\\") };
                SplitNode::Leaf(PaneState::new_with_columns(
                    p,
                    show_hidden,
                    *name_col_width,
                    *size_col_width,
                    *mtime_col_width,
                ))
            }
            SavedSplit::Split {
                dir,
                children,
                ratios,
            } => {
                let dir = if dir == "v" {
                    SplitDir::Vertical
                } else {
                    SplitDir::Horizontal
                };
                let mut kids = Vec::with_capacity(children.len());
                for c in children {
                    kids.push(SplitNode::from_saved(c, show_hidden));
                }
                if kids.is_empty() {
                    SplitNode::Leaf(PaneState::new(PathBuf::from("C:\\"), show_hidden))
                } else {
                    // ratios の長さが children と一致しない/空なら均等配分。
                    let mut r = if ratios.len() == kids.len() && !ratios.is_empty() {
                        ratios.clone()
                    } else {
                        vec![1.0 / kids.len() as f32; kids.len()]
                    };
                    normalize_ratios(&mut r);
                    SplitNode::Split {
                        dir,
                        children: kids,
                        ratios: floem::reactive::Scope::new().create_rw_signal(r),
                    }
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
        if let SplitNode::Split {
            dir: my_dir,
            children,
            ratios,
        } = self
        {
            if *my_dir == dir {
                for i in 0..children.len() {
                    if let SplitNode::Leaf(p) = &children[i] {
                        if p.id == pane_id {
                            children.insert(i + 1, SplitNode::Leaf(new_pane));
                            // i 番目の ratio を半分に分け、新規子に半分を与える
                            ratios.update(|v| {
                                let half = v.get(i).copied().unwrap_or(0.0) * 0.5;
                                if let Some(r) = v.get_mut(i) {
                                    *r = half;
                                }
                                v.insert(i + 1, half);
                                normalize_ratios(v);
                            });
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
                    ratios: make_equal_ratios(2),
                };
                return true;
            }
        }
        false
    }

    /// 指定 pane を木から削除。子が 1 つになった Split は平坦化。
    /// 戻り値: 削除成功なら true。root が消滅する場合 (= leaf が pane_id しか無い) は false を返さず削除側で判定。
    pub fn remove_leaf(&mut self, pane_id: u64) -> bool {
        if let SplitNode::Split {
            children, ratios, ..
        } = self
        {
            // 直下の Leaf を検査
            if let Some(idx) = children
                .iter()
                .position(|c| matches!(c, SplitNode::Leaf(p) if p.id == pane_id))
            {
                children.remove(idx);
                ratios.update(|v| {
                    if idx < v.len() {
                        v.remove(idx);
                    }
                    normalize_ratios(v);
                });
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
    /// ロック中 (true) は Ctrl+W / × ボタンで閉じない (中クリックでトグル)
    pub locked: RwSignal<bool>,
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
            locked: s.create_rw_signal(false),
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
            locked: s.create_rw_signal(false),
        }
    }

    pub fn primary(&self) -> PaneState {
        self.root
            .with(|r| r.first_leaf())
            .expect("tab must have at least one pane")
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

    pub fn active_pane_resolved(&self) -> Option<PaneState> {
        let active_id = self.active_pane.get_untracked();
        let panes = self.all_panes();
        if panes.is_empty() {
            return None;
        }
        if let Some(p) = panes.iter().find(|p| p.id == active_id).cloned() {
            return Some(p);
        }
        let fallback = panes[0].clone();
        if active_id != fallback.id {
            self.active_pane.set(fallback.id);
        }
        Some(fallback)
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
    /// ペイン分割内スプリッタのドラッグ状態
    pub pane_splitter_drag: RwSignal<Option<PaneSplitterDrag>>,
    /// FS 変化通知 (ツリーペイン等が track して再ロードするためのグローバルティック)
    pub tree_tick: RwSignal<u64>,
    /// テーマ/プリセット/アクセント変更時にインクリメントするリビジョン。
    /// app_view ルートの dyn_container がこれを track して全 UI を再構築し
    /// theme:: の関数評価を全 view で再走させる (即時反映)。
    pub theme_rev: RwSignal<u32>,
    /// Undo 履歴 (ADR 0006/0008)。アプリ全体で 1 本、N=20、起動間で保持しない。
    pub undo_manager: Arc<Mutex<fastfiler_domain::undo::UndoManager>>,
    /// `toggle-tree` / `toggle-tabs` の復帰先記憶 (in-memory、起動間で保持しない)。
    /// `hidden` に切替える直前の `panel_dock_*` 値を覚え、再表示時に復元する。
    pub panel_dock_tree_prev: RwSignal<String>,
    pub panel_dock_tabs_prev: RwSignal<String>,
    /// プログレスダイアログ用のジョブ管理 (#5)
    pub jobs: crate::core::jobs::JobsState,
    /// ワークスペースツリーの roots (起動時に list_drives() で初期化、UNC 追加等で拡張)
    pub tree_roots: RwSignal<im::Vector<crate::core::tree_model::TreeNode>>,
    /// ツリーペインがキーボードフォーカスを保持しているか (FocusGained/Lost で更新)
    pub tree_focused: RwSignal<bool>,
    /// ツリーペイン上でのキーフォーカス位置 (None なら未確定)
    pub tree_focused_path: RwSignal<Option<PathBuf>>,
    /// 値が変わるたびにツリーペイン container が request_focus する (Ctrl+Shift+E 等)
    pub tree_focus_req: RwSignal<u64>,
    /// 外部 D&D 受信中のホバー状態 (#D Phase 2 受信)。
    /// `Some(ExternalDropHover { pane_id, effect_str })` のときペインを薄くハイライト。
    pub external_drop_hover: RwSignal<Option<ExternalDropHover>>,
    /// 自前 IDropTarget の登録ハンドル (#D Phase 2 受信)。
    /// `WindowGotFocus` 初回で `Some(...)` を入れる。AppState (および window) が
    /// drop されると RAII で `RevokeDragDrop` が呼ばれる。
    /// IDropTarget は COM オブジェクトで Send/Sync ではないが、UI スレッドからのみ
    /// 触る前提で `DropTargetCell` で Send 化している。
    pub drop_target_reg: Arc<Mutex<DropTargetCell>>,
    /// 右ボタン D&D 中の状態 (ADR 0010 / 0011)。
    /// `None` = 右ドラッグなし。`Some` = pane.rs PointerMove で閾値超え。
    /// Win32 サブクラスが `WM_RBUTTONUP` 時に読んで、メニュー表示後 `None` に戻す。
    pub right_drag: RwSignal<Option<RightDragState>>,
    /// Spring-loaded folder の hover 状態。
    /// D&D 中、フォルダ row や ツリーノード上で 0.7s 静止すると
    /// 自動で navigate (ペイン) / expand (ツリー) する。
    pub spring_hover: RwSignal<Option<SpringHover>>,
    /// spring_hover 用の単調増加 epoch。
    /// 別スレッドのタイマーが発火時に最新 epoch と一致するかで stale を判別する。
    pub spring_epoch: RwSignal<u64>,
}

/// 外部 D&D ホバー状態 (背景ハイライト用)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalDropHover {
    pub pane_id: u64,
    /// `"copy"` / `"move"` / `"none"` のいずれか。将来カーソル切り替え等に使う。
    pub effect: &'static str,
}

/// 右ボタン D&D 中の状態 (ADR 0010 / 0011)。
///
/// PointerMove で閾値超えに `Some` となり、Win32 サブクラスの
/// `WM_RBUTTONUP` ハンドラがこれを読んでドロップメニューを表示する。
/// メニュー表示直前に `None` へクリアされる。
#[derive(Clone, Debug)]
pub struct RightDragState {
    pub source_pane: u64,
    pub paths: Vec<PathBuf>,
    /// 現在ホバー中のドロップ先ペイン ID (`None` ならどのペイン上にもいない)
    pub hover_pane: Option<u64>,
}

/// Spring-loaded folder hover state.
///
/// D&D 中にフォルダ row や ツリーノード上で 0.7s カーソルが静止すると
/// 自動で対象を開く / 展開する。タイマーは別スレッドで sleep し
/// `floem::ext_event::create_ext_action` 経由で UI スレッドに戻ってくる。
/// stale なタイマーは `epoch` で判別。
#[derive(Clone, Debug)]
pub struct SpringHover {
    pub epoch: u64,
    pub target: PathBuf,
    pub kind: SpringKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpringKind {
    /// ペイン内 folder row hover → navigate
    Pane(u64),
    /// ツリーノード hover → expand
    Tree,
}

/// `DropTargetRegistration` を AppState 内に保持するためのラッパ。
///
/// 中身は **UI スレッドからしか触らない** ことを呼び出し側で保証する
/// (外部 D&D の登録/解除は WindowGotFocus と AppState drop のみ)。
#[derive(Default)]
pub struct DropTargetCell(pub Option<fastfiler_domain::ole_dnd::DropTargetRegistration>);

// SAFETY: AppState は floem の reactive system 上で複数スコープから clone されるため
// Send 境界を満たす必要がある。実体の COM ptr は UI スレッドからのみアクセスされる。
unsafe impl Send for DropTargetCell {}

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
        // 復元した順序で locked を再適用 (長さが合わなくても安全側 false)
        {
            let saved_locked = settings.tab_locked.get_untracked();
            for (i, tab) in tabs_vec.iter().enumerate() {
                if let Some(&l) = saved_locked.get(i) {
                    tab.locked.set(l);
                }
            }
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
            pane_splitter_drag: RwSignal::new(None),
            tree_tick: RwSignal::new(0),
            theme_rev: RwSignal::new(0),
            undo_manager: Arc::new(Mutex::new(fastfiler_domain::undo::UndoManager::new())),
            panel_dock_tree_prev: RwSignal::new(String::from("left")),
            panel_dock_tabs_prev: RwSignal::new(String::from("left")),
            jobs: crate::core::jobs::JobsState::new(),
            tree_roots: RwSignal::new(im::Vector::new()),
            tree_focused: RwSignal::new(false),
            tree_focused_path: RwSignal::new(None),
            tree_focus_req: RwSignal::new(0),
            external_drop_hover: RwSignal::new(None),
            drop_target_reg: Arc::new(Mutex::new(DropTargetCell::default())),
            right_drag: RwSignal::new(None),
            spring_hover: RwSignal::new(None),
            spring_epoch: RwSignal::new(0),
        }
    }

    /// ツリーペインへフォーカスを送るためのカウンタをインクリメントする。
    /// `tree_pane` の `request_focus` がこの signal を track している。
    pub fn request_tree_focus(&self) {
        self.tree_focus_req.update(|v| *v = v.wrapping_add(1));
    }

    pub fn active_tab(&self) -> Option<Tab> {
        let id = self.active.get();
        self.tabs.get().iter().find(|t| t.id == id).cloned()
    }

    /// 互換用: アクティブタブの primary ペイン
    pub fn active_pane(&self) -> Option<PaneState> {
        self.active_tab().and_then(|t| t.active_pane_resolved())
    }

    pub fn add_tab(&self, start: PathBuf) {
        let _g = crate::core::perf::scope(
            crate::core::perf::MetricKind::TabSwitch,
            format!("add_tab {}", start.display()),
        );
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

    /// ロック中のタブは Ctrl+W / × からの close を無視する (中クリックで解除)。
    pub fn close_tab(&self, id: u64) {
        let locked = self.tabs.with_untracked(|v| {
            v.iter()
                .find(|t| t.id == id)
                .map(|t| t.locked.get_untracked())
        });
        if locked == Some(true) {
            crate::flog!("[tab] close ignored: tab {} is locked", id);
            return;
        }
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
        let _g = crate::core::perf::scope(
            crate::core::perf::MetricKind::PaneSplit,
            if vertical { "vertical" } else { "horizontal" },
        );
        if let Some(tab) = self.active_tab() {
            if tab.pane_count() >= 4 {
                crate::flog!("[split] denied (>=4 panes)");
                return;
            }
            let active_id = tab.active_pane.get_untracked();
            let dir = if vertical {
                SplitDir::Vertical
            } else {
                SplitDir::Horizontal
            };
            let base = tab
                .all_panes()
                .into_iter()
                .find(|p| p.id == active_id)
                .map(|p| p.cur_path.get_untracked())
                .unwrap_or_else(|| tab.primary().cur_path.get_untracked());
            let (name_col_w, size_col_w, mtime_col_w) = tab
                .all_panes()
                .into_iter()
                .find(|p| p.id == active_id)
                .map(|p| {
                    (
                        p.name_col_width.get_untracked(),
                        p.size_col_width.get_untracked(),
                        p.mtime_col_width.get_untracked(),
                    )
                })
                .unwrap_or((
                    def_name_col_width(),
                    def_size_col_width(),
                    def_mtime_col_width(),
                ));
            let show_hidden = self.settings.show_hidden;
            let new_pane =
                PaneState::new_with_columns(base, show_hidden, name_col_w, size_col_w, mtime_col_w);
            let new_id = new_pane.id;

            let mut ok = false;
            tab.root.update(|r| {
                ok = r.split_leaf(active_id, dir, new_pane.clone());
            });
            crate::flog!(
                "[split] dir={:?} active={} ok={} new_id={}",
                dir,
                active_id,
                ok,
                new_id
            );
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
