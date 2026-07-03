//! ウィンドウハンドルから Win32 HWND を取り出すヘルパ。
//!
//! **これが HWND 取得の正規経路** (Phase 5 の本実装もこれを使う):
//! iced 0.14 の `window::run(id, |w| ..)` でクロージャに渡る `&dyn Window` は
//! `HasWindowHandle` を実装しており、`w.window_handle()?.as_raw()` を本関数に
//! 渡せば Win32 HWND が取れる。
//!
//! 注意: `window::raw_id` の u64 も現行 winit (0.30.x) では HWND 値と一致するが、
//! それは winit の**非公開の内部表現**への依存 (WindowId のカウンタ化提案あり)。
//! スパイクで実証済みだが、恒久コードでは本経路を使うこと。
//!
//! domain へは `isize` で渡す流儀 (GPUI 版 pane.rs `hwnd_of` の先例を踏襲。
//! windows crate のバージョン差で型が割れないようにするため)。

use raw_window_handle::RawWindowHandle;

/// raw-window-handle の値から Win32 HWND を `isize` で取り出す。
/// Win32 以外のハンドル (他プラットフォーム) は `None`。
pub fn hwnd_from_raw(handle: RawWindowHandle) -> Option<isize> {
    match handle {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
        _ => None,
    }
}

/// スクリーン座標 → クライアント座標 (物理 px)。OLE D&D コールバックの
/// POINTL (スクリーン) をペインヒットテスト用のウィンドウ座標へ変換する。
/// iced の論理座標にするには呼び出し側で scale factor で割ること。
pub fn screen_to_client(hwnd: isize, x: i32, y: i32) -> Option<(i32, i32)> {
    use windows::Win32::Foundation::{HWND, POINT};
    use windows::Win32::Graphics::Gdi::ScreenToClient;
    let mut pt = POINT { x, y };
    let ok = unsafe { ScreenToClient(HWND(hwnd as *mut _), &mut pt) };
    ok.as_bool().then_some((pt.x, pt.y))
}
