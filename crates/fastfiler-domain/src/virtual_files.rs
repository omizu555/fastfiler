//! 仮想ファイルクリップボード (CFSTR_FILEDESCRIPTORW + CFSTR_FILECONTENTS) の受信。
//!
//! リモートデスクトップ (rdpclip / MS-RDPECLIP) や Outlook 添付のコピーは、
//! 実パスの CF_HDROP ではなく「ファイル記述子の一覧 + 内容ストリーム」の
//! 仮想形式でクリップボードに載る。エクスプローラはこの形式で貼り付けできる —
//! 本モジュールはその受信側パリティを提供する。
//!
//! 分担:
//!  - 記述子 (名前/属性/サイズ) は**素のクリップボード API** で読む
//!    (OLE 不要・軽量 — can_paste 判定と衝突計画に使う)
//!  - 内容 (FILECONTENTS) は lindex 指定の `IDataObject::GetData` が必須なので
//!    `OleGetClipboard` で読む。呼び出しスレッドで OLE を初期化する
//!    (`with_ole_clipboard`) — ジョブ専用スレッドから使うこと
//!    (rdpclip の遅延レンダリングでブロックするため UI スレッド禁止)。
//!
//! 記述子は**任意の外部プロセスが書ける入力**なので、cFileName は
//! `sanitize_rel_path` で検証してから使う (絶対パス・`..`・不正文字の拒否)。

use crate::error::{AppError, AppResult};

/// 仮想ファイル 1 件。`index` はクリップボード内の FILECONTENTS lindex。
/// `rel_path` は貼り付け先相対の `dir\file.txt` 形式 (サニタイズ済み)。
#[derive(Debug, Clone, PartialEq)]
pub struct VirtualFileEntry {
    pub index: u32,
    pub rel_path: String,
    pub is_dir: bool,
    /// FD_FILESIZE が立っているときのみ Some (進捗表示の合計に使う)。
    pub size: Option<u64>,
}

/// 仮想ファイル形式がクリップボードにあるか (貼り付けメニューの淡色判定)。
/// IsClipboardFormatAvailable は遅延レンダリングを起こさない軽量チェック。
pub fn virtual_files_available() -> bool {
    #[cfg(not(windows))]
    {
        return false;
    }
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::DataExchange::IsClipboardFormatAvailable;
        IsClipboardFormatAvailable(cf_file_group_descriptor_w()).is_ok()
    }
}

/// クリップボードの仮想ファイル記述子一覧を読む。形式がなければ `Ok(None)`。
/// サニタイズで弾かれた項目は除外する (全滅なら `Ok(None)`)。
pub fn clipboard_read_virtual_entries() -> AppResult<Option<Vec<VirtualFileEntry>>> {
    #[cfg(not(windows))]
    {
        return Ok(None);
    }
    #[cfg(windows)]
    unsafe {
        read_descriptors_win()
    }
}

/// FILEDESCRIPTOR の cFileName を宛先相対パスとして安全化する。
/// 区切りは `\` と `/` の両方を許し `\` に正規化。次を拒否して None:
/// 絶対パス (先頭区切り・UNC)・ドライブや ADS の `:`・`.`/`..` 成分・
/// Windows 不正文字/制御文字・末尾が `.` か空白の成分 (CreateFile が
/// 静かに削って別名になるため)・予約デバイス名 (CON / NUL 等)。
///
/// ⚠ core `update.rs::check_folder_line` (F7 の階層フォルダ検証) と規則の双子
/// (core は domain 非依存のため別実装)。規則を変えるときは両方揃えること。
pub fn sanitize_rel_path(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    const INVALID: [char; 7] = [':', '*', '?', '"', '<', '>', '|'];
    let mut parts: Vec<&str> = Vec::new();
    for comp in raw.split(['\\', '/']) {
        if comp.is_empty() || comp == "." || comp == ".." {
            return None; // 先頭区切り (絶対/UNC) も空成分としてここで落ちる
        }
        if comp.contains(INVALID) || comp.chars().any(char::is_control) {
            return None;
        }
        if comp.ends_with('.') || comp.ends_with(' ') {
            return None;
        }
        if is_reserved_device_name(comp) {
            return None;
        }
        parts.push(comp);
    }
    Some(parts.join("\\"))
}

/// Windows の予約デバイス名 (CON / PRN / AUX / NUL / COM1-9 / LPT1-9) か。
/// 拡張子付き ("NUL.txt") も予約扱い (最初の . より前で判定)。書き込むと
/// 古い API がデバイスへ解決し得るため受信を拒否する。
fn is_reserved_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    let up = stem.to_ascii_uppercase();
    match up.as_str() {
        "CON" | "PRN" | "AUX" | "NUL" => true,
        _ => {
            up.len() == 4
                && (up.starts_with("COM") || up.starts_with("LPT"))
                && up.as_bytes()[3].is_ascii_digit()
                && up.as_bytes()[3] != b'0' // COM0/LPT0 は予約外
        }
    }
}

// ================================================================
// Windows 実装
// ================================================================

#[cfg(windows)]
fn cf_file_group_descriptor_w() -> u32 {
    use windows::Win32::System::DataExchange::RegisterClipboardFormatA;
    unsafe { RegisterClipboardFormatA(windows::core::s!("FileGroupDescriptorW")) }
}

#[cfg(windows)]
fn cf_file_contents() -> u32 {
    use windows::Win32::System::DataExchange::RegisterClipboardFormatA;
    unsafe { RegisterClipboardFormatA(windows::core::s!("FileContents")) }
}

#[cfg(windows)]
unsafe fn read_descriptors_win() -> AppResult<Option<Vec<VirtualFileEntry>>> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY;
    use windows::Win32::System::DataExchange::{GetClipboardData, IsClipboardFormatAvailable};
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    use windows::Win32::UI::Shell::{FD_ATTRIBUTES, FD_FILESIZE, FILEDESCRIPTORW};

    let cf = cf_file_group_descriptor_w();
    if cf == 0 || IsClipboardFormatAvailable(cf).is_err() {
        return Ok(None);
    }
    crate::win_clipboard::with_clipboard(|| {
        let h = GetClipboardData(cf)
            .map_err(|e| AppError::Win32(format!("GetClipboardData(FileGroupDescriptorW): {e}")))?;
        if h.is_invalid() {
            return Ok(None);
        }
        let hg = HGLOBAL(h.0);
        let total = GlobalSize(hg);
        let base = GlobalLock(hg) as *const u8;
        if base.is_null() {
            return Err(AppError::Win32("GlobalLock 失敗".into()));
        }
        let parsed = (|| {
            // FILEGROUPDESCRIPTORW = DWORD cItems + FILEDESCRIPTORW[cItems] (packed)。
            // GlobalSize と照合して範囲外読み取りを断つ (外部プロセスが書ける入力)
            if total < 4 {
                return Err(AppError::Win32(
                    "FileGroupDescriptorW が小さすぎます".into(),
                ));
            }
            let citems = (base as *const u32).read_unaligned() as usize;
            let fd_size = std::mem::size_of::<FILEDESCRIPTORW>();
            if citems == 0 || total < 4 + citems * fd_size {
                return Err(AppError::Win32(format!(
                    "FileGroupDescriptorW の項目数が不正です (cItems={citems}, size={total})"
                )));
            }
            let mut entries: Vec<VirtualFileEntry> = Vec::with_capacity(citems);
            for i in 0..citems {
                // packed(1) 構造体なので read_unaligned でコピーし、フィールドも
                // ローカルへ複写してから使う (packed フィールドへの参照は UB — E0793)
                let fd = (base.add(4 + i * fd_size) as *const FILEDESCRIPTORW).read_unaligned();
                let name_buf: [u16; 260] = fd.cFileName;
                let (flags, attrs) = (fd.dwFlags, fd.dwFileAttributes);
                let (size_hi, size_lo) = (fd.nFileSizeHigh, fd.nFileSizeLow);
                let name_len = name_buf
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(name_buf.len());
                let raw = String::from_utf16_lossy(&name_buf[..name_len]);
                let Some(rel_path) = sanitize_rel_path(&raw) else {
                    continue;
                };
                let is_dir =
                    flags & FD_ATTRIBUTES.0 as u32 != 0 && attrs & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
                let size = (flags & FD_FILESIZE.0 as u32 != 0 && !is_dir)
                    .then_some(((size_hi as u64) << 32) | size_lo as u64);
                entries.push(VirtualFileEntry {
                    index: i as u32,
                    rel_path,
                    is_dir,
                    size,
                });
            }
            Ok(if entries.is_empty() {
                None
            } else {
                Some(entries)
            })
        })();
        let _ = GlobalUnlock(hg);
        parsed
    })
}

/// 呼び出しスレッドで OLE を初期化し、クリップボードの `IDataObject` を f へ渡す。
/// ジョブ専用スレッドから呼ぶこと (UI スレッド禁止 — ドキュメントコメント参照)。
#[cfg(windows)]
pub fn with_ole_clipboard<T>(
    f: impl FnOnce(&windows::Win32::System::Com::IDataObject) -> AppResult<T>,
) -> AppResult<T> {
    use windows::Win32::System::Ole::{OleGetClipboard, OleInitialize, OleUninitialize};
    let hr = unsafe { OleInitialize(None) };
    // IDataObject (COM プロキシ) を OleUninitialize より先に解放するため
    // クロージャスコープで完結させる
    let r = (|| {
        let data = unsafe { OleGetClipboard() }
            .map_err(|e| AppError::Win32(format!("OleGetClipboard: {e}")))?;
        f(&data)
    })();
    if hr.is_ok() {
        unsafe { OleUninitialize() };
    }
    r
}

/// FILECONTENTS (lindex = index) を dest へ書き出す。IStream / HGLOBAL の両
/// tymed に対応。on_chunk には書けたバイト数を渡す (進捗集計用)。
#[cfg(windows)]
pub fn extract_contents_to_file(
    data: &windows::Win32::System::Com::IDataObject,
    index: u32,
    dest: &std::path::Path,
    cancel: &std::sync::atomic::AtomicBool,
    on_chunk: &mut dyn FnMut(u64),
) -> AppResult<()> {
    use std::io::Write;
    use std::sync::atomic::Ordering;
    use windows::Win32::System::Com::{
        DVASPECT_CONTENT, FORMATETC, STGMEDIUM, TYMED_HGLOBAL, TYMED_ISTREAM,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    use windows::Win32::System::Ole::ReleaseStgMedium;

    struct StgMediumGuard(STGMEDIUM);
    impl Drop for StgMediumGuard {
        fn drop(&mut self) {
            unsafe { ReleaseStgMedium(&mut self.0) };
        }
    }

    unsafe {
        let fe = FORMATETC {
            cfFormat: cf_file_contents() as u16,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: index as i32,
            tymed: (TYMED_ISTREAM.0 | TYMED_HGLOBAL.0) as u32,
        };
        let stgm = data
            .GetData(&fe)
            .map_err(|e| AppError::Win32(format!("GetData(FileContents[{index}]): {e}")))?;
        let guard = StgMediumGuard(stgm);
        let stgm = &guard.0;

        let mut file = std::fs::File::create(dest)?;
        if stgm.tymed == TYMED_ISTREAM.0 as u32 {
            let stream =
                stgm.u.pstm.as_ref().ok_or_else(|| {
                    AppError::Win32("FileContents の IStream が null です".into())
                })?;
            // rdpclip はここで初めて実データをチャネル転送する — チャンクごとに
            // キャンセルを見る (巨大ファイルの中断点)
            let mut buf = vec![0u8; 256 * 1024];
            loop {
                if cancel.load(Ordering::SeqCst) {
                    return Err(AppError::Canceled);
                }
                let mut read: u32 = 0;
                let hr = stream.Read(
                    buf.as_mut_ptr() as *mut core::ffi::c_void,
                    buf.len() as u32,
                    Some(&mut read),
                );
                if hr.is_err() {
                    return Err(AppError::Win32(format!(
                        "IStream::Read(FileContents[{index}]): {hr}"
                    )));
                }
                if read == 0 {
                    break; // S_OK/S_FALSE で 0 バイト = 終端
                }
                file.write_all(&buf[..read as usize])?;
                on_chunk(read as u64);
            }
        } else if stgm.tymed == TYMED_HGLOBAL.0 as u32 {
            let hg = stgm.u.hGlobal;
            let size = GlobalSize(hg);
            let p = GlobalLock(hg) as *const u8;
            if p.is_null() {
                return Err(AppError::Win32("GlobalLock(FileContents) 失敗".into()));
            }
            let res = file.write_all(std::slice::from_raw_parts(p, size));
            let _ = GlobalUnlock(hg);
            res?;
            on_chunk(size as u64);
        } else {
            return Err(AppError::Win32(format!(
                "FileContents tymed={} (expected ISTREAM/HGLOBAL)",
                stgm.tymed
            )));
        }
        file.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_rel_path;

    #[test]
    fn sanitize_accepts_plain_and_nested_names() {
        assert_eq!(sanitize_rel_path("a.txt"), Some("a.txt".into()));
        assert_eq!(sanitize_rel_path("dir\\a.txt"), Some("dir\\a.txt".into()));
        // スラッシュ区切りも \ に正規化して受ける
        assert_eq!(sanitize_rel_path("dir/a.txt"), Some("dir\\a.txt".into()));
        assert_eq!(
            sanitize_rel_path("日本語 (2).txt"),
            Some("日本語 (2).txt".into())
        );
    }

    #[test]
    fn sanitize_rejects_escapes_and_invalid_names() {
        // 宛先の外へ逃がすパス
        assert_eq!(sanitize_rel_path("..\\evil.txt"), None);
        assert_eq!(sanitize_rel_path("dir\\..\\evil.txt"), None);
        assert_eq!(sanitize_rel_path("\\abs.txt"), None);
        assert_eq!(sanitize_rel_path("\\\\server\\share\\x"), None);
        assert_eq!(sanitize_rel_path("C:\\abs.txt"), None); // `:` 拒否 (ADS も同時に防ぐ)
                                                            // Windows の命名規則違反
        assert_eq!(sanitize_rel_path("a?.txt"), None);
        assert_eq!(sanitize_rel_path("a\u{7}.txt"), None);
        assert_eq!(sanitize_rel_path("name."), None); // CreateFile が末尾 . を静かに削る
        assert_eq!(sanitize_rel_path("name "), None);
        assert_eq!(sanitize_rel_path(""), None);
        assert_eq!(sanitize_rel_path("dir\\"), None); // 空成分
                                                      // 予約デバイス名 (拡張子付き含む) は成分単位で拒否
        assert_eq!(sanitize_rel_path("con"), None);
        assert_eq!(sanitize_rel_path("NUL.txt"), None);
        assert_eq!(sanitize_rel_path("dir\\com1\\x"), None);
        // 似ているだけの名前は通す
        assert_eq!(sanitize_rel_path("control"), Some("control".into()));
        assert_eq!(sanitize_rel_path("com10"), Some("com10".into()));
        assert_eq!(sanitize_rel_path("com0"), Some("com0".into()));
    }
}
