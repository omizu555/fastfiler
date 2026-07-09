//! CF_HDROP (DROPFILES) バイト列の構築と HGLOBAL 確保の共通実装。
//!
//! ole_dnd (D&D 送信) と win_clipboard (コピー/切り取り) で同一仕様の
//! CF_HDROP を作るため、レイアウトの不変条件 (pFiles オフセット / fWide /
//! ダブル NUL 終端) の保守箇所をここ 1 つにする。

use crate::error::{AppError, AppResult};
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GHND};
use windows::Win32::UI::Shell::DROPFILES;

/// CF_HDROP 用バイト列 (DROPFILES ヘッダ + ワイドパス列 + ダブル NUL)。
pub fn build_hdrop_bytes<I, S>(paths: I) -> Vec<u8>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let wide = crate::wstr::wide_path_list_double_nul(paths);
    let dropfiles_size = std::mem::size_of::<DROPFILES>();
    let total = dropfiles_size + wide.len() * 2;
    let mut buf = vec![0u8; total];
    unsafe {
        let df = buf.as_mut_ptr() as *mut DROPFILES;
        (*df).pFiles = dropfiles_size as u32;
        (*df).pt = std::mem::zeroed();
        (*df).fNC = false.into();
        (*df).fWide = true.into();
        let dst = buf.as_mut_ptr().add(dropfiles_size) as *mut u16;
        std::ptr::copy_nonoverlapping(wide.as_ptr(), dst, wide.len());
    }
    buf
}

/// バイト列を HGLOBAL (GHND) へコピーして返す (毎回新規 alloc。use-after-free 回避)。
///
/// # Safety
/// 戻り値 HANDLE の解放責務は呼び出し側にある (クリップボード/OLE へ所有権を
/// 渡すか、`HGlobalGuard` で包むこと)。
pub unsafe fn alloc_hglobal_from_bytes(bytes: &[u8]) -> AppResult<HANDLE> {
    let h =
        GlobalAlloc(GHND, bytes.len()).map_err(|e| AppError::Win32(format!("GlobalAlloc: {e}")))?;
    if h.is_invalid() {
        return Err(AppError::Win32("GlobalAlloc invalid".into()));
    }
    let p = GlobalLock(h) as *mut u8;
    if p.is_null() {
        let _ = GlobalFree(h);
        return Err(AppError::Win32("GlobalLock failed".into()));
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
    let _ = GlobalUnlock(h);
    Ok(HANDLE(h.0))
}

/// DWORD 1 個の HGLOBAL (Preferred DropEffect 等)。
///
/// # Safety
/// `alloc_hglobal_from_bytes` と同じ (解放責務は呼び出し側)。
pub unsafe fn alloc_hglobal_dword(v: u32) -> AppResult<HANDLE> {
    alloc_hglobal_from_bytes(&v.to_le_bytes())
}

/// 「SetClipboardData へ渡すまで」の HGLOBAL リーク防止ガード。
/// OS へ所有権が移ったら `disarm()` する。エラーパスで自動的に GlobalFree される
/// (従来はエラー時に DROPFILES 全量が漏れていた)。
pub struct HGlobalGuard {
    h: HANDLE,
    armed: bool,
}

impl HGlobalGuard {
    pub fn from_bytes(bytes: &[u8]) -> AppResult<Self> {
        Ok(Self {
            h: unsafe { alloc_hglobal_from_bytes(bytes)? },
            armed: true,
        })
    }

    pub fn from_dword(v: u32) -> AppResult<Self> {
        Ok(Self {
            h: unsafe { alloc_hglobal_dword(v)? },
            armed: true,
        })
    }

    pub fn handle(&self) -> HANDLE {
        self.h
    }

    /// 所有権が OS (クリップボード等) へ移った — 以後こちらでは解放しない。
    pub fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for HGlobalGuard {
    fn drop(&mut self) {
        if self.armed {
            unsafe {
                let _ = GlobalFree(HGLOBAL(self.h.0));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 旧 ole_dnd::build_hdrop_bytes / 旧 win_clipboard の手組みとバイト互換。
    #[test]
    fn hdrop_bytes_layout_is_stable() {
        let buf = build_hdrop_bytes(["C:/a", "D:\\b"]);
        let header = std::mem::size_of::<DROPFILES>();
        // pFiles = ヘッダサイズ (リトルエンディアン u32)
        assert_eq!(
            u32::from_le_bytes(buf[0..4].try_into().unwrap()) as usize,
            header
        );
        // ペイロード = "C:\a\0D:\b\0\0" (UTF-16LE、'/' は正規化済み)
        let payload: Vec<u16> = buf[header..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let expect: Vec<u16> = "C:\\a\0D:\\b\0\0".encode_utf16().collect();
        assert_eq!(payload, expect);
    }
}
