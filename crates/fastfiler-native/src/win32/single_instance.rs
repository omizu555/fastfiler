//! 多重起動防止 (Named Mutex) + 既存ウィンドウの前面化。
//!
//! `run_app()` の冒頭で [`acquire_single_instance`] を呼び、`false` が
//! 返ったら既に別プロセスが起動中なので静かに終了する。
//!
//! 既存ウィンドウは `FindWindowW(NULL, "FastFiler")` で検索し、
//! 最小化されていれば `SW_RESTORE` して `SetForegroundWindow` で前面化する。

use windows::core::PCWSTR;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

/// セッション単位で一意な mutex 名。`Local\` プレフィックスを付けることで
/// 同一ユーザーセッション内の多重起動だけを抑止する (別ユーザーや別 RDP
/// セッションでは並行起動可)。
const MUTEX_NAME: &str = "Local\\FastFiler-SingleInstance-Mutex-v1\0";

/// Named Mutex を取得して多重起動の有無を判定する。
///
/// - 戻り値 `true`: 自プロセスが最初のインスタンス。続行可。
/// - 戻り値 `false`: 別プロセスが既に起動中。呼び出し側は終了すべき。
///
/// `HANDLE` は `Copy` 型のため Drop は走らない。`CloseHandle` を
/// 明示的に呼ばない限り mutex オブジェクトはプロセス終了時まで OS が
/// 保持する (CloseHandle を呼ぶと参照カウントが減り、他プロセスでの
/// `ERROR_ALREADY_EXISTS` 判定にも影響するので何もしない)。
pub fn acquire_single_instance() -> bool {
    let name: Vec<u16> = MUTEX_NAME.encode_utf16().collect();
    unsafe {
        let handle = match CreateMutexW(None, false, PCWSTR(name.as_ptr())) {
            Ok(h) => h,
            Err(_) => return true, // mutex 作成自体に失敗した場合は通常起動を許可
        };
        let already = GetLastError() == ERROR_ALREADY_EXISTS;
        // HANDLE は Copy 型なので Drop は走らず、CloseHandle を呼ばない限り
        // mutex オブジェクトはプロセス終了時まで OS が保持する。明示的に
        // 解放しないことを示す意図で `_` に束縛しておく。
        let _keep_alive = handle;
        !already
    }
}

/// 起動済みの FastFiler ウィンドウを前面化する。多重起動防止で
/// 自プロセスを終了する直前に呼ぶ用途。失敗しても黙って無視する。
pub fn activate_existing_window() {
    let title: Vec<u16> = "FastFiler\0".encode_utf16().collect();
    unsafe {
        if let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())) {
            if !hwnd.is_invalid() {
                if IsIconic(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                }
                let _ = SetForegroundWindow(hwnd);
            }
        }
    }
}
