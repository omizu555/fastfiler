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

fn current_exe_path() -> Result<String, String> {
    let p: PathBuf = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(p.to_string_lossy().into_owned())
}

fn build_command_value(exe: &str) -> String {
    format!("\"{}\" \"%1\"", exe)
}

#[cfg(windows)]
#[tauri::command]
pub fn shell_assoc_enable() -> Result<(), String> {
    let exe = current_exe_path()?;
    let cmd = build_command_value(&exe);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // 各 ProgID (Folder / Directory) について:
    //   shell\open                       (default)         = "開く(&O)"   (任意, MUIVerb)
    //   shell\open                       DelegateExecute   = ""          (HKCR の CLSID ハンドラを無効化)
    //   shell\open\command               (default)         = "<exe>" "%1"
    //   shell\open\ddeexec               (default)         = ""          (HKCR の DDE を無効化)
    //
    // DelegateExecute / ddeexec を空文字で上書きしないと、HKCR 側の既定ハンドラが優先され続けるため
    // フォルダ ダブルクリック / Excel リンクが Explorer に行ってしまう。
    let progids = ["Folder", "Directory"];
    for pid in progids {
        // shell\open
        let open_path = format!(r"Software\Classes\{pid}\shell\open");
        let (open_key, _) = hkcu
            .create_subkey(&open_path)
            .map_err(|e| format!("create_subkey({open_path}): {e}"))?;
        // MUIVerb は任意 (なくても可)
        let _ = open_key.set_value("MUIVerb", &"開く(&O)".to_string());
        // HKCR の DelegateExecute を打ち消す (空文字オーバーライド)
        open_key
            .set_value("DelegateExecute", &String::new())
            .map_err(|e| format!("set DelegateExecute({pid}): {e}"))?;

        // shell\open\command
        let cmd_path = format!(r"Software\Classes\{pid}\shell\open\command");
        let (cmd_key, _) = hkcu
            .create_subkey(&cmd_path)
            .map_err(|e| format!("create_subkey({cmd_path}): {e}"))?;
        cmd_key
            .set_value("", &cmd)
            .map_err(|e| format!("set_value({cmd_path}): {e}"))?;
        // 念のため DelegateExecute もクリア
        let _ = cmd_key.set_value("DelegateExecute", &String::new());

        // shell\open\ddeexec — 空文字で DDE 起動を抑制
        let dde_path = format!(r"Software\Classes\{pid}\shell\open\ddeexec");
        let (dde_key, _) = hkcu
            .create_subkey(&dde_path)
            .map_err(|e| format!("create_subkey({dde_path}): {e}"))?;
        dde_key
            .set_value("", &String::new())
            .map_err(|e| format!("set ddeexec({pid}): {e}"))?;
    }
    Ok(())
}

#[cfg(windows)]
#[tauri::command]
pub fn shell_assoc_status() -> Result<bool, String> {
    let exe = current_exe_path()?;
    let expected = build_command_value(&exe);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for pid in ["Folder", "Directory"] {
        let cmd_path = format!(r"Software\Classes\{pid}\shell\open\command");
        let Ok(k) = hkcu.open_subkey(&cmd_path) else { return Ok(false); };
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
pub fn shell_assoc_enable() -> Result<(), String> {
    Err("Windows only".into())
}

#[cfg(not(windows))]
#[tauri::command]
pub fn shell_assoc_status() -> Result<bool, String> { Ok(false) }

#[cfg(not(windows))]
#[tauri::command]
pub fn shell_assoc_enable() -> Result<(), String> {
    Err("Windows only".into())
}

#[cfg(windows)]
#[tauri::command]
pub fn shell_assoc_disable() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    // Folder / Directory それぞれの shell\open ツリーを根こそぎ削除
    for pid in ["Folder", "Directory"] {
        let open_path = format!(r"Software\Classes\{pid}\shell\open");
        let _ = hkcu.delete_subkey_all(&open_path);
        // 親が空であれば段階的に削除 (空でなければ失敗するが他キーは温存)
        let parents = [
            format!(r"Software\Classes\{pid}\shell"),
            format!(r"Software\Classes\{pid}"),
        ];
        for p in &parents {
            let _ = hkcu.delete_subkey(p);
        }
    }
    Ok(())
}

#[cfg(windows)]
#[tauri::command]
pub fn shell_assoc_diagnose() -> Result<String, String> {
    let exe = current_exe_path()?;
    let expected = build_command_value(&exe);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let mut out = String::new();
    out.push_str(&format!("現在の exe: {exe}\n期待値    : {expected}\n\n"));
    for pid in ["Folder", "Directory"] {
        out.push_str(&format!("[HKCU\\Software\\Classes\\{pid}]\n"));
        for sub in ["shell\\open", "shell\\open\\command", "shell\\open\\ddeexec"] {
            let p = format!(r"Software\Classes\{pid}\{sub}");
            match hkcu.open_subkey(&p) {
                Ok(k) => {
                    let def: Result<String, _> = k.get_value("");
                    let de: Result<String, _> = k.get_value("DelegateExecute");
                    out.push_str(&format!("  {sub} (default)='{}' DelegateExecute='{}'\n",
                        def.unwrap_or_else(|_| "<none>".into()),
                        de.unwrap_or_else(|_| "<none>".into()),
                    ));
                }
                Err(_) => out.push_str(&format!("  {sub}: <キー無し>\n")),
            }
        }
        out.push('\n');
    }
    Ok(out)
}

#[cfg(not(windows))]
#[tauri::command]
pub fn shell_assoc_diagnose() -> Result<String, String> { Ok(String::new()) }
