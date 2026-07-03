//! 表示テキストの整形 (GPUI 版 pane.rs とパリティ)。
//!
//! - サイズ: `human_size` — 1024 進 B/KB/MB/GB/TB。B は整数、それ以外は小数 1 桁
//!   (GPUI 版 pane.rs:3867 と同一規則)。
//! - 更新日時: `YYYY/MM/DD HH:MM` ローカル時刻 (USAGE.md §2)。0 以下は空欄。
//! - 種類: フォルダ → "フォルダ" / 拡張子大文字 / 拡張子なし → "ファイル"
//!   (GPUI 版 pane.rs:3128 と同一規則)。

use chrono::{Local, TimeZone};

/// バイト数の人間可読表記。
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut b = bytes as f64;
    let mut i = 0;
    while b >= 1024.0 && i < UNITS.len() - 1 {
        b /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", bytes, UNITS[i])
    } else {
        format!("{:.1} {}", b, UNITS[i])
    }
}

/// unix 秒 → ローカル時刻 `YYYY/MM/DD HH:MM`。0 以下・変換不能は空欄。
pub fn format_modified(unix: i64) -> String {
    if unix <= 0 {
        return String::new();
    }
    match Local.timestamp_opt(unix, 0) {
        chrono::LocalResult::Single(t) | chrono::LocalResult::Ambiguous(t, _) => {
            t.format("%Y/%m/%d %H:%M").to_string()
        }
        chrono::LocalResult::None => String::new(),
    }
}

/// 種類列の表示。
pub fn kind_text(is_dir: bool, ext: Option<&str>) -> String {
    if is_dir {
        "フォルダ".to_string()
    } else {
        ext.map(|e| e.to_uppercase())
            .unwrap_or_else(|| "ファイル".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_matches_gpui_rules() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(1024 * 1024), "1.0 MB");
        assert_eq!(human_size(u64::MAX).ends_with("TB"), true);
    }

    #[test]
    fn modified_empty_for_epoch_and_negative() {
        assert_eq!(format_modified(0), "");
        assert_eq!(format_modified(-5), "");
        let s = format_modified(1_700_000_000);
        // ローカル TZ 依存のため形だけ検証 (YYYY/MM/DD HH:MM)
        assert_eq!(s.len(), 16);
        assert_eq!(&s[4..5], "/");
        assert_eq!(&s[13..14], ":");
    }

    #[test]
    fn kind_text_rules() {
        assert_eq!(kind_text(true, Some("txt")), "フォルダ");
        assert_eq!(kind_text(false, Some("txt")), "TXT");
        assert_eq!(kind_text(false, None), "ファイル");
    }
}
