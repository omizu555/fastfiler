// Phase 1: FsService — 実 FS 列挙・stat・ドライブ列挙
//
// Tauri Command で呼び出される。シンボリックリンクや権限拒否は
// エントリ単位でスキップして列挙の堅牢性を保つ。

use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Serialize, Clone)]
pub struct FileEntry {
    pub name: String,
    pub kind: &'static str, // "dir" | "file" | "symlink"
    pub size: u64,
    pub modified: i64, // unix seconds
    pub ext: Option<String>,
    pub hidden: bool,
    pub readonly: bool,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DriveInfo {
    pub letter: String, // "C:\"
    pub label: String,  // ボリュームラベル ("" の場合あり)
    pub kind: String,   // "fixed" | "removable" | "network" | "cdrom" | "ram" | "unknown"
    pub remote_path: Option<String>, // network の場合 "\\server\share"
}

fn to_unix_secs(t: SystemTime) -> i64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(windows)]
fn is_hidden(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    (meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN) != 0
}
#[cfg(not(windows))]
fn is_hidden(_meta: &fs::Metadata) -> bool { false }

#[tauri::command]
pub fn list_dir(path: String) -> AppResult<Vec<FileEntry>> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(AppError::NotFound(path));
    }
    let read = fs::read_dir(&p)?;
    let mut out = Vec::with_capacity(64);
    for ent in read.flatten() {
        let Ok(meta) = ent.metadata() else { continue };
        let name = ent.file_name().to_string_lossy().to_string();
        let kind = if meta.is_dir() {
            "dir"
        } else if meta.file_type().is_symlink() {
            "symlink"
        } else {
            "file"
        };
        let modified = meta.modified().map(to_unix_secs).unwrap_or(0);
        let ext = if kind == "file" {
            Path::new(&name)
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_lowercase())
        } else {
            None
        };
        out.push(FileEntry {
            name,
            kind,
            size: if meta.is_file() { meta.len() } else { 0 },
            modified,
            ext,
            hidden: is_hidden(&meta),
            readonly: meta.permissions().readonly(),
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn stat_path(path: String) -> AppResult<FileEntry> {
    let p = PathBuf::from(&path);
    let meta = fs::metadata(&p)?;
    let name = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());
    let kind = if meta.is_dir() { "dir" } else { "file" };
    Ok(FileEntry {
        name,
        kind,
        size: if meta.is_file() { meta.len() } else { 0 },
        modified: meta.modified().map(to_unix_secs).unwrap_or(0),
        ext: p.extension().and_then(|s| s.to_str()).map(|s| s.to_lowercase()),
        hidden: is_hidden(&meta),
        readonly: meta.permissions().readonly(),
    })
}

#[tauri::command]
pub fn list_dirs(path: String, include_hidden: Option<bool>) -> AppResult<Vec<FileEntry>> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(AppError::NotFound(path));
    }
    let read = fs::read_dir(&p)?;
    let inc_hidden = include_hidden.unwrap_or(true);
    let mut out = Vec::with_capacity(64);
    for ent in read.flatten() {
        let Ok(meta) = ent.metadata() else { continue };
        if !meta.is_dir() {
            continue;
        }
        let hidden = is_hidden(&meta);
        if hidden && !inc_hidden {
            continue;
        }
        let name = ent.file_name().to_string_lossy().to_string();
        out.push(FileEntry {
            name,
            kind: "dir",
            size: 0,
            modified: meta.modified().map(to_unix_secs).unwrap_or(0),
            ext: None,
            hidden,
            readonly: meta.permissions().readonly(),
        });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

#[tauri::command]
pub fn home_dir() -> AppResult<String> {
    let h = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| AppError::Other("home not found".into()))?;
    Ok(h)
}

#[tauri::command]
pub fn list_drives() -> AppResult<Vec<DriveInfo>> {
    #[cfg(windows)]
    {
        use windows::core::PWSTR;
        use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS, MAX_PATH};
        use windows::Win32::NetworkManagement::WNet::WNetGetConnectionW;
        use windows::Win32::Storage::FileSystem::{
            GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
        };

        // GetDriveTypeW returns plain u32 codes
        const DRIVE_REMOVABLE: u32 = 2;
        const DRIVE_FIXED: u32 = 3;
        const DRIVE_REMOTE: u32 = 4;
        const DRIVE_CDROM: u32 = 5;
        const DRIVE_RAMDISK: u32 = 6;

        fn to_wide_z(s: &str) -> Vec<u16> {
            s.encode_utf16().chain(std::iter::once(0)).collect()
        }
        fn from_wide(buf: &[u16]) -> String {
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            String::from_utf16_lossy(&buf[..len])
        }

        let mask = unsafe { GetLogicalDrives() };
        let mut drives = Vec::new();
        for i in 0..26u32 {
            if (mask & (1 << i)) == 0 {
                continue;
            }
            let letter = format!("{}:\\", (b'A' + i as u8) as char);
            let wide = to_wide_z(&letter);

            let dt = unsafe { GetDriveTypeW(windows::core::PCWSTR(wide.as_ptr())) };
            let kind = match dt {
                DRIVE_FIXED => "fixed",
                DRIVE_REMOVABLE => "removable",
                DRIVE_REMOTE => "network",
                DRIVE_CDROM => "cdrom",
                DRIVE_RAMDISK => "ram",
                _ => "unknown",
            };

            let mut name_buf = [0u16; (MAX_PATH + 1) as usize];
            let label = unsafe {
                if GetVolumeInformationW(
                    windows::core::PCWSTR(wide.as_ptr()),
                    Some(&mut name_buf),
                    None,
                    None,
                    None,
                    None,
                )
                .is_ok()
                {
                    from_wide(&name_buf)
                } else {
                    String::new()
                }
            };

            let remote_path = if kind == "network" {
                let local = format!("{}:", (b'A' + i as u8) as char);
                let local_w = to_wide_z(&local);
                let mut remote_buf = vec![0u16; 1024];
                let mut size: u32 = remote_buf.len() as u32;
                let r = unsafe {
                    WNetGetConnectionW(
                        windows::core::PCWSTR(local_w.as_ptr()),
                        PWSTR(remote_buf.as_mut_ptr()),
                        &mut size,
                    )
                };
                if r == ERROR_SUCCESS {
                    Some(from_wide(&remote_buf))
                } else if r == ERROR_MORE_DATA {
                    remote_buf.resize(size as usize, 0);
                    let r2 = unsafe {
                        WNetGetConnectionW(
                            windows::core::PCWSTR(local_w.as_ptr()),
                            PWSTR(remote_buf.as_mut_ptr()),
                            &mut size,
                        )
                    };
                    if r2 == ERROR_SUCCESS {
                        Some(from_wide(&remote_buf))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            drives.push(DriveInfo {
                letter,
                label,
                kind: kind.to_string(),
                remote_path,
            });
        }
        Ok(drives)
    }
    #[cfg(not(windows))]
    {
        Ok(vec![DriveInfo {
            letter: "/".into(),
            label: "/".into(),
            kind: "fixed".into(),
            remote_path: None,
        }])
    }
}
