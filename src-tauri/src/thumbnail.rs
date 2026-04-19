// Phase 4: サムネイル
// Windows IShellItemImageFactory 経由でサムネイルを取得し PNG として返す。
// 読み出し結果は in-memory LRU + ディスクキャッシュ (%LOCALAPPDATA%\fastfiler\thumbs)。

use crate::error::{AppError, AppResult};
use base64::Engine;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(serde::Serialize)]
pub struct ThumbnailResult {
    pub data_url: String,
    pub width: u32,
    pub height: u32,
}

const MAX_LRU: usize = 256;

struct LruCache {
    map: HashMap<String, ThumbnailResult>,
    order: Vec<String>,
}

impl LruCache {
    fn new() -> Self { Self { map: HashMap::new(), order: Vec::new() } }
    fn get(&mut self, k: &str) -> Option<&ThumbnailResult> {
        if self.map.contains_key(k) {
            self.order.retain(|s| s != k);
            self.order.push(k.to_owned());
            return self.map.get(k);
        }
        None
    }
    fn put(&mut self, k: String, v: ThumbnailResult) {
        if self.map.contains_key(&k) {
            self.order.retain(|s| s != &k);
        } else if self.map.len() >= MAX_LRU {
            if let Some(old) = self.order.first().cloned() {
                self.order.remove(0);
                self.map.remove(&old);
            }
        }
        self.order.push(k.clone());
        self.map.insert(k, v);
    }
}

static CACHE: Lazy<Mutex<LruCache>> = Lazy::new(|| Mutex::new(LruCache::new()));

fn cache_key(path: &str, size: u32, mtime: i64) -> String {
    format!("{}::{}::{}", path, size, mtime)
}

fn disk_cache_dir() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    let mut p = PathBuf::from(local);
    p.push("fastfiler");
    p.push("thumbs");
    let _ = std::fs::create_dir_all(&p);
    Some(p)
}

fn disk_cache_path(key: &str) -> Option<PathBuf> {
    let dir = disk_cache_dir()?;
    let safe = format!("{:x}.png", md5_like(key));
    Some(dir.join(safe))
}

// シンプルな fnv-1a 64bit (依存追加を避けるため)
fn md5_like(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn to_data_url_png(bytes: &[u8]) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:image/png;base64,{}", b64)
}

#[tauri::command]
pub fn get_thumbnail(path: String, size: u32) -> AppResult<ThumbnailResult> {
    let size = size.clamp(16, 512);
    let mtime = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64)
        .unwrap_or_default();
    let key = cache_key(&path, size, mtime);

    if let Some(hit) = CACHE.lock().get(&key) {
        return Ok(ThumbnailResult { data_url: hit.data_url.clone(), width: hit.width, height: hit.height });
    }

    if let Some(dp) = disk_cache_path(&key) {
        if let Ok(bytes) = std::fs::read(&dp) {
            if let Ok(img) = image::load_from_memory(&bytes) {
                let res = ThumbnailResult {
                    data_url: to_data_url_png(&bytes),
                    width: img.width(),
                    height: img.height(),
                };
                CACHE.lock().put(key, ThumbnailResult { data_url: res.data_url.clone(), width: res.width, height: res.height });
                return Ok(res);
            }
        }
    }

    #[cfg(windows)]
    {
        let png = win::fetch_png(&path, size)?;
        let img = image::load_from_memory(&png)
            .map_err(|e| AppError::Other(format!("image decode: {e}")))?;
        let result = ThumbnailResult {
            data_url: to_data_url_png(&png),
            width: img.width(),
            height: img.height(),
        };
        if let Some(dp) = disk_cache_path(&key) {
            let _ = std::fs::write(&dp, &png);
        }
        CACHE.lock().put(key, ThumbnailResult { data_url: result.data_url.clone(), width: result.width, height: result.height });
        return Ok(result);
    }
    #[cfg(not(windows))]
    {
        let _ = size;
        Err(AppError::NotSupported("thumbnails are windows-only".into()))
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::thread;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
    };
    use windows::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK,
        SIIGBF_RESIZETOFIT,
    };

    pub fn fetch_png(path: &str, size: u32) -> AppResult<Vec<u8>> {
        let path = path.to_owned();
        let handle = thread::spawn(move || -> AppResult<Vec<u8>> {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
                let res = inner(&path, size);
                CoUninitialize();
                res
            }
        });
        handle.join().map_err(|_| AppError::Other("thumb thread panicked".into()))?
    }

    unsafe fn inner(path: &str, size: u32) -> AppResult<Vec<u8>> {
        let wide: Vec<u16> = OsStr::new(path).encode_wide().chain(std::iter::once(0)).collect();
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)
                .map_err(|e| AppError::Other(format!("SHCreateItemFromParsingName: {e}")))?;
        let sz = SIZE { cx: size as i32, cy: size as i32 };
        let hbmp: HBITMAP = factory
            .GetImage(sz, SIIGBF_RESIZETOFIT | SIIGBF_BIGGERSIZEOK)
            .map_err(|e| AppError::Other(format!("GetImage: {e}")))?;
        let png = hbitmap_to_png(hbmp).map_err(|e| {
            let _ = DeleteObject(hbmp);
            e
        })?;
        let _ = DeleteObject(hbmp);
        Ok(png)
    }

    unsafe fn hbitmap_to_png(hbmp: HBITMAP) -> AppResult<Vec<u8>> {
        let mut bmp = BITMAP::default();
        let n = GetObjectW(
            hbmp,
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bmp as *mut _ as *mut _),
        );
        if n == 0 {
            return Err(AppError::Other("GetObjectW failed".into()));
        }
        let width = bmp.bmWidth;
        let height = bmp.bmHeight.abs();
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let stride = (width as usize) * 4;
        let mut buf = vec![0u8; stride * height as usize];
        let dc = HDC::default();
        let ok = GetDIBits(
            dc,
            hbmp,
            0,
            height as u32,
            Some(buf.as_mut_ptr() as *mut _),
            &mut bi,
            DIB_RGB_COLORS,
        );
        if ok == 0 {
            return Err(AppError::Other("GetDIBits failed".into()));
        }
        // BGRA → RGBA
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        // png encode
        let mut png_bytes: Vec<u8> = Vec::new();
        {
            let mut cursor = std::io::Cursor::new(&mut png_bytes);
            let img = image::RgbaImage::from_raw(width as u32, height as u32, buf)
                .ok_or_else(|| AppError::Other("RgbaImage build failed".into()))?;
            image::DynamicImage::ImageRgba8(img)
                .write_to(&mut cursor, image::ImageFormat::Png)
                .map_err(|e| AppError::Other(format!("png encode: {e}")))?;
        }
        Ok(png_bytes)
    }
}
