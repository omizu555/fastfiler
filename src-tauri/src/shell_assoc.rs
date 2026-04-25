// v1.12: シェル統合 — Folder/Directory のオープン ハンドラを fastfiler.exe に差し替え
//
// HKCU スコープなので管理者権限不要。
// register/unregister はトグル可能。状態取得は (default) 値が現 exe を指しているかで判定。
//
// 元の Windows 既定値は HKCR (= HKLM の Folder) に存在する。
// HKCU に値を書くと HKCR より優先されるため、HKCU 側のキーを削除すれば既定が復帰する。

#![cfg_attr(not(windows), allow(unused))]

use std::path::PathBuf;
#[cfg(windows)]
use winreg::enums::*;
#[cfg(windows)]
use winreg::RegKey;

#[cfg(windows)]
const KEYS: &[&str] = &[
    r"Software\Classes\Folder\shell\open\command",
    r"Software\Classes\Directory\shell\open\command",
];

fn current_exe_path() -> Result<String, String> {
    let p: PathBuf = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(p.to_string_lossy().into_owned())
}

fn build_command_value(exe: &str) -> String {
    format!("\"{}\" \"%1\"", exe)
}

#[cfg(windows)]
#[tauri::command]
pub fn shell_assoc_status() -> Result<bool, String> {
    let exe = current_exe_path()?;
    let expected = build_command_value(&exe);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for key in KEYS {
        let Ok(k) = hkcu.open_subkey(key) else { return Ok(false); };
        let v: Result<String, _> = k.get_value("");
        match v {
            Ok(s) if s.eq_ignore_ascii_case(&expected) => continue,
            _ => return Ok(false),
        }
    }
    Ok(true)
}

#[cfg(not(windows))]
#[tauri::command]
pub fn shell_assoc_status() -> Result<bool, String> { Ok(false) }

#[cfg(windows)]
#[tauri::command]
pub fn shell_assoc_enable() -> Result<(), String> {
    let exe = current_exe_path()?;
    let cmd = build_command_value(&exe);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for key in KEYS {
        let (k, _) = hkcu
            .create_subkey(key)
            .map_err(|e| format!("create_subkey({key}): {e}"))?;
        k.set_value("", &cmd)
            .map_err(|e| format!("set_value({key}): {e}"))?;
    }
    Ok(())
}

#[cfg(not(windows))]
#[tauri::command]
pub fn shell_assoc_enable() -> Result<(), String> {
    Err("Windows only".into())
}

#[cfg(windows)]
#[tauri::command]
pub fn shell_assoc_disable() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for key in KEYS {
        let _ = hkcu.delete_subkey_all(key);
        let parents: &[&str] = match *key {
            r"Software\Classes\Folder\shell\open\command" => &[
                r"Software\Classes\Folder\shell\open",
                r"Software\Classes\Folder\shell",
                r"Software\Classes\Folder",
            ],
            r"Software\Classes\Directory\shell\open\command" => &[
                r"Software\Classes\Directory\shell\open",
                r"Software\Classes\Directory\shell",
                r"Software\Classes\Directory",
            ],
            _ => &[],
        };
        for p in parents {
            let _ = hkcu.delete_subkey(p);
        }
    }
    Ok(())
}

#[cfg(not(windows))]
#[tauri::command]
pub fn shell_assoc_disable() -> Result<(), String> {
    Err("Windows only".into())
}
