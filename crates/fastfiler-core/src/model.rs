//! アプリ状態の骨格 (計画書 §5.2)。
//!
//! Phase 1: 単一ペインの一覧状態 (`PaneState`) と行モデル (`Entry`)。
//! Phase 3 で `AppModel` (タブ + BSP) がこの上に乗る。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use slotmap::new_key_type;

use crate::format;

new_key_type! {
    /// ペインの識別子。GPUI 版の `EntityId` に相当する。
    /// 所有は常に `AppModel` の SlotMap にあり、ID はただの参照。
    pub struct PaneId;
}

/// 一覧の 1 行。表示テキストとソートキーは生成時に前計算する (計画書 §9-4)。
///
/// domain の `fs::FileEntry` から GUI 層で変換する (core は domain に依存しない —
/// GPL コードを一切リンクしない層を保つため。計画書 §5.1)。
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    // 生成後は不変なので Box<str> (16B) — String (24B) だと 10 万行フォルダで
    // 約 5MB 余計に常駐する (フィールド 6 本 × 8B/行)
    pub name: Box<str>,
    pub is_dir: bool,
    pub size: u64,
    /// unix 秒 (0 以下 = 不明)。
    pub modified: i64,
    /// 拡張子 (小文字、ドットなし)。
    pub ext: Option<Box<str>>,
    pub hidden: bool,
    // ---- 前計算 (表示・ソート用) ----
    pub name_lower: Box<str>,
    pub size_text: Box<str>,
    pub modified_text: Box<str>,
    pub kind_text: Box<str>,
}

impl Entry {
    pub fn new(
        name: String,
        is_dir: bool,
        size: u64,
        modified: i64,
        ext: Option<String>,
        hidden: bool,
    ) -> Self {
        // 供給元は概ね小文字済みだが、検索ヒット等は生の拡張子を渡すため正規化は必須
        let ext = ext.map(|e| e.to_lowercase().into_boxed_str());
        let name_lower = name.to_lowercase().into_boxed_str();
        let size_text = if is_dir {
            "".into()
        } else {
            format::human_size(size).into_boxed_str()
        };
        let modified_text = format::format_modified(modified).into_boxed_str();
        let kind_text = format::kind_text(is_dir, ext.as_deref()).into_boxed_str();
        Self {
            name: name.into_boxed_str(),
            is_dir,
            size,
            modified,
            ext,
            hidden,
            name_lower,
            size_text,
            modified_text,
            kind_text,
        }
    }

    /// アイコン共有キー: フォルダ = "/" / 拡張子あり = ext 小文字 / なし = ""。
    /// ("/" はファイル名に使えない文字なので拡張子と衝突しない)。
    /// is_dir と ext から導出できるためフィールドとしては持たない
    /// (String で持つと 10 万行で約 5-7MB の無駄な常駐になる)。
    pub fn icon_key(&self) -> &str {
        if self.is_dir {
            "/"
        } else {
            self.ext.as_deref().unwrap_or("")
        }
    }
}

/// ソート対象の列 (GPUI 版 SortCol と同一)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Name,
    Modified,
    Size,
    Type,
}

/// ソート状態。列見出しクリックで同列なら昇降トグル、別列なら昇順から。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortState {
    pub col: Column,
    pub asc: bool,
}

impl Default for SortState {
    fn default() -> Self {
        Self {
            col: Column::Name,
            asc: true,
        }
    }
}

/// 並び順の比較器 (sort_entries と、選択 index を保ったままの並べ替え
/// [update.rs の HeaderClicked] で共用する)。
pub fn entry_cmp(a: &Entry, b: &Entry, sort: SortState) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let dir_ord = b.is_dir.cmp(&a.is_dir); // dir(true) を先頭へ
    if dir_ord != Ordering::Equal {
        return dir_ord;
    }
    let ord = match sort.col {
        Column::Name => a.name_lower.cmp(&b.name_lower),
        Column::Modified => a.modified.cmp(&b.modified),
        Column::Size => a.size.cmp(&b.size),
        Column::Type => a
            .ext
            .cmp(&b.ext)
            .then_with(|| a.name_lower.cmp(&b.name_lower)),
    };
    if sort.asc {
        ord
    } else {
        ord.reverse()
    }
}

/// フォルダ常に先頭 + 列キー比較 (GPUI 版 pane.rs:525 と同一規則)。
pub fn sort_entries(entries: &mut [Entry], sort: SortState) {
    entries.sort_by(|a, b| entry_cmp(a, b, sort));
}

/// ペインのオーバーレイ状態 (計画書 §10-3: 8 個の Option を 1 enum に)。
/// 常に高々 1 つ。キー入力は「Overlay があれば Overlay へ、なければ一覧へ」。
#[derive(Debug, Clone, PartialEq)]
pub enum Overlay {
    /// パスバー直接入力 (F-304)。
    PathEdit { value: String },
    /// 入力モーダル (F2 リネーム / F7 新規フォルダ / F8 新規ファイル)。
    Modal { kind: ModalKind, value: String },
    /// 同名衝突ダイアログ (F-503)。
    Conflict { plan: crate::transfer::TransferPlan },
    /// 右ボタン D&D のドロップメニュー (F-605)。
    DropMenu {
        items: Vec<crate::menu::MenuItem>,
        at: (f32, f32),
        paths: Vec<std::path::PathBuf>,
        dest: std::path::PathBuf,
    },
    /// 右クリックメニュー (F-904)。at はウィンドウ座標。
    ContextMenu {
        items: Vec<crate::menu::MenuItem>,
        at: (f32, f32),
        /// クリックで開いているサブメニューの index チェーン (最大 3 階層)。
        open_path: Vec<usize>,
        /// 行の上で開いた場合の行 index (開く/リネーム等の対象)。
        target_row: Option<usize>,
        /// テンプレートフォルダのパス (OpenTemplatesDir 用)。
        templates_dir: String,
    },
}

/// 入力モーダルの種類 (GPUI 版 ModalKind と同じ 3 種)。
#[derive(Debug, Clone, PartialEq)]
pub enum ModalKind {
    Rename { original: String },
    NewFolder,
    NewFile,
}

/// 検索バーの状態 (F-701/F-702)。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SearchUi {
    pub query: String,
    /// 実行中の検索 job id (結果の突き合わせ)。
    pub job_id: Option<u64>,
    pub running: bool,
    /// Enter 済み = 結果リスト表示中 (一覧と切替)。
    pub showing: bool,
    /// 結果 (Entry 化して FileList で表示)。フルパスは search_paths と同順。
    pub hits: Vec<Entry>,
    pub hit_paths: Vec<(std::path::PathBuf, bool)>,
    pub summary: Option<String>,
}

/// 実行中のファイルジョブ (フッタの進捗表示 + キャンセル対象)。
#[derive(Debug, Clone, PartialEq)]
pub struct JobStatus {
    pub id: u64,
    pub kind: String,
    pub done_files: u64,
    pub total_files: u64,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub current: String,
}

/// 1 ペインの一覧状態 (Phase 1〜2 スコープ)。
#[derive(Debug)]
pub struct PaneState {
    pub cur_path: PathBuf,
    pub entries: Vec<Entry>,
    /// 読み込み中 (連続ナビゲーションのキャンセルは世代番号で行う)。
    pub loading: bool,
    /// 読み込み世代。navigate のたびに +1 し、古い世代の結果は捨てる。
    pub load_gen: u64,
    pub load_error: Option<String>,
    pub sort: SortState,
    // ---- ナビゲーション履歴 (F-303。ペイン単位、GPUI 版と同じ) ----
    pub history_back: Vec<PathBuf>,
    pub history_fwd: Vec<PathBuf>,
    // ---- オーバーレイ (モーダル系。§10-3) ----
    pub overlay: Option<Overlay>,
    /// watcher デバウンスの通し番号 (150ms 以内の連続変化を 1 回の reload に潰す)。
    pub reload_seq: u64,
    /// 実行中ジョブ (高々 1 つ — GPUI 版と同じ)。
    pub job: Option<JobStatus>,
    /// 検索バー (F-701)。Some = バー表示中。一覧は表示されたまま。
    pub search: Option<SearchUi>,
    /// フッタの一時メッセージ (エラー / 完了通知)。
    pub status_msg: Option<String>,
    // ---- 選択モデル (selection.rs に操作を実装) ----
    pub cursor: Option<usize>,
    pub selected: BTreeSet<usize>,
    pub anchor: Option<usize>,
    /// 次回 Loaded 時にこの名前へカーソルを合わせる (親へ戻ったとき等)。
    pub pending_cursor_name: Option<String>,
    /// 修飾なしで選択済み行を押下したとき、単一選択への確定を Release まで
    /// 保留している行 (エクスプローラ準拠 — 複数選択のままドラッグを始めるため)。
    pub pending_click: Option<usize>,
    /// 現在 `entries` に表示している一覧のパス (Loaded 時に確定)。
    /// 移動先の読み込みに失敗したとき、ここへ cur_path を戻す。
    pub loaded_path: Option<PathBuf>,
    // ---- ビュー幾何 (スクロールは core が所有し、単体テスト可能にする) ----
    /// 列幅 [更新日時, サイズ, 種類] px (F-402。名前列が残りを吸収)。
    pub col_widths: [f32; 3],
    pub scroll_offset: f32,
    /// 一覧ビューポートの高さ px (FileList から通知される)。
    pub viewport_h: f32,
    /// 行高 px (Phase 6 でフォントサイズ追従)。
    pub row_h: f32,
}

/// GPUI 版の既定列幅 (セッション未保存時)。
pub const DEFAULT_COL_WIDTHS: [f32; 3] = [140.0, 90.0, 90.0];
pub const DEFAULT_ROW_H: f32 = 24.0;
/// 列幅ドラッグの範囲 (USAGE.md §2: 40〜400px)。
pub const COL_W_MIN: f32 = 40.0;
pub const COL_W_MAX: f32 = 400.0;

/// パス区切りの正規化 (連続 `\` を 1 つに / `/` を `\` に / 末尾区切り・`.` を除去)。
///
/// ドライブ列挙 (`"D:\"`) 由来の組み立てミスやユーザー入力の表記ゆれで
/// `D:\\AI` のようなパスが cur_path に入ると、Path 同士の比較 (components
/// ベース) は通るのに文字列比較 (watcher の一致判定・表示) だけが狂う。
/// cur_path に入るパスは必ずここを通す (PaneState::new / set_path_and_load)。
pub fn normalize_path(path: &Path) -> PathBuf {
    path.components().collect()
}

impl PaneState {
    pub fn new(cur_path: PathBuf) -> Self {
        Self {
            cur_path: normalize_path(&cur_path),
            entries: Vec::new(),
            loading: false,
            load_gen: 0,
            load_error: None,
            sort: SortState::default(),
            history_back: Vec::new(),
            history_fwd: Vec::new(),
            overlay: None,
            reload_seq: 0,
            job: None,
            search: None,
            status_msg: None,
            cursor: None,
            selected: BTreeSet::new(),
            anchor: None,
            pending_cursor_name: None,
            pending_click: None,
            loaded_path: None,
            col_widths: DEFAULT_COL_WIDTHS,
            scroll_offset: 0.0,
            viewport_h: 0.0,
            row_h: DEFAULT_ROW_H,
        }
    }

    /// 表示中リストの行数 (検索結果リスト表示中は hits — F-701)。
    /// 選択・キーナビ・スクロールはこの長さを基準にする。
    pub fn visible_len(&self) -> usize {
        match &self.search {
            Some(s) if s.showing => s.hits.len(),
            _ => self.entries.len(),
        }
    }

    /// 検索結果リストを表示中か。
    pub fn showing_search(&self) -> bool {
        self.search.as_ref().is_some_and(|s| s.showing)
    }

    /// スクロール可能量の上限。
    pub fn max_scroll(&self) -> f32 {
        (self.visible_len() as f32 * self.row_h - self.viewport_h).max(0.0)
    }

    /// カーソル行が可視範囲に入るよう scroll_offset を調整する。
    pub fn ensure_cursor_visible(&mut self) {
        let Some(ix) = self.cursor else { return };
        let top = ix as f32 * self.row_h;
        let bottom = top + self.row_h;
        if top < self.scroll_offset {
            self.scroll_offset = top;
        } else if self.viewport_h > 0.0 && bottom > self.scroll_offset + self.viewport_h {
            self.scroll_offset = bottom - self.viewport_h;
        }
        self.scroll_offset = self.scroll_offset.clamp(0.0, self.max_scroll());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn e(name: &str, is_dir: bool) -> Entry {
        let ext = (!is_dir)
            .then(|| name.rsplit_once('.').map(|(_, e)| e.to_string()))
            .flatten();
        Entry::new(name.to_string(), is_dir, 10, 1_700_000_000, ext, false)
    }

    #[test]
    fn normalize_path_fixes_separator_variants() {
        // 実機報告: ドライブ列挙由来の "D:\\AI" (二重区切り) が cur_path に入り、
        // パスバー表示と文字列比較が狂う。文字列レベルで正すことを確認する
        let s = |p: &str| normalize_path(Path::new(p)).to_string_lossy().to_string();
        assert_eq!(s("D:\\\\AI\\comfy\\output"), "D:\\AI\\comfy\\output");
        assert_eq!(s("D:/AI/comfy"), "D:\\AI\\comfy"); // スラッシュ入力
        assert_eq!(s("D:\\AI\\"), "D:\\AI"); // 末尾区切り
        assert_eq!(s("C:\\"), "C:\\"); // ドライブルートは不変
        assert_eq!(s("\\\\nas\\share\\a"), "\\\\nas\\share\\a"); // UNC は不変

        // PaneState::new も同じ正規化を通す
        let p = PaneState::new(PathBuf::from("D:\\\\AI"));
        assert_eq!(p.cur_path.to_string_lossy(), "D:\\AI");
    }

    #[test]
    fn sort_dirs_first_then_name_case_insensitive() {
        let mut v = vec![e("b.txt", false), e("Alpha", true), e("A.txt", false)];
        sort_entries(&mut v, SortState::default());
        let names: Vec<_> = v.iter().map(|x| x.name.as_ref()).collect();
        assert_eq!(names, ["Alpha", "A.txt", "b.txt"]);
    }

    #[test]
    fn sort_desc_keeps_dirs_first() {
        let mut v = vec![e("a.txt", false), e("dir", true), e("z.txt", false)];
        sort_entries(
            &mut v,
            SortState {
                col: Column::Name,
                asc: false,
            },
        );
        let names: Vec<_> = v.iter().map(|x| x.name.as_ref()).collect();
        assert_eq!(names, ["dir", "z.txt", "a.txt"]);
    }

    #[test]
    fn sort_by_type_uses_ext_then_name() {
        let mut v = vec![e("b.zip", false), e("a.txt", false), e("c.txt", false)];
        sort_entries(
            &mut v,
            SortState {
                col: Column::Type,
                asc: true,
            },
        );
        let names: Vec<_> = v.iter().map(|x| x.name.as_ref()).collect();
        assert_eq!(names, ["a.txt", "c.txt", "b.zip"]);
    }

    #[test]
    fn ensure_cursor_visible_scrolls_both_directions() {
        let mut p = PaneState::new(PathBuf::from("C:\\"));
        p.entries = (0..100).map(|i| e(&format!("f{i}.txt"), false)).collect();
        p.viewport_h = 240.0; // 10 行
        p.row_h = 24.0;
        // 下方向
        p.cursor = Some(50);
        p.ensure_cursor_visible();
        assert_eq!(p.scroll_offset, 50.0 * 24.0 + 24.0 - 240.0);
        // 上方向
        p.cursor = Some(3);
        p.ensure_cursor_visible();
        assert_eq!(p.scroll_offset, 72.0);
        // 可視内なら動かない
        let before = p.scroll_offset;
        p.cursor = Some(5);
        p.ensure_cursor_visible();
        assert_eq!(p.scroll_offset, before);
    }

    #[test]
    fn entry_precomputes_display_fields() {
        let x = Entry::new(
            "Read Me.TXT".into(),
            false,
            2048,
            0,
            Some("TXT".into()),
            false,
        );
        assert_eq!(&*x.name_lower, "read me.txt");
        assert_eq!(&*x.size_text, "2.0 KB");
        assert_eq!(&*x.modified_text, "");
        assert_eq!(&*x.kind_text, "TXT");
        assert_eq!(x.icon_key(), "txt");
        let d = Entry::new("dir".into(), true, 0, 0, None, false);
        assert_eq!(&*d.size_text, "");
        assert_eq!(&*d.kind_text, "フォルダ");
        assert_eq!(d.icon_key(), "/");
    }
}
