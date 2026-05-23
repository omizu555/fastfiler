//! Windows パスに対する補助関数。
//!
//! `volume_key` は D&D のデフォルト動作 (同ボリューム=Move / 別=Copy) や
//! 跨ぎ判定に利用する。junction / subst 等の論理的別物理ボリュームは
//! 表面パスでは判別できないため既知制限とする。

use std::path::Path;

/// パス先頭からボリュームを示すキーを抽出する。
///
/// 戻り値:
///  - `C:\foo`         → `Some("c:")`
///  - `C:`             → `Some("c:")`
///  - `\\server\share\foo` → `Some(r"\\server\share")` (小文字化)
///  - `\\?\C:\foo`     → `Some("c:")`
///  - `\\?\UNC\server\share\foo` → `Some(r"\\server\share")`
///  - それ以外 (相対パス等) → `None` (呼び出し側で Copy fallback)
pub fn volume_key(path: &Path) -> Option<String> {
    let s = path.to_string_lossy();
    let s = s.as_ref();
    let lower = s.to_ascii_lowercase();
    let b = lower.as_bytes();

    // \\?\UNC\server\share\...  または  \\?\C:\...
    if let Some(rest) = lower.strip_prefix(r"\\?\") {
        if let Some(unc_rest) = rest.strip_prefix(r"unc\") {
            return unc_two_segments(unc_rest);
        }
        // \\?\C:\... → c:
        let rb = rest.as_bytes();
        if rb.len() >= 2 && rb[0].is_ascii_alphabetic() && rb[1] == b':' {
            return Some(format!("{}:", rb[0] as char));
        }
        return None;
    }

    // \\server\share\...
    if let Some(rest) = lower.strip_prefix(r"\\") {
        return unc_two_segments(rest);
    }

    // C:\... or C:
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        return Some(format!("{}:", b[0] as char));
    }

    None
}

fn unc_two_segments(rest: &str) -> Option<String> {
    // server\share[\...] から "\\server\share" を作る
    let mut it = rest.splitn(3, '\\');
    let server = it.next()?;
    let share = it.next()?;
    if server.is_empty() || share.is_empty() {
        return None;
    }
    Some(format!(r"\\{}\{}", server, share))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn k(s: &str) -> Option<String> {
        volume_key(&PathBuf::from(s))
    }

    #[test]
    fn drive_letter() {
        assert_eq!(k(r"C:\foo\bar"), Some("c:".into()));
        assert_eq!(k(r"c:\foo"), Some("c:".into()));
        assert_eq!(k(r"D:"), Some("d:".into()));
    }

    #[test]
    fn unc() {
        assert_eq!(
            k(r"\\Server\Share\dir\file"),
            Some(r"\\server\share".into())
        );
        assert_eq!(k(r"\\server\share"), Some(r"\\server\share".into()));
    }

    #[test]
    fn extended_drive() {
        assert_eq!(k(r"\\?\C:\foo"), Some("c:".into()));
    }

    #[test]
    fn extended_unc() {
        assert_eq!(
            k(r"\\?\UNC\server\share\foo"),
            Some(r"\\server\share".into())
        );
    }

    #[test]
    fn relative_returns_none() {
        assert_eq!(k(r"foo\bar"), None);
        assert_eq!(k(r""), None);
    }

    #[test]
    fn malformed_unc_returns_none() {
        assert_eq!(k(r"\\server"), None);
        assert_eq!(k(r"\\"), None);
    }
}
