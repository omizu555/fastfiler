//! パス関連の小物ユーティリティ。
//!
//! 注: かつてここにあった `volume_key` (同一ボリューム判定) はプロダクション
//! 呼び出しゼロの死コードだった (正は core の transfer::same_volume — 拡張長
//! UNC の解釈も異なっていた) ため削除した。

/// `%APPDATA%\FastFiler` (テンプレート・ユーザーコマンドの置き場)。
/// 旧実装は "fastfiler" 小文字表記が混在していた (NTFS は大小文字非区別のため
/// 既存ユーザーのフォルダはそのまま使われる)。
pub fn appdata_dir() -> Option<std::path::PathBuf> {
    let base = std::env::var("APPDATA").ok()?;
    Some(std::path::PathBuf::from(base).join("FastFiler"))
}
