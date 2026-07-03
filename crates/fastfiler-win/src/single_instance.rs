//! 多重起動防止 (Named Mutex) + 既存ウィンドウの前面化 (F-1106)。
//!
//! GPUI 版 win32_single_instance.rs の移植 + 堅牢化:
//! - GPUI 版はタイトル文字列 `FindWindowW(NULL, "FastFiler")` に依存していた。
//!   本実装は mutex 獲得側 (先行プロセス) が **共有メモリに自分の HWND を書き**、
//!   後発プロセスはそれを読んで前面化する — タイトル変更に影響されない。
//! - mutex 名は iced 版専用 (開発期は GPUI 版と並行起動できるよう別名。
//!   Phase 7 の切替時に統一を判断)。

use windows::core::PCWSTR;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND};
use windows::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, OpenFileMappingW, FILE_MAP_READ, FILE_MAP_WRITE,
    PAGE_READWRITE,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

const MUTEX_NAME: &str = "Local\\FastFiler-Iced-SingleInstance-Mutex-v1\0";
const SHM_NAME: &str = "Local\\FastFiler-Iced-MainWindow-Hwnd-v1\0";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Named Mutex を取得して多重起動の有無を判定する。
/// `true` = 最初のインスタンス (続行可)。`false` = 既に起動中 (終了すべき)。
pub fn acquire_single_instance() -> bool {
    let name = wide(MUTEX_NAME);
    unsafe {
        let handle = match CreateMutexW(None, false, PCWSTR(name.as_ptr())) {
            Ok(h) => h,
            Err(_) => return true, // mutex 作成に失敗した場合は通常起動を許可
        };
        let already = GetLastError() == ERROR_ALREADY_EXISTS;
        // CloseHandle を呼ばない限りプロセス終了まで OS が保持する
        let _keep_alive = handle;
        !already
    }
}

/// 先行プロセスが自分のメインウィンドウ HWND を共有メモリへ公開する。
/// (ウィンドウ生成後に一度呼ぶ)
pub fn publish_main_hwnd(hwnd: isize) {
    let name = wide(SHM_NAME);
    unsafe {
        let Ok(mapping) = CreateFileMappingW(
            HANDLE::default(),
            None,
            PAGE_READWRITE,
            0,
            std::mem::size_of::<isize>() as u32,
            PCWSTR(name.as_ptr()),
        ) else {
            return;
        };
        let view = MapViewOfFile(mapping, FILE_MAP_WRITE, 0, 0, 0);
        if !view.Value.is_null() {
            (view.Value as *mut isize).write_unaligned(hwnd);
        }
        // mapping ハンドルは保持し続ける (プロセス終了まで有効)
        let _keep_alive = mapping;
    }
}

/// 起動済みインスタンスのウィンドウを前面化する (後発プロセスが終了直前に呼ぶ)。
/// 共有メモリの HWND を優先し、無ければ何もしない。失敗は黙って無視。
pub fn activate_existing_window() {
    let name = wide(SHM_NAME);
    unsafe {
        let Ok(mapping) = OpenFileMappingW(FILE_MAP_READ.0, false, PCWSTR(name.as_ptr())) else {
            return;
        };
        let view = MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, 0);
        if view.Value.is_null() {
            return;
        }
        let hwnd_val = (view.Value as *const isize).read_unaligned();
        if hwnd_val == 0 {
            return;
        }
        let hwnd = HWND(hwnd_val as *mut _);
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
}
