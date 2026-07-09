//! STA COM 初期化の共通ヘルパ。
//!
//! 「CoInitializeEx(STA) → 処理 → 成功時のみ CoUninitialize」の定型が
//! shell.rs / file_ops.rs に 5 回コピペされ、COINIT フラグも不揃いだったのを
//! 一元化する (OLE1 DDE 無効化はパフォーマンス推奨設定)。

use std::thread::JoinHandle;

use windows::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};

/// 現在のスレッドを STA として f を実行する。
/// CoInitializeEx が成功 (S_OK / S_FALSE) したときだけ対で CoUninitialize する。
pub fn with_sta<T>(f: impl FnOnce() -> T) -> T {
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
    let r = f();
    if hr.is_ok() {
        unsafe { CoUninitialize() };
    }
    r
}

/// 専用 STA スレッドを起こして f を実行する (ShellExecuteW / IFileOperation 等、
/// UI スレッド再入を避けたい COM 呼び出し用)。
pub fn spawn_sta<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> JoinHandle<T> {
    std::thread::spawn(move || with_sta(f))
}
