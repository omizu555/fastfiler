// Phase 1+3: 基本ファイル操作
//
// - copy / move / rename / mkdir / delete (std::fs)
// - delete_to_trash: Windows IFileOperation 経由でゴミ箱送り (Phase 3)

use crate::error::{AppError, AppResult};
use std::fs;
use std::path::PathBuf;

#[tauri::command]
pub fn create_dir(path: String) -> AppResult<()> {
    fs::create_dir_all(PathBuf::from(path))?;
    Ok(())
}

#[tauri::command]
pub fn rename_path(from: String, to: String) -> AppResult<()> {
    fs::rename(PathBuf::from(from), PathBuf::from(to))?;
    Ok(())
}

#[tauri::command]
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

#[tauri::command]
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

#[tauri::command]
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
#[tauri::command]
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
    use std::thread;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL,
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::UI::Shell::{
        FileOperation, IFileOperation, IShellItem, SHCreateItemFromParsingName,
        FILEOPERATION_FLAGS, FOFX_ADDUNDORECORD, FOFX_RECYCLEONDELETE,
        FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT,
    };

    pub fn delete_to_trash(paths: Vec<String>) -> AppResult<()> {
        // COM STA を持つ専用スレッドで実行
        let handle = thread::spawn(move || -> AppResult<()> {
            unsafe {
                let hr = CoInitializeEx(
                    None,
                    COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE,
                );
                if hr.is_err() {
                    return Err(AppError::Other(format!("CoInitializeEx failed: {:?}", hr)));
                }
                let result = run_op(&paths);
                CoUninitialize();
                result
            }
        });
        handle
            .join()
            .map_err(|_| AppError::Other("trash thread panicked".into()))?
    }

    unsafe fn run_op(paths: &[String]) -> AppResult<()> {
        let op: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL)
            .map_err(|e| AppError::Other(format!("CoCreateInstance failed: {e}")))?;

        let flags = FILEOPERATION_FLAGS(
            (FOF_NOCONFIRMATION.0
                | FOF_NOERRORUI.0
                | FOF_SILENT.0
                | FOFX_RECYCLEONDELETE.0 as u32
                | FOFX_ADDUNDORECORD.0 as u32) as u32,
        );
        op.SetOperationFlags(flags)
            .map_err(|e| AppError::Other(format!("SetOperationFlags failed: {e}")))?;
        op.SetOwnerWindow(HWND::default()).ok();

        for p in paths {
            let wide: Vec<u16> = p.encode_utf16().chain(std::iter::once(0)).collect();
            let item: IShellItem =
                SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)
                    .map_err(|e| AppError::Other(format!("SHCreateItemFromParsingName({p}): {e}")))?;
            op.DeleteItem(&item, None)
                .map_err(|e| AppError::Other(format!("DeleteItem({p}): {e}")))?;
        }

        op.PerformOperations()
            .map_err(|e| AppError::Other(format!("PerformOperations: {e}")))?;

        let aborted = op.GetAnyOperationsAborted().unwrap_or_default();
        if aborted.as_bool() {
            return Err(AppError::Other("operation aborted".into()));
        }
        Ok(())
    }
}

