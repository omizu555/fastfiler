//! UTF-16 (ワイド文字列) 変換の共通ヘルパ。
//!
//! 同型のローカル定義が fs/icons/shell/single_instance 等に 6 つ + インライン
//! 約 10 箇所に乱立していたのを一元化した。NUL 終端の付け忘れ / 二重付けという
//! 事故クラスをこのモジュールに閉じ込める。

use std::ffi::OsStr;

/// NUL 終端付き UTF-16 へ変換する。
/// `&str` / `&Path` / `&OsStr` を無損失で受ける (`&str` 入力の出力は
/// `encode_utf16()` と同一 — 既存 2 流儀の置換で挙動は変わらない)。
pub fn to_wide_z(s: impl AsRef<OsStr>) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

/// NUL 終端バッファ → String (最初の NUL まで。NUL がなければ全体)。
pub fn from_wide_z(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// double NUL 終端のワイド・パス列 (CF_HDROP ペイロード / SHFileOperation 形式)。
/// '/' は '\\' へ正規化する (エクスプローラとの相互運用)。
pub fn wide_path_list_double_nul<I, S>(paths: I) -> Vec<u16>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut wide: Vec<u16> = Vec::new();
    for p in paths {
        let normalized: String = p
            .as_ref()
            .chars()
            .map(|c| if c == '/' { '\\' } else { c })
            .collect();
        wide.extend(normalized.encode_utf16());
        wide.push(0);
    }
    wide.push(0); // ダブル NUL 終端
    wide
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_wide_z_appends_single_nul() {
        assert_eq!(to_wide_z("ab"), vec![97, 98, 0]);
        // &Path 入力も同一 (非 UTF-8 も無損失だが、有効な UTF-8 では同じ)
        assert_eq!(to_wide_z(std::path::Path::new("ab")), vec![97, 98, 0]);
        assert_eq!(from_wide_z(&[97, 98, 0, 99]), "ab");
        assert_eq!(from_wide_z(&[97, 98]), "ab");
    }

    #[test]
    fn path_list_normalizes_and_double_terminates() {
        let w = wide_path_list_double_nul(["C:/a", "D:\\b"]);
        let expect: Vec<u16> = "C:\\a\0D:\\b\0\0".encode_utf16().collect();
        assert_eq!(w, expect);
        // 空リストでも二重終端 (= NUL 1 つ + 終端 NUL ではなく仕様どおり末尾のみ)
        assert_eq!(wide_path_list_double_nul(Vec::<&str>::new()), vec![0]);
    }
}
