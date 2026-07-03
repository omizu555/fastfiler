//! 副作用の記述 (計画書 §5.3)。`update` は I/O をせず Effect を返し、
//! GUI 層 (fastfiler-iced/effects.rs) が実行する。

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// フォルダ一覧の読み込み (domain fs::list_dir + アイコン取得)。
    /// gen が古くなった結果は捨てる (連続ナビゲーションのキャンセル)。
    LoadDir { generation: u64, path: PathBuf },
    /// ファイルを既定アプリで開く (domain shell)。
    OpenFile { path: PathBuf },
}
