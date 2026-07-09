//! インストール済みフォントファミリーの列挙 (設定画面のコンボボックス用)。
//!
//! GDI `EnumFontFamiliesExW` でファミリー名を集める (レジストリ列挙と違い
//! スタイルサフィックス — "Bold" 等 — が混ざらない)。
//! `@` 始まり (縦書き用) は除外。重複除去してソート済みで返す。

use std::collections::BTreeSet;

use windows::Win32::Foundation::LPARAM;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, EnumFontFamiliesExW, DEFAULT_CHARSET, ENUMLOGFONTEXW, LOGFONTW,
    TEXTMETRICW,
};

unsafe extern "system" fn enum_proc(
    lf: *const LOGFONTW,
    _tm: *const TEXTMETRICW,
    _font_type: u32,
    lparam: LPARAM,
) -> i32 {
    let set = &mut *(lparam.0 as *mut BTreeSet<String>);
    let elf = &*(lf as *const ENUMLOGFONTEXW);
    let name_u16 = &elf.elfLogFont.lfFaceName;
    let name = fastfiler_domain::wstr::from_wide_z(name_u16);
    if !name.is_empty() && !name.starts_with('@') {
        set.insert(name);
    }
    1 // 続行
}

/// インストール済みフォントファミリー名の一覧 (ソート済み・重複なし)。
pub fn installed_font_families() -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    unsafe {
        let hdc = CreateCompatibleDC(None);
        if hdc.is_invalid() {
            return Vec::new();
        }
        let lf = LOGFONTW {
            lfCharSet: DEFAULT_CHARSET,
            ..Default::default()
        };
        EnumFontFamiliesExW(
            hdc,
            &lf,
            Some(enum_proc),
            LPARAM(&mut set as *mut _ as isize),
            0,
        );
        let _ = DeleteDC(hdc);
    }
    set.into_iter().collect()
}
