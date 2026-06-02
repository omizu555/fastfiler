// Phase 1+3: 基本ファイル操作
//
// - copy / move / rename / mkdir / delete (std::fs)
// - delete_to_trash: Windows IFileOperation 経由でゴミ箱送り (Phase 3)
// - rename_path_no_overwrite / move_path_no_overwrite / restore_from_trash: Undo 経路用 (ADR 0008)

use crate::error::{AppError, AppResult};
use crate::undo::TrashedItem;
use std::fs;
use std::path::Path;

pub fn create_dir(path: &Path) -> AppResult<()> {
    fs::create_dir_all(path)?;
    Ok(())
}

pub fn rename_path(from: &Path, to: &Path) -> AppResult<()> {
    fs::rename(from, to)?;
    Ok(())
}

pub fn delete_path(path: &Path, recursive: bool) -> AppResult<()> {
    let meta = fs::metadata(path)?;
    if meta.is_dir() {
        if recursive {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_dir(path)?;
        }
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn copy_path(from: &Path, to: &Path) -> AppResult<()> {
    let meta = fs::metadata(from)?;
    if meta.is_dir() {
        copy_dir_recursive(from, to)?;
    } else {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(from, to)?;
    }
    Ok(())
}

pub fn move_path(from: &Path, to: &Path) -> AppResult<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(from, to) {
        Ok(_) => Ok(()),
        Err(_) => {
            let meta = fs::metadata(from)?;
            if meta.is_dir() {
                copy_dir_recursive(from, to)?;
                fs::remove_dir_all(from)?;
            } else {
                fs::copy(from, to)?;
                fs::remove_file(from)?;
            }
            Ok(())
        }
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> AppResult<()> {
    fs::create_dir_all(dst)?;
    for ent in fs::read_dir(src)? {
        let ent = ent?;
        let from = ent.path();
        let to = dst.join(ent.file_name());
        let m = ent.metadata()?;
        if m.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// 複数ファイルをゴミ箱へ送る (Windows IFileOperation)
pub fn delete_to_trash(paths: Vec<String>) -> AppResult<()> {
    #[cfg(windows)]
    {
        trash_impl::delete_to_trash(paths)
    }
    #[cfg(not(windows))]
    {
        let _ = paths;
        Err(AppError::NotSupported(
            "trash only supported on Windows".into(),
        ))
    }
}

// ─────────────────────────────────────────────────────────────
// Undo 用 API (ADR 0008)
// ─────────────────────────────────────────────────────────────

/// 上書き禁止版 rename。`to` が既に存在したら失敗扱い。
/// Undo の逆 rename / 逆 move に使う。
pub fn rename_path_no_overwrite(from: &Path, to: &Path) -> AppResult<()> {
    if to.exists() {
        return Err(AppError::Other(format!(
            "destination exists: {}",
            to.display()
        )));
    }
    fs::rename(from, to)?;
    Ok(())
}

/// 上書き禁止版 move。`to` が存在すれば失敗、親フォルダは必要なら作成する。
/// ドライブ跨ぎフォールバックも上書きしない (`copy_path_no_overwrite` 経由)。
pub fn move_path_no_overwrite(from: &Path, to: &Path) -> AppResult<()> {
    if to.exists() {
        return Err(AppError::Other(format!(
            "destination exists: {}",
            to.display()
        )));
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    // ドライブ跨ぎ等で rename が失敗した場合は copy + remove で擬似 move。
    // 上書きしない (dst 存在チェックは冒頭で済み)。
    let meta = fs::metadata(from)?;
    if meta.is_dir() {
        copy_dir_no_overwrite(from, to)?;
        fs::remove_dir_all(from)?;
    } else {
        fs::copy(from, to)?;
        fs::remove_file(from)?;
    }
    Ok(())
}

fn copy_dir_no_overwrite(src: &Path, dst: &Path) -> AppResult<()> {
    if dst.exists() {
        return Err(AppError::Other(format!(
            "destination exists: {}",
            dst.display()
        )));
    }
    fs::create_dir(dst)?;
    for ent in fs::read_dir(src)? {
        let ent = ent?;
        let from = ent.path();
        let to = dst.join(ent.file_name());
        let m = ent.metadata()?;
        if m.is_dir() {
            copy_dir_no_overwrite(&from, &to)?;
        } else {
            // この copy は親フォルダが直前に dst として作られたばかりなので衝突しない。
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// ゴミ箱から `item.original_path` の場所へ復元する。
/// 元場所が空でない場合は上書きせず失敗する。
pub fn restore_from_trash(item: &TrashedItem) -> AppResult<()> {
    #[cfg(windows)]
    {
        trash_impl::restore_from_trash(item)
    }
    #[cfg(not(windows))]
    {
        let _ = item;
        Err(AppError::NotSupported(
            "restore_from_trash only supported on Windows".into(),
        ))
    }
}

#[cfg(windows)]
mod trash_impl {
    use super::*;
    use crate::undo::TrashedItem;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{FILETIME, HWND};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::PropertiesSystem::{
        IPropertyStore, PSGetPropertyKeyFromName, PROPERTYKEY,
    };
    use windows::Win32::UI::Shell::{
        BHID_EnumItems, BHID_PropertyStore, FOLDERID_RecycleBinFolder, FileOperation,
        IEnumShellItems, IFileOperation, IShellItem, SHCreateItemFromParsingName, SHFileOperationW,
        SHGetKnownFolderItem, FILEOPERATION_FLAGS, FOF_ALLOWUNDO, FOF_NOCONFIRMATION,
        FOF_NOERRORUI, FOF_SILENT, FO_DELETE, KF_FLAG_DEFAULT, SHFILEOPSTRUCTW,
        SIGDN_NORMALDISPLAY,
    };

    pub fn delete_to_trash(paths: Vec<String>) -> AppResult<()> {
        // SHFileOperationW は呼び出しスレッドの COM 状態に依存しない安定 API。
        // 文字列はダブル NUL 終端の wide リストにする。
        let mut wide: Vec<u16> = Vec::new();
        for p in &paths {
            // 念のため正規化 (バックスラッシュに揃える)
            let normalized: String = p.chars().map(|c| if c == '/' { '\\' } else { c }).collect();
            for u in normalized.encode_utf16() {
                wide.push(u);
            }
            wide.push(0);
        }
        wide.push(0); // 二重 NUL 終端

        // catch_unwind で Rust panic は受ける (FFI 由来 SEH は別途ガード対象だが
        // SHFileOperationW は HRESULT/int を返すため通常は SEH を起こさない)。
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            let mut op: SHFILEOPSTRUCTW = std::mem::zeroed();
            op.hwnd = HWND::default();
            op.wFunc = FO_DELETE;
            op.pFrom = windows::core::PCWSTR(wide.as_ptr());
            op.pTo = windows::core::PCWSTR::null();
            op.fFlags =
                (FOF_ALLOWUNDO.0 | FOF_NOCONFIRMATION.0 | FOF_NOERRORUI.0 | FOF_SILENT.0) as u16;
            SHFileOperationW(&mut op as *mut _)
        }));

        match result {
            Ok(0) => Ok(()),
            Ok(code) => Err(AppError::Win32(format!(
                "SHFileOperationW failed (code=0x{:X})",
                code
            ))),
            Err(_) => Err(AppError::Win32(
                "SHFileOperationW panicked (unwind caught)".into(),
            )),
        }
    }

    // ─────────────────────────────────────────────────────────────
    // restore_from_trash (ADR 0008)
    //
    // 方針:
    //  1. 復元先 (item.original_path) が既に存在する場合は失敗
    //  2. ゴミ箱を列挙し、各エントリから
    //     - PKEY `System.Recycle.DeletedFrom` (**親フォルダパスのみ**、ファイル名は含まない)
    //     - `IShellItem::GetDisplayName(SIGDN_PARENTRELATIVEPARSING)` で元ファイル名
    //     を取得し、(親ディレクトリ + ファイル名) の両方が一致するエントリを集める。
    //  3. ちょうど 1 件のときのみ復元
    //  4. 0 件 / 2 件以上のときは AppError::NotFound / Other で失敗
    //     → 呼出側が「ゴミ箱を開く」フォールバック UI を出す
    //  5. IFileOperation::MoveItem で元の親フォルダへ移動
    // ─────────────────────────────────────────────────────────────

    pub fn restore_from_trash(item: &TrashedItem) -> AppResult<()> {
        if item.original_path.exists() {
            return Err(AppError::Other(format!(
                "destination exists: {}",
                item.original_path.display()
            )));
        }
        let parent = item
            .original_path
            .parent()
            .ok_or_else(|| AppError::InvalidPath(item.original_path.display().to_string()))?
            .to_path_buf();

        // catch_unwind は不要 (HRESULT で返るため)。CoInitialize は本スレッド限定で in-out。
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        // CoUninitialize は restore 終了後に呼ぶ (Drop ガード)
        struct ComGuard;
        impl Drop for ComGuard {
            fn drop(&mut self) {
                unsafe { CoUninitialize() };
            }
        }
        let _com = ComGuard;

        let res = (|| -> AppResult<()> {
            // 復元先親フォルダの IShellItem
            let parent_wide: Vec<u16> = parent
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let dest_folder: IShellItem = unsafe {
                SHCreateItemFromParsingName(PCWSTR(parent_wide.as_ptr()), None)
                    .map_err(|e| AppError::Win32(format!("dest IShellItem: {e}")))?
            };

            // ゴミ箱の IShellItem
            let bin: IShellItem = unsafe {
                SHGetKnownFolderItem(&FOLDERID_RecycleBinFolder, KF_FLAG_DEFAULT, None)
                    .map_err(|e| AppError::Win32(format!("RecycleBin folder: {e}")))?
            };
            let enumer: IEnumShellItems = unsafe { bin.BindToHandler(None, &BHID_EnumItems) }
                .map_err(|e| AppError::Win32(format!("EnumItems: {e}")))?;

            // PKEY: System.Recycle.DeletedFrom
            let mut pkey_from = PROPERTYKEY::default();
            unsafe {
                PSGetPropertyKeyFromName(
                    windows::core::w!("System.Recycle.DeletedFrom"),
                    &mut pkey_from,
                )
                .map_err(|e| AppError::Win32(format!("PSGetPropertyKeyFromName: {e}")))?;
            }
            // PKEY: System.Recycle.DateDeleted (同名・同パスのゴミ箱エントリを区別するために使う)
            let mut pkey_date = PROPERTYKEY::default();
            unsafe {
                PSGetPropertyKeyFromName(
                    windows::core::w!("System.Recycle.DateDeleted"),
                    &mut pkey_date,
                )
                .map_err(|e| AppError::Win32(format!("PSGetPropertyKeyFromName: {e}")))?;
            }
            let target_ft = systemtime_to_filetime_100ns(item.deleted_at);

            // 親ディレクトリと元フルパス (display name と比較)
            let parent_str = parent.to_string_lossy().to_string();
            let parent_norm = normalize_path_compare(&parent_str);
            let original_norm = normalize_path_compare(&item.original_path.to_string_lossy());

            // 列挙ループ
            // (IShellItem, DateDeleted を 100ns 単位 FILETIME に変換した u64 (取得できなければ None))
            let mut matches: Vec<(IShellItem, Option<u64>)> = Vec::new();
            loop {
                let mut fetched: u32 = 0;
                let mut buf: [Option<IShellItem>; 8] = Default::default();
                let hr = unsafe { enumer.Next(&mut buf, Some(&mut fetched)) };
                if hr.is_err() || fetched == 0 {
                    break;
                }
                for slot in buf.iter_mut().take(fetched as usize) {
                    let Some(it) = slot.take() else { continue };
                    // PropertyStore から DeletedFrom (= 親ディレクトリ) と DateDeleted を取得。
                    // 一部 (画像など WIC が絡む) エントリでは取得に失敗するが、ゴミ箱本体の
                    // メタデータ取得とは無関係 (WINCODEC_ERR_COMPONENTNOTFOUND) なので無視。
                    let ps_res: Result<IPropertyStore, _> =
                        unsafe { it.BindToHandler(None, &BHID_PropertyStore) };
                    let Ok(ps) = ps_res else {
                        continue;
                    };
                    let Some(from_str) = read_string_prop(&ps, &pkey_from) else {
                        continue;
                    };
                    if normalize_path_compare(&from_str) != parent_norm {
                        continue;
                    }
                    // 元ファイル名は SIGDN_NORMALDISPLAY (ゴミ箱では元フルパスを返す挙動)
                    let name_str = unsafe { it.GetDisplayName(SIGDN_NORMALDISPLAY) }
                        .ok()
                        .map(|p| unsafe { pwstr_to_string_and_free(p) })
                        .unwrap_or_default();
                    if normalize_path_compare(&name_str) == original_norm {
                        let ft = read_filetime_prop(&ps, &pkey_date).map(filetime_to_u64);
                        matches.push((it, ft));
                    }
                }
            }

            if matches.is_empty() {
                return Err(AppError::NotFound(format!(
                    "trash entry not found for: {}",
                    item.original_path.display()
                )));
            }

            // 複数 match のときは TrashedItem.deleted_at に最も時刻が近いエントリを採用する。
            // (同一元パスのファイルを過去にも削除したことがあるケースで、対応する 1 件を選ぶ)
            // DateDeleted が取れなかったエントリは比較不能扱いで最後に回す。
            let chosen_idx: usize = if matches.len() == 1 {
                0
            } else {
                let mut best_idx = 0usize;
                let mut best_diff: Option<u64> = None;
                for (i, (_, ft)) in matches.iter().enumerate() {
                    if let Some(ft) = ft {
                        let diff = ft.abs_diff(target_ft);
                        if best_diff.map_or(true, |b| diff < b) {
                            best_diff = Some(diff);
                            best_idx = i;
                        }
                    }
                }
                // 全エントリで DateDeleted が取れなかった場合は最新削除 (= 最後の列挙) を使う
                if best_diff.is_none() {
                    matches.len() - 1
                } else {
                    best_idx
                }
            };

            let chosen = &matches[chosen_idx].0;

            // IFileOperation::MoveItem で復元
            let op: IFileOperation = unsafe {
                CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| AppError::Win32(format!("FileOperation: {e}")))?
            };
            unsafe {
                op.SetOperationFlags(FILEOPERATION_FLAGS(
                    FOF_NOCONFIRMATION.0 | FOF_NOERRORUI.0 | FOF_SILENT.0,
                ))
                .map_err(|e| AppError::Win32(format!("SetOperationFlags: {e}")))?;

                // file_name は元のまま戻したい → NewName に元のファイル名を指定
                let name_wide: Vec<u16> = item
                    .file_name
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();
                op.MoveItem(chosen, &dest_folder, PCWSTR(name_wide.as_ptr()), None)
                    .map_err(|e| AppError::Win32(format!("MoveItem: {e}")))?;
                op.PerformOperations()
                    .map_err(|e| AppError::Win32(format!("PerformOperations: {e}")))?;
            }
            Ok(())
        })();

        res
    }

    fn read_string_prop(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<String> {
        use windows::Win32::System::Com::StructuredStorage::{
            PropVariantClear, PropVariantToString,
        };
        unsafe {
            let mut pv = store.GetValue(key).ok()?;
            // PropVariantToString は呼び出し側 buffer を要求する。260 文字あれば NTFS パスの大半をカバー。
            let mut buf = [0u16; 1024];
            let res = PropVariantToString(&pv, &mut buf);
            let _ = PropVariantClear(&mut pv);
            res.ok()?;
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Some(String::from_utf16_lossy(&buf[..len]))
        }
    }

    /// `IPropertyStore` から FILETIME (VT_FILETIME) を読む。
    /// 失敗時や型不一致時は None。
    fn read_filetime_prop(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<FILETIME> {
        use windows::Win32::System::Com::StructuredStorage::{
            PropVariantClear, PropVariantToFileTime,
        };
        use windows::Win32::System::Variant::PSTF_UTC;
        unsafe {
            let mut pv = store.GetValue(key).ok()?;
            let res = PropVariantToFileTime(&pv, PSTF_UTC);
            let _ = PropVariantClear(&mut pv);
            res.ok()
        }
    }

    fn filetime_to_u64(ft: FILETIME) -> u64 {
        ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64)
    }

    /// `SystemTime` を FILETIME 相当 (1601-01-01 UTC からの 100ns ticks) に変換する。
    fn systemtime_to_filetime_100ns(t: std::time::SystemTime) -> u64 {
        // UNIX エポック (1970-01-01) と FILETIME エポック (1601-01-01) の差
        const EPOCH_DIFF_100NS: u64 = 116_444_736_000_000_000;
        match t.duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => {
                let secs = d.as_secs();
                let sub = (d.subsec_nanos() as u64) / 100;
                secs.saturating_mul(10_000_000)
                    .saturating_add(sub)
                    .saturating_add(EPOCH_DIFF_100NS)
            }
            Err(_) => EPOCH_DIFF_100NS,
        }
    }

    fn normalize_path_compare(s: &str) -> String {
        let mut out: String = s.chars().map(|c| if c == '/' { '\\' } else { c }).collect();
        out = out.to_lowercase();
        // 末尾 '\\' を取り除く (ディレクトリ表記揺れ吸収)
        while out.ends_with('\\') && out.len() > 3 {
            out.pop();
        }
        out
    }

    /// COM が確保した PWSTR (null 終端 UTF-16) を Rust String にコピーし、
    /// CoTaskMemFree で解放する。null ポインタなら空文字を返す。
    unsafe fn pwstr_to_string_and_free(p: windows::core::PWSTR) -> String {
        use windows::Win32::System::Com::CoTaskMemFree;
        if p.is_null() {
            return String::new();
        }
        let mut len = 0usize;
        while *p.0.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(p.0, len);
        let s = String::from_utf16_lossy(slice);
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }

    use std::os::windows::ffi::OsStrExt;
}
