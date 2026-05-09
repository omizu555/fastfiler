//! インストール済みフォント一覧の取得。
//! Windows ではレジストリ (HKLM/HKCU の Fonts) から family 名を抽出する。
//! 他 OS では空のリストを返す (将来 fontconfig 等で拡張可)。

use std::sync::OnceLock;

static FONT_CACHE: OnceLock<Vec<String>> = OnceLock::new();

/// インストール済みフォントの一覧を取得する (アルファベット順、重複排除)。
/// 結果は初回呼び出し時にキャッシュされ、以後同じものを返す。
pub fn installed_fonts() -> &'static [String] {
    FONT_CACHE.get_or_init(load_fonts).as_slice()
}

fn load_fonts() -> Vec<String> {
    #[cfg(windows)]
    {
        windows_fonts()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

#[cfg(windows)]
fn windows_fonts() -> Vec<String> {
    use std::collections::BTreeSet;
    use winreg::enums::*;
    use winreg::RegKey;

    let mut set: BTreeSet<String> = BTreeSet::new();
    // (空) は「システムデフォルトに任せる」を意味する
    set.insert(String::new());

    for (root, hive) in [
        (RegKey::predef(HKEY_LOCAL_MACHINE), HKEY_LOCAL_MACHINE),
        (RegKey::predef(HKEY_CURRENT_USER), HKEY_CURRENT_USER),
    ] {
        let _ = hive; // unused warning suppression
        if let Ok(key) = root.open_subkey_with_flags(
            "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Fonts",
            KEY_READ,
        ) {
            for v in key.enum_values().flatten() {
                let name = v.0;
                if let Some(family) = strip_font_suffix(&name) {
                    set.insert(family);
                }
            }
        }
    }

    set.into_iter().collect()
}

/// レジストリ値名 (例: `"Arial (TrueType)"`, `"游ゴシック Light & 游ゴシック Light Italic (TrueType)"`)
/// から末尾の ` (TrueType)` / ` (OpenType)` 等を取り除き、`&` で結合された複数の family は
/// 個別に扱えるよう先頭1つだけを返す簡易版。
#[cfg(windows)]
fn strip_font_suffix(raw: &str) -> Option<String> {
    let s = raw.trim();
    let end = s.rfind(" (")?;
    let body = s[..end].trim();
    if body.is_empty() {
        return None;
    }
    // "Family A & Family B" → 最初の Family A だけを採用
    let first = body.split('&').next().unwrap_or(body).trim();
    if first.is_empty() {
        None
    } else {
        Some(first.to_string())
    }
}
