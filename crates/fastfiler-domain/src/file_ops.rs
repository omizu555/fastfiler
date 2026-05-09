// Phase 1+3: 基本ファイル操作
//
// - copy / move / rename / mkdir / delete (std::fs)
// - delete_to_trash: Windows IFileOperation 経由でゴミ箱送り (Phase 3)

use crate::error::{AppError, AppResult};
use std::fs;
use std::path::PathBuf;

pub fn create_dir(path: String) -> AppResult<()> {
    fs::create_dir_all(PathBuf::from(path))?;
    Ok(())
}

pub fn rename_path(from: String, to: String) -> AppResult<()> {
    fs::rename(PathBuf::from(from), PathBuf::from(to))?;
    Ok(())
}

pub fn delete_path(path: String, recursive: bool) -> AppResult<()> {
    let p = PathBuf::from(&path);
    let meta = fs::metadata(&p)?;
    if meta.is_dir() {
        if recursive { fs::remove_dir_all(&p)?; } else { fs::remove_dir(&p)?; }
    } else {
        fs::remove_file(&p)?;
    }
    Ok(())
}

pub fn copy_path(from: String, to: String) -> AppResult<()> {
    let src = PathBuf::from(&from);
    let dst = PathBuf::from(&to);
    let meta = fs::metadata(&src)?;
    if meta.is_dir() {
        copy_dir_recursive(&src, &dst)?;
    } else {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&src, &dst)?;
    }
    Ok(())
}

pub fn move_path(from: String, to: String) -> AppResult<()> {
    let src = PathBuf::from(&from);
    let dst = PathBuf::from(&to);
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(&src, &dst) {
        Ok(_) => Ok(()),
        Err(_) => {
            let meta = fs::metadata(&src)?;
            if meta.is_dir() {
                copy_dir_recursive(&src, &dst)?;
                fs::remove_dir_all(&src)?;
            } else {
                fs::copy(&src, &dst)?;
                fs::remove_file(&src)?;
            }
            Ok(())
        }
    }
}

fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> AppResult<()> {
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
        return trash_impl::delete_to_trash(paths);
    }
    #[cfg(not(windows))]
    {
        let _ = paths;
        Err(AppError::NotSupported("trash only supported on Windows".into()))
    }
}

#[cfg(windows)]
mod trash_impl {
    use super::*;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::{
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT,
        FO_DELETE, SHFILEOPSTRUCTW,
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
            op.fFlags = (FOF_ALLOWUNDO.0
                | FOF_NOCONFIRMATION.0
                | FOF_NOERRORUI.0
                | FOF_SILENT.0) as u16;
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
}

