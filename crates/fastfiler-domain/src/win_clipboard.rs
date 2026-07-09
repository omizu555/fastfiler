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

pub fn clipboard_write_paths(paths: Vec<String>, op: String) -> AppResult<()> {
    if paths.is_empty() {
        return Err(AppError::Win32("paths が空です".into()));
    }
    #[cfg(not(windows))]
    {
        let _ = (paths, op);
        return Err(AppError::Win32("Windows でのみ利用可能".into()));
    }
    #[cfg(windows)]
    unsafe {
        write_paths_win(&paths, &op)
    }
}

#[cfg(windows)]
unsafe fn write_paths_win(paths: &[String], op: &str) -> AppResult<()> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Ole::CF_HDROP;

    // CF_HDROP バイト列 (DROPFILES + ダブル NUL ワイド列) — 構築は hdrop.rs に
    // 集約 (ole_dnd の D&D 送信と同一仕様)。ガードは SetClipboardData 成功まで
    // の HGLOBAL リークを防ぐ (従来はエラーパスで全量漏れていた)
    let bytes = crate::hdrop::build_hdrop_bytes(paths);
    let mut h_drop = crate::hdrop::HGlobalGuard::from_bytes(&bytes)?;
    // Preferred DropEffect: 1=COPY, 2=MOVE
    let effect: u32 = if op == "cut" || op == "move" { 2 } else { 1 };
    let mut h_eff = crate::hdrop::HGlobalGuard::from_dword(effect)?;

    if OpenClipboard(HWND(std::ptr::null_mut())).is_err() {
        return Err(AppError::Win32("OpenClipboard 失敗".into()));
    }
    let res = (|| -> AppResult<()> {
        EmptyClipboard().map_err(|e| AppError::Win32(format!("EmptyClipboard: {e}")))?;

        SetClipboardData(CF_HDROP.0 as u32, h_drop.handle())
            .map_err(|e| AppError::Win32(format!("SetClipboardData(HDROP): {e}")))?;
        h_drop.disarm(); // 所有権はクリップボードへ移った

        let cf_pref = crate::ole_dnd::cf_preferred_drop_effect() as u32;
        if cf_pref == 0 {
            return Err(AppError::Win32("RegisterClipboardFormat 失敗".into()));
        }
        SetClipboardData(cf_pref, h_eff.handle())
            .map_err(|e| AppError::Win32(format!("SetClipboardData(Pref): {e}")))?;
        h_eff.disarm();
        Ok(())
    })();
    let _ = CloseClipboard();
    res
}

// =================================================================
// クリップボードから CF_HDROP + Preferred DropEffect を読み出す
// =================================================================

#[derive(serde::Serialize)]
pub struct ClipboardPaths {
    pub paths: Vec<String>,
    /// "copy" | "cut"
    pub op: String,
}

pub fn clipboard_read_paths() -> AppResult<Option<ClipboardPaths>> {
    #[cfg(not(windows))]
    {
        return Ok(None);
    }
    #[cfg(windows)]
    unsafe {
        read_paths_win()
    }
}

#[cfg(windows)]
unsafe fn read_paths_win() -> AppResult<Option<ClipboardPaths>> {
    use windows::Win32::Foundation::{HGLOBAL, HWND};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows::Win32::System::Ole::CF_HDROP;
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

    if IsClipboardFormatAvailable(CF_HDROP.0 as u32).is_err() {
        return Ok(None);
    }
    if OpenClipboard(HWND(std::ptr::null_mut())).is_err() {
        return Err(AppError::Win32("OpenClipboard 失敗".into()));
    }

    let result: AppResult<Option<ClipboardPaths>> = (|| {
        let h = GetClipboardData(CF_HDROP.0 as u32)
            .map_err(|e| AppError::Win32(format!("GetClipboardData(HDROP): {e}")))?;
        if h.is_invalid() {
            return Ok(None);
        }
        let hdrop = HDROP(h.0);
        let count = DragQueryFileW(hdrop, 0xFFFFFFFF, None);
        let mut paths: Vec<String> = Vec::with_capacity(count as usize);
        for i in 0..count {
            let needed = DragQueryFileW(hdrop, i, None);
            if needed == 0 {
                continue;
            }
            let mut buf: Vec<u16> = vec![0u16; (needed + 1) as usize];
            let written = DragQueryFileW(hdrop, i, Some(&mut buf));
            if written == 0 {
                continue;
            }
            let s = String::from_utf16_lossy(&buf[..written as usize]);
            paths.push(s);
        }

        // Preferred DropEffect を読む (1=COPY, 2=MOVE)。形式 ID は ole_dnd と共用
        let cf_pref = crate::ole_dnd::cf_preferred_drop_effect() as u32;
        let mut op = "copy".to_string();
        if cf_pref != 0 && IsClipboardFormatAvailable(cf_pref).is_ok() {
            if let Ok(h_eff) = GetClipboardData(cf_pref) {
                if !h_eff.is_invalid() {
                    let hg = HGLOBAL(h_eff.0);
                    let p = GlobalLock(hg) as *const u32;
                    if !p.is_null() {
                        if *p == 2 {
                            op = "cut".to_string();
                        }
                        let _ = GlobalUnlock(hg);
                    }
                }
            }
        }
        Ok(Some(ClipboardPaths { paths, op }))
    })();

    let _ = CloseClipboard();
    result
}

// =================================================================
// クリップボードへプレーンテキスト (CF_UNICODETEXT) を書き込む
// =================================================================

/// 任意の文字列を Unicode テキストとしてクリップボードへ書き込む。
///
/// 改行は CRLF に正規化する (メモ帳・Excel など Windows 系アプリで自然に
/// 貼り付けられるようにするため)。
pub fn clipboard_write_text(text: &str) -> AppResult<()> {
    #[cfg(not(windows))]
    {
        let _ = text;
        return Err(AppError::Win32("Windows でのみ利用可能".into()));
    }
    #[cfg(windows)]
    unsafe {
        write_text_win(text)
    }
}

#[cfg(windows)]
unsafe fn write_text_win(text: &str) -> AppResult<()> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Ole::CF_UNICODETEXT;

    // 改行を CRLF に正規化
    let normalized = text.replace("\r\n", "\n").replace('\n', "\r\n");
    let wide = crate::wstr::to_wide_z(&normalized);

    // u16 列 → バイト列 (リトルエンディアン)
    let bytes: Vec<u8> = wide.iter().flat_map(|u| u.to_le_bytes()).collect();
    let mut h = crate::hdrop::HGlobalGuard::from_bytes(&bytes)?;

    if OpenClipboard(HWND(std::ptr::null_mut())).is_err() {
        return Err(AppError::Win32("OpenClipboard 失敗".into()));
    }
    let res = (|| -> AppResult<()> {
        EmptyClipboard().map_err(|e| AppError::Win32(format!("EmptyClipboard: {e}")))?;
        SetClipboardData(CF_UNICODETEXT.0 as u32, h.handle())
            .map_err(|e| AppError::Win32(format!("SetClipboardData(TEXT): {e}")))?;
        h.disarm();
        Ok(())
    })();
    let _ = CloseClipboard();
    res
}
