// Windows シェル API (SHGetFileInfoW) を使ってエクスプローラと同じアイコンを
// PNG バイト列で取得する。
//
// - large=true で 32x32 / false で 16x16
// - ext_only=true なら実ファイル不要 (拡張子だけからアイコン解決, FILE_ATTRIBUTE_NORMAL)
// - 結果は LRU(1024) でキャッシュ。ext_only モードでは拡張子をキーに共通化する。

use crate::error::{AppError, AppResult};
use std::path::Path;
use std::sync::Mutex;

use once_cell::sync::Lazy;

#[cfg(windows)]
mod sys {
    use super::*;
    use lru::LruCache;
    use std::ffi::OsStr;
    use std::num::NonZeroUsize;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS, HBITMAP,
    };
    use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
    use windows::Win32::UI::Shell::{
        SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_SMALLICON,
        SHGFI_USEFILEATTRIBUTES,
    };
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

    static CACHE: Lazy<Mutex<LruCache<String, std::sync::Arc<Vec<u8>>>>> =
        Lazy::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(1024).unwrap())));

    fn cache_key(path: &str, ext_only: bool, large: bool) -> String {
        let mut k = String::new();
        if ext_only {
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

    pub fn system_icon_png(
        path: &str,
        large: bool,
        ext_only: bool,
    ) -> AppResult<std::sync::Arc<Vec<u8>>> {
        do_get(path, large, ext_only, false)
    }

    pub fn folder_icon_png(large: bool) -> AppResult<std::sync::Arc<Vec<u8>>> {
        do_get("__dir__", large, true, true)
    }

    fn do_get(
        path: &str,
        large: bool,
        ext_only: bool,
        is_dir: bool,
    ) -> AppResult<std::sync::Arc<Vec<u8>>> {
        let key = {
            let mut k = cache_key(path, ext_only, large);
            if is_dir { k.push('d'); }
            k
        };
        if let Some(v) = CACHE.lock().unwrap().get(&key) {
            return Ok(v.clone());
        }
        unsafe {
            let mut info: SHFILEINFOW = std::mem::zeroed();
            let wpath = to_wide(path);
            let mut flags = SHGFI_ICON | if large { SHGFI_LARGEICON } else { SHGFI_SMALLICON };
            let attrs = if ext_only {
                flags |= SHGFI_USEFILEATTRIBUTES;
                if is_dir {
                    windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY.0
                } else {
                    FILE_ATTRIBUTE_NORMAL.0
                }
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
            let png = hicon_to_png(info.hIcon)?;
            let _ = DestroyIcon(info.hIcon);
            let arc = std::sync::Arc::new(png);
            CACHE.lock().unwrap().put(key, arc.clone());
            Ok(arc)
        }
    }

    unsafe fn hicon_to_png(hicon: HICON) -> AppResult<Vec<u8>> {
        let mut iconinfo: ICONINFO = std::mem::zeroed();
        if GetIconInfo(hicon, &mut iconinfo as *mut _).is_err() {
            return Err(AppError::Other("GetIconInfo failed".into()));
        }
        let color_bm: HBITMAP = iconinfo.hbmColor;
        let mask_bm: HBITMAP = iconinfo.hbmMask;
        let result = (|| -> AppResult<Vec<u8>> {
            let hdc = GetDC(HWND(std::ptr::null_mut()));
            if hdc.is_invalid() {
                return Err(AppError::Other("GetDC failed".into()));
            }
            let _guard = ScopeGuard::new(|| {
                ReleaseDC(HWND(std::ptr::null_mut()), hdc);
            });

            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            let r1 = GetDIBits(hdc, color_bm, 0, 0, None, &mut bmi, DIB_RGB_COLORS);
            if r1 == 0 {
                return Err(AppError::Other("GetDIBits(query) failed".into()));
            }
            let w = bmi.bmiHeader.biWidth.unsigned_abs();
            let h = bmi.bmiHeader.biHeight.unsigned_abs();
            if w == 0 || h == 0 {
                return Err(AppError::Other("zero-size icon".into()));
            }
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

            let mut all_alpha_zero = true;
            for px in color_buf.chunks_exact(4) {
                if px[3] != 0 {
                    all_alpha_zero = false;
                    break;
                }
            }
            if all_alpha_zero && !mask_bm.is_invalid() {
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
                        let m = mask_buf[off];
                        color_buf[off + 3] = if m == 0 { 255 } else { 0 };
                    }
                }
            }
            // BGRA → RGBA
            for px in color_buf.chunks_exact_mut(4) {
                px.swap(0, 2);
            }

            let mut png_bytes: Vec<u8> = Vec::with_capacity(stride * h as usize / 2);
            {
                let img = image::RgbaImage::from_raw(w, h, color_buf)
                    .ok_or_else(|| AppError::Other("RgbaImage::from_raw failed".into()))?;
                let mut cursor = std::io::Cursor::new(&mut png_bytes);
                img.write_to(&mut cursor, image::ImageFormat::Png)
                    .map_err(|e| AppError::Other(format!("png encode: {}", e)))?;
            }
            Ok(png_bytes)
        })();

        if !color_bm.is_invalid() {
            let _ = DeleteObject(color_bm);
        }
        if !mask_bm.is_invalid() {
            let _ = DeleteObject(mask_bm);
        }
        result
    }

    struct ScopeGuard<F: FnMut()> {
        f: F,
    }
    impl<F: FnMut()> ScopeGuard<F> {
        fn new(f: F) -> Self {
            Self { f }
        }
    }
    impl<F: FnMut()> Drop for ScopeGuard<F> {
        fn drop(&mut self) {
            (self.f)();
        }
    }
}

#[cfg(windows)]
pub fn system_icon_png(
    path: &str,
    large: bool,
    ext_only: bool,
) -> AppResult<std::sync::Arc<Vec<u8>>> {
    sys::system_icon_png(path, large, ext_only)
}

#[cfg(not(windows))]
pub fn system_icon_png(
    _path: &str,
    _large: bool,
    _ext_only: bool,
) -> AppResult<std::sync::Arc<Vec<u8>>> {
    Err(AppError::Other("system_icon is Windows-only".into()))
}

#[cfg(windows)]
pub fn folder_icon_png(large: bool) -> AppResult<std::sync::Arc<Vec<u8>>> {
    sys::folder_icon_png(large)
}

#[cfg(not(windows))]
pub fn folder_icon_png(_large: bool) -> AppResult<std::sync::Arc<Vec<u8>>> {
    Err(AppError::Other("folder_icon is Windows-only".into()))
}
