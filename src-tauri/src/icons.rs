// v1.14: アイコン パック (system + custom)
//
// system パック:
//   Windows シェル API (SHGetFileInfoW) でエクスプローラと同じアイコンを
//   取得し、PNG/base64 dataURL でフロントへ返す。
//   - ext_only=true なら拡張子だけから取得 (実ファイル不要、結果は拡張子で
//     キャッシュ)
//   - 大量列挙でも LRU 1024 件で抑える
//
// custom パック:
//   %APPDATA%\fastfiler\icons\<pack>\manifest.json + SVG ファイル群を読み込み、
//   フロントが byName / byFolderName / byExt / defaults の順に解決する。
//   read_icon_file は SVG (or PNG/ICO) を base64 dataURL で返す。

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::Engine;
use once_cell::sync::Lazy;

// ---------------- icons dir ----------------

fn icons_dir_inner() -> AppResult<PathBuf> {
    let appdata = std::env::var("APPDATA")
        .map_err(|_| AppError::Other("APPDATA not set".into()))?;
    let dir = PathBuf::from(appdata).join("fastfiler").join("icons");
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

#[tauri::command]
pub fn icons_dir() -> AppResult<String> {
    let p = icons_dir_inner()?;
    Ok(p.to_string_lossy().into_owned())
}

// ---------------- custom packs ----------------

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct IconDefaults {
    #[serde(default)]
    pub folder: Option<String>,
    #[serde(default)]
    pub folder_open: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub drive: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct IconManifest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub defaults: IconDefaults,
    #[serde(default, rename = "byExt")]
    pub by_ext: HashMap<String, String>,
    #[serde(default, rename = "byName")]
    pub by_name: HashMap<String, String>,
    #[serde(default, rename = "byFolderName")]
    pub by_folder_name: HashMap<String, String>,
}

#[derive(Serialize, Clone)]
pub struct IconPackInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub manifest: IconManifest,
}

#[tauri::command]
pub fn list_icon_packs() -> AppResult<Vec<IconPackInfo>> {
    let dir = icons_dir_inner()?;
    let mut out = Vec::new();
    let rd = match fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(_) => return Ok(out),
    };
    for ent in rd.flatten() {
        let path = ent.path();
        if !path.is_dir() {
            continue;
        }
        let id = ent.file_name().to_string_lossy().into_owned();
        let manifest_path = path.join("manifest.json");
        if !manifest_path.exists() {
            continue;
        }
        let txt = match fs::read_to_string(&manifest_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let manifest: IconManifest = match serde_json::from_str(&txt) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[icons] manifest parse error in {}: {}", id, e);
                continue;
            }
        };
        let name = manifest.name.clone().unwrap_or_else(|| id.clone());
        let version = manifest.version.clone().unwrap_or_else(|| "0".into());
        out.push(IconPackInfo { id, name, version, manifest });
    }
    Ok(out)
}

#[tauri::command]
pub fn read_icon_file(pack: String, rel: String) -> AppResult<String> {
    if pack.contains("..") || pack.contains('/') || pack.contains('\\') {
        return Err(AppError::InvalidPath(format!("invalid pack id: {}", pack)));
    }
    if rel.contains("..") {
        return Err(AppError::InvalidPath(format!("invalid rel: {}", rel)));
    }
    let dir = icons_dir_inner()?.join(&pack);
    let file = dir.join(rel.replace('\\', "/"));
    let canon_dir = dir.canonicalize().map_err(|_| AppError::NotFound(pack.clone()))?;
    let canon_file = file.canonicalize().map_err(|_| AppError::NotFound(rel.clone()))?;
    if !canon_file.starts_with(&canon_dir) {
        return Err(AppError::InvalidPath("escape".into()));
    }
    let bytes = fs::read(&canon_file)?;
    let mime = guess_mime(&canon_file);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{};base64,{}", mime, b64))
}

fn guess_mime(p: &Path) -> &'static str {
    match p.extension().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

// ---------------- bundled material pack auto-extract ----------------

/// 同梱 Material アイコン パックを APPDATA に展開する (なければ作成)。
/// build.rs で ICONS_BUNDLE 配列に (相対パス, バイト列) が埋め込まれる。
pub fn ensure_bundled_packs() -> AppResult<()> {
    let dir = icons_dir_inner()?;
    let target = dir.join("material");
    let manifest_path = target.join("manifest.json");
    if manifest_path.exists() {
        return Ok(());
    }
    fs::create_dir_all(&target)?;
    for (rel, bytes) in MATERIAL_BUNDLE {
        let p = target.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&p, *bytes)?;
    }
    Ok(())
}

include!(concat!(env!("OUT_DIR"), "/icons_bundle.rs"));

// ---------------- system icons (Windows Shell) ----------------

#[cfg(windows)]
mod sys {
    use super::*;
    use std::ffi::OsStr;
    use std::num::NonZeroUsize;
    use std::os::windows::ffi::OsStrExt;
    use lru::LruCache;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS, HBITMAP,
    };
    use windows::Win32::UI::Shell::{
        SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_SMALLICON,
        SHGFI_USEFILEATTRIBUTES,
    };
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
    use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;

    static CACHE: Lazy<Mutex<LruCache<String, String>>> = Lazy::new(|| {
        Mutex::new(LruCache::new(NonZeroUsize::new(1024).unwrap()))
    });

    fn cache_key(path: &str, ext_only: bool, large: bool) -> String {
        let mut k = String::new();
        if ext_only {
            // 拡張子のみで共通化
            let ext = Path::new(path)
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            k.push_str("ext:");
            k.push_str(&ext);
        } else {
            k.push_str("path:");
            k.push_str(&path.to_ascii_lowercase());
        }
        k.push(if large { '+' } else { '-' });
        k
    }

    fn to_wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    pub fn system_icon(path: &str, large: bool, ext_only: bool) -> AppResult<String> {
        let key = cache_key(path, ext_only, large);
        if let Some(v) = CACHE.lock().unwrap().get(&key) {
            return Ok(v.clone());
        }
        unsafe {
            let mut info: SHFILEINFOW = std::mem::zeroed();
            let wpath = to_wide(path);
            let mut flags = SHGFI_ICON | if large { SHGFI_LARGEICON } else { SHGFI_SMALLICON };
            let attrs = if ext_only {
                flags |= SHGFI_USEFILEATTRIBUTES;
                FILE_ATTRIBUTE_NORMAL.0
            } else {
                0
            };
            let r = SHGetFileInfoW(
                PCWSTR(wpath.as_ptr()),
                windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(attrs),
                Some(&mut info as *mut _),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                flags,
            );
            if r == 0 || info.hIcon.0.is_null() {
                return Err(AppError::Other(format!("SHGetFileInfoW failed for {}", path)));
            }
            let dataurl = hicon_to_dataurl(info.hIcon)?;
            let _ = DestroyIcon(info.hIcon);
            CACHE.lock().unwrap().put(key, dataurl.clone());
            Ok(dataurl)
        }
    }

    unsafe fn hicon_to_dataurl(hicon: HICON) -> AppResult<String> {
        let mut iconinfo: ICONINFO = std::mem::zeroed();
        if GetIconInfo(hicon, &mut iconinfo as *mut _).is_err() {
            return Err(AppError::Other("GetIconInfo failed".into()));
        }
        let color_bm: HBITMAP = iconinfo.hbmColor;
        let mask_bm: HBITMAP = iconinfo.hbmMask;
        let result = (|| -> AppResult<String> {
            let hdc = GetDC(HWND(std::ptr::null_mut()));
            if hdc.is_invalid() {
                return Err(AppError::Other("GetDC failed".into()));
            }
            let _guard = ScopeGuard::new(|| { ReleaseDC(HWND(std::ptr::null_mut()), hdc); });

            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            // 1 度目: biWidth/biHeight を取得するために bits=NULL で呼ぶ
            let r1 = GetDIBits(hdc, color_bm, 0, 0, None, &mut bmi, DIB_RGB_COLORS);
            if r1 == 0 {
                return Err(AppError::Other("GetDIBits(query) failed".into()));
            }
            let w = bmi.bmiHeader.biWidth.unsigned_abs();
            let h = bmi.bmiHeader.biHeight.unsigned_abs();
            if w == 0 || h == 0 {
                return Err(AppError::Other("zero-size icon".into()));
            }
            // 32bpp top-down で取り直す
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB.0 as u32;
            bmi.bmiHeader.biHeight = -(h as i32);
            let stride = (w * 4) as usize;
            let mut color_buf = vec![0u8; stride * h as usize];
            let r2 = GetDIBits(
                hdc,
                color_bm,
                0,
                h,
                Some(color_buf.as_mut_ptr() as *mut _),
                &mut bmi,
                DIB_RGB_COLORS,
            );
            if r2 == 0 {
                return Err(AppError::Other("GetDIBits(color) failed".into()));
            }

            // BGRA → RGBA, アルファが全 0 のときはマスクで構築
            let mut all_alpha_zero = true;
            for px in color_buf.chunks_exact(4) {
                if px[3] != 0 { all_alpha_zero = false; break; }
            }
            if all_alpha_zero && !mask_bm.is_invalid() {
                // マスクは 1bpp (白=透明、黒=不透明)
                let mut mbmi: BITMAPINFO = std::mem::zeroed();
                mbmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                mbmi.bmiHeader.biPlanes = 1;
                mbmi.bmiHeader.biBitCount = 32;
                mbmi.bmiHeader.biCompression = BI_RGB.0 as u32;
                mbmi.bmiHeader.biWidth = w as i32;
                mbmi.bmiHeader.biHeight = -(h as i32);
                let mut mask_buf = vec![0u8; stride * h as usize];
                let mr = GetDIBits(
                    hdc,
                    mask_bm,
                    0,
                    h,
                    Some(mask_buf.as_mut_ptr() as *mut _),
                    &mut mbmi,
                    DIB_RGB_COLORS,
                );
                if mr != 0 {
                    for i in 0..(w * h) as usize {
                        let off = i * 4;
                        // mask: white => transparent, black => opaque
                        let m = mask_buf[off];
                        color_buf[off + 3] = if m == 0 { 255 } else { 0 };
                    }
                }
            }
            // BGRA → RGBA 変換
            for px in color_buf.chunks_exact_mut(4) {
                px.swap(0, 2);
            }

            // PNG エンコード
            let mut png_bytes: Vec<u8> = Vec::with_capacity(stride * h as usize / 2);
            {
                let img = image::RgbaImage::from_raw(w, h, color_buf)
                    .ok_or_else(|| AppError::Other("RgbaImage::from_raw failed".into()))?;
                let mut cursor = std::io::Cursor::new(&mut png_bytes);
                img.write_to(&mut cursor, image::ImageFormat::Png)
                    .map_err(|e| AppError::Other(format!("png encode: {}", e)))?;
            }
            let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
            Ok(format!("data:image/png;base64,{}", b64))
        })();

        if !color_bm.is_invalid() {
            let _ = DeleteObject(color_bm);
        }
        if !mask_bm.is_invalid() {
            let _ = DeleteObject(mask_bm);
        }
        result
    }

    struct ScopeGuard<F: FnMut()> { f: F }
    impl<F: FnMut()> ScopeGuard<F> {
        fn new(f: F) -> Self { Self { f } }
    }
    impl<F: FnMut()> Drop for ScopeGuard<F> {
        fn drop(&mut self) { (self.f)(); }
    }
}

#[cfg(windows)]
#[tauri::command]
pub fn system_icon(path: String, large: Option<bool>, ext_only: Option<bool>) -> AppResult<String> {
    sys::system_icon(&path, large.unwrap_or(false), ext_only.unwrap_or(false))
}

#[cfg(not(windows))]
#[tauri::command]
pub fn system_icon(_path: String, _large: Option<bool>, _ext_only: Option<bool>) -> AppResult<String> {
    Err(AppError::NotSupported("system_icon is Windows-only".into()))
}
