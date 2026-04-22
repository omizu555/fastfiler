// Windows クリップボードへファイルパス (CF_HDROP) を書き込み、エクスプローラ等
// で「貼り付け」「切り取り→貼り付け」を実現する。
//
// 仕組み:
//   * CF_HDROP (DROPFILES + 連結ワイド文字列 + ダブル NUL 終端) を書き込む
//   * "Preferred DropEffect" レジストリ形式 (DWORD) を併せて書き込む
//       1 = COPY, 2 = MOVE
//     これによりエクスプローラは「コピー」「切り取り」を判別できる
//
// 「切り取り」表示 (淡色化) は CFSTR_PREFERREDDROPEFFECT=DROPEFFECT_MOVE が
// 書き込まれていれば、エクスプローラ側が貼り付け成功時に元ファイルを削除する。

use crate::error::{AppError, AppResult};

#[tauri::command]
pub fn clipboard_write_paths(paths: Vec<String>, op: String) -> AppResult<()> {
    if paths.is_empty() {
        return Err(AppError::Other("paths が空です".into()));
    }
    #[cfg(not(windows))]
    {
        let _ = (paths, op);
        return Err(AppError::Other("Windows でのみ利用可能".into()));
    }
    #[cfg(windows)]
    unsafe {
        write_paths_win(&paths, &op)
    }
}

#[cfg(windows)]
unsafe fn write_paths_win(paths: &[String], op: &str) -> AppResult<()> {
    use windows::Win32::Foundation::{HANDLE, HWND};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GHND};
    use windows::Win32::System::Ole::CF_HDROP;
    use windows::Win32::UI::Shell::DROPFILES;
    use windows::core::PCWSTR;

    // 1. CF_HDROP 用バイト列を準備
    //    DROPFILES + (path1\0 path2\0 ... \0)
    let mut wide_paths: Vec<u16> = Vec::new();
    for p in paths {
        // バックスラッシュへ正規化
        let normalized: String = p.chars().map(|c| if c == '/' { '\\' } else { c }).collect();
        for u in normalized.encode_utf16() {
            wide_paths.push(u);
        }
        wide_paths.push(0);
    }
    wide_paths.push(0); // ダブル NUL 終端

    let dropfiles_size = std::mem::size_of::<DROPFILES>();
    let payload_bytes = wide_paths.len() * 2;
    let total = dropfiles_size + payload_bytes;

    // 2. グローバルメモリ確保 (CF_HDROP 本体)
    let h_drop = GlobalAlloc(GHND, total)
        .map_err(|e| AppError::Other(format!("GlobalAlloc(HDROP): {e}")))?;
    if h_drop.is_invalid() {
        return Err(AppError::Other("GlobalAlloc(HDROP) returned invalid".into()));
    }
    {
        let p = GlobalLock(h_drop) as *mut u8;
        if p.is_null() {
            // 失敗時の解放は省略 (極めて稀。SetClipboardData 成功までの一時的な漏れは許容)
            return Err(AppError::Other("GlobalLock(HDROP) failed".into()));
        }
        // DROPFILES を書き込む
        let df = p as *mut DROPFILES;
        (*df).pFiles = dropfiles_size as u32;
        (*df).pt = std::mem::zeroed();
        (*df).fNC = false.into();
        (*df).fWide = true.into();
        // ワイド文字配列をコピー
        let dst = p.add(dropfiles_size) as *mut u16;
        std::ptr::copy_nonoverlapping(wide_paths.as_ptr(), dst, wide_paths.len());
        let _ = GlobalUnlock(h_drop);
    }

    // 3. Preferred DropEffect 用 DWORD を別途確保
    let h_eff = GlobalAlloc(GHND, std::mem::size_of::<u32>())
        .map_err(|e| AppError::Other(format!("GlobalAlloc(Effect): {e}")))?;
    if h_eff.is_invalid() {
        return Err(AppError::Other("GlobalAlloc(Effect) returned invalid".into()));
    }
    {
        let p = GlobalLock(h_eff) as *mut u32;
        if p.is_null() {
            return Err(AppError::Other("GlobalLock(Effect) failed".into()));
        }
        // 1=COPY, 2=MOVE
        *p = if op == "cut" || op == "move" { 2 } else { 1 };
        let _ = GlobalUnlock(h_eff);
    }

    // 4. クリップボードへセット
    if OpenClipboard(HWND(std::ptr::null_mut())).is_err() {
        return Err(AppError::Other("OpenClipboard 失敗".into()));
    }

    let res = (|| -> AppResult<()> {
        EmptyClipboard().map_err(|e| AppError::Other(format!("EmptyClipboard: {e}")))?;

        // CF_HDROP
        SetClipboardData(CF_HDROP.0 as u32, HANDLE(h_drop.0))
            .map_err(|e| AppError::Other(format!("SetClipboardData(HDROP): {e}")))?;

        // CFSTR_PREFERREDDROPEFFECT
        let fmt_name: Vec<u16> = "Preferred DropEffect\0".encode_utf16().collect();
        let cf_pref = RegisterClipboardFormatW(PCWSTR(fmt_name.as_ptr()));
        if cf_pref == 0 {
            return Err(AppError::Other("RegisterClipboardFormatW 失敗".into()));
        }
        SetClipboardData(cf_pref, HANDLE(h_eff.0))
            .map_err(|e| AppError::Other(format!("SetClipboardData(Pref): {e}")))?;
        Ok(())
    })();

    let _ = CloseClipboard();
    res
}
