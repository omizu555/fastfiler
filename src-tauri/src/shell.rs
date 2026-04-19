// Phase 3: シェル統合
//
// - open_with_shell: ShellExecuteW("open", path) — 既定アプリで開く
// - reveal_in_explorer: explorer.exe /select で「エクスプローラで表示」
// - show_properties: SHObjectProperties でプロパティダイアログ
//
// 将来的に IContextMenu のフルサポートを追加予定。

use crate::error::{AppError, AppResult};

#[tauri::command]
pub fn open_with_shell(path: String) -> AppResult<()> {
    #[cfg(windows)]
    {
        win::shell_exec("open", &path, None)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(AppError::NotSupported("shell open is windows-only".into()))
    }
}

#[tauri::command]
pub fn reveal_in_explorer(path: String) -> AppResult<()> {
    #[cfg(windows)]
    {
        // explorer.exe /select,"path" でファイルを選択状態でフォルダを開く
        let arg = format!("/select,\"{}\"", path);
        win::shell_exec("open", "explorer.exe", Some(&arg))
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(AppError::NotSupported("reveal is windows-only".into()))
    }
}

#[tauri::command]
pub fn show_properties(path: String) -> AppResult<()> {
    #[cfg(windows)]
    {
        win::show_properties(&path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(AppError::NotSupported("properties is windows-only".into()))
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::thread;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::UI::Shell::{
        SHObjectProperties, ShellExecuteW, SHOP_FILEPATH,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    pub fn shell_exec(op: &str, file: &str, args: Option<&str>) -> AppResult<()> {
        let op_w = wide(op);
        let file_w = wide(file);
        let args_w = args.map(wide);
        let hinst = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(op_w.as_ptr()),
                PCWSTR(file_w.as_ptr()),
                args_w
                    .as_ref()
                    .map(|w| PCWSTR(w.as_ptr()))
                    .unwrap_or(PCWSTR::null()),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        if hinst.0 as isize <= 32 {
            return Err(AppError::Other(format!("ShellExecuteW failed ({})", hinst.0 as isize)));
        }
        Ok(())
    }

    pub fn show_properties(path: &str) -> AppResult<()> {
        let path = path.to_owned();
        let handle = thread::spawn(move || -> AppResult<()> {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
                let path_w = wide(&path);
                let res = SHObjectProperties(
                    HWND::default(),
                    SHOP_FILEPATH,
                    PCWSTR(path_w.as_ptr()),
                    PCWSTR::null(),
                );
                CoUninitialize();
                if res.as_bool() {
                    Ok(())
                } else {
                    Err(AppError::Other("SHObjectProperties returned FALSE".into()))
                }
            }
        });
        handle
            .join()
            .map_err(|_| AppError::Other("properties thread panicked".into()))?
    }
}

