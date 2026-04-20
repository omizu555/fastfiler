// v4.0 (40b / fu-explorer-dnd) OLE D&D 受信側 本実装
//
// Phase 1: エクスプローラ (や他のシェル系アプリ) からのファイル drop を受け取り、
//          フロントへ event "ole-drop" として配信する。
//
// Phase 2 (drag-out / Files → エクスプローラ) は別途。スキャフォールド関数を残す。
//
// 設計:
//   1. Tauri の setup フックで OS スレッドを 1 本起こし、その中で
//      OleInitialize → RegisterDragDrop(hwnd, IDropTarget) を呼ぶ。
//      RegisterDragDrop は呼び出し元 STA に紐付くが、HWND を持つ STA に対して
//      別スレッドから登録しても COM はメッセージマーシャリングを行うため
//      動作する (Tauri main webview のメッセージループが処理)。
//   2. IDropTarget 実装は CF_HDROP のみサポート。
//   3. Drop で AppHandle.emit("ole-drop", { paths, effect, x, y }) を発火。

use crate::error::{AppError, AppResult};
use serde::Serialize;

#[cfg(windows)]
use std::sync::OnceLock;

#[cfg(windows)]
static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

#[derive(Serialize, Clone, Debug)]
pub struct OleDropPayload {
    pub paths: Vec<String>,
    pub effect: u32, // 1=COPY, 2=MOVE, 4=LINK
    pub x: i32,
    pub y: i32,
}

#[cfg(windows)]
pub fn register(app: &tauri::AppHandle) {
    use tauri::Manager;
    let _ = APP_HANDLE.set(app.clone());
    let app2 = app.clone();
    // ウィンドウが用意されてから登録 (Tauri 2 では setup 時点で取得可能)
    std::thread::Builder::new()
        .name("ole-dnd-register".into())
        .spawn(move || {
            // 少し待ってウィンドウ作成完了を確実に
            std::thread::sleep(std::time::Duration::from_millis(300));
            if let Some(window) = app2.get_webview_window("main") {
                if let Ok(hwnd) = window.hwnd() {
                    unsafe {
                        // tauri は別バージョンの windows crate の HWND を返すので raw ポインタ経由で変換
                        let hwnd_local = windows::Win32::Foundation::HWND(hwnd.0 as *mut _);
                        if let Err(e) = ole_register_for_hwnd(hwnd_local) {
                            eprintln!("[ole-dnd] register failed: {e:?}");
                        }
                    }
                }
            }
        })
        .ok();
}

#[cfg(not(windows))]
pub fn register(_app: &tauri::AppHandle) {}

#[cfg(windows)]
unsafe fn ole_register_for_hwnd(hwnd: windows::Win32::Foundation::HWND) -> windows::core::Result<()> {
    use windows::Win32::System::Com::CoInitializeEx;
    use windows::Win32::System::Com::COINIT_APARTMENTTHREADED;
    use windows::Win32::System::Ole::{OleInitialize, RegisterDragDrop};
    use windows::core::ComObject;

    // STA を確保
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    let _ = OleInitialize(None);

    let target: ComObject<DropTarget> = DropTarget::new().into();
    let idt: windows::Win32::System::Ole::IDropTarget = target.to_interface();
    RegisterDragDrop(hwnd, &idt)?;
    // メッセージループはアプリ本体に任せる (このスレッドはここで終了して OK。
    // RegisterDragDrop は HWND の所属スレッドのループで処理されるため)
    Ok(())
}

// ---------------- IDropTarget 実装 ----------------

#[cfg(windows)]
mod impl_target {
    use super::*;
    use windows::core::implement;
    use windows::Win32::Foundation::POINTL;
    use windows::Win32::System::Com::{IDataObject, FORMATETC, DVASPECT_CONTENT, TYMED_HGLOBAL};
    use windows::Win32::System::Ole::IDropTarget_Impl;
    use windows::Win32::System::Ole::{
        DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE, DROPEFFECT_NONE,
    };
    use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
    use windows::Win32::System::Memory::GlobalLock;
    use windows::Win32::System::Memory::GlobalUnlock;
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
    use windows::Win32::System::Ole::CF_HDROP;

    #[implement(windows::Win32::System::Ole::IDropTarget)]
    pub struct DropTarget;

    impl DropTarget {
        pub fn new() -> Self { Self }

        fn extract_paths(pdataobj: Option<&IDataObject>) -> Vec<String> {
            let Some(d) = pdataobj else { return vec![]; };
            unsafe {
                let mut fmt = FORMATETC {
                    cfFormat: CF_HDROP.0 as u16,
                    ptd: std::ptr::null_mut(),
                    dwAspect: DVASPECT_CONTENT.0,
                    lindex: -1,
                    tymed: TYMED_HGLOBAL.0 as u32,
                };
                let mut medium = match d.GetData(&fmt as *const _) {
                    Ok(m) => m,
                    Err(_) => return vec![],
                };
                let _ = &mut fmt; // silence unused_mut (kept for clarity)
                let h = medium.u.hGlobal;
                let p = GlobalLock(h);
                if p.is_null() {
                    let _ = windows::Win32::System::Ole::ReleaseStgMedium(&mut medium);
                    return vec![];
                }
                let hdrop = HDROP(p as *mut _);
                let count = DragQueryFileW(hdrop, 0xFFFFFFFF, None);
                let mut out = Vec::with_capacity(count as usize);
                for i in 0..count {
                    let mut buf = [0u16; 1024];
                    let n = DragQueryFileW(hdrop, i, Some(&mut buf));
                    if n > 0 {
                        out.push(String::from_utf16_lossy(&buf[..n as usize]));
                    }
                }
                let _ = GlobalUnlock(h);
                let _ = windows::Win32::System::Ole::ReleaseStgMedium(&mut medium);
                out
            }
        }

        fn pick_effect(grfkeystate: MODIFIERKEYS_FLAGS, pdweffect: DROPEFFECT) -> DROPEFFECT {
            // Shift = MOVE, Ctrl = COPY, Alt/Ctrl+Shift = LINK
            const MK_CONTROL: u32 = 0x0008;
            const MK_SHIFT: u32 = 0x0004;
            const MK_ALT: u32 = 0x0020;
            let k = grfkeystate.0;
            let want = if k & MK_SHIFT != 0 && k & MK_CONTROL != 0 {
                DROPEFFECT_LINK
            } else if k & MK_SHIFT != 0 {
                DROPEFFECT_MOVE
            } else if k & MK_CONTROL != 0 {
                DROPEFFECT_COPY
            } else if k & MK_ALT != 0 {
                DROPEFFECT_LINK
            } else {
                // 既定はコピー (異なるドライブ/プロセス間は COPY が無難)
                DROPEFFECT_COPY
            };
            // ソースが許可していなければ落とす
            if (pdweffect.0 & want.0) != 0 { want } else if pdweffect.0 & DROPEFFECT_COPY.0 != 0 { DROPEFFECT_COPY } else { DROPEFFECT_NONE }
        }
    }

    impl IDropTarget_Impl for DropTarget_Impl {
        fn DragEnter(
            &self,
            _pdataobj: Option<&IDataObject>,
            grfkeystate: MODIFIERKEYS_FLAGS,
            _pt: &POINTL,
            pdweffect: *mut DROPEFFECT,
        ) -> windows::core::Result<()> {
            unsafe {
                if !pdweffect.is_null() {
                    let cur = *pdweffect;
                    *pdweffect = DropTarget::pick_effect(grfkeystate, cur);
                }
            }
            Ok(())
        }
        fn DragOver(
            &self,
            grfkeystate: MODIFIERKEYS_FLAGS,
            _pt: &POINTL,
            pdweffect: *mut DROPEFFECT,
        ) -> windows::core::Result<()> {
            unsafe {
                if !pdweffect.is_null() {
                    let cur = *pdweffect;
                    *pdweffect = DropTarget::pick_effect(grfkeystate, cur);
                }
            }
            Ok(())
        }
        fn DragLeave(&self) -> windows::core::Result<()> { Ok(()) }
        fn Drop(
            &self,
            pdataobj: Option<&IDataObject>,
            grfkeystate: MODIFIERKEYS_FLAGS,
            pt: &POINTL,
            pdweffect: *mut DROPEFFECT,
        ) -> windows::core::Result<()> {
            let paths = DropTarget::extract_paths(pdataobj);
            let effect = unsafe {
                let cur = if pdweffect.is_null() { DROPEFFECT_COPY } else { *pdweffect };
                let chosen = DropTarget::pick_effect(grfkeystate, cur);
                if !pdweffect.is_null() { *pdweffect = chosen; }
                chosen.0
            };
            if !paths.is_empty() {
                if let Some(app) = APP_HANDLE.get() {
                    use tauri::Emitter;
                    let _ = app.emit("ole-drop", OleDropPayload {
                        paths, effect, x: pt.x, y: pt.y,
                    });
                }
            }
            Ok(())
        }
    }
}

#[cfg(windows)]
use impl_target::DropTarget;

// ---------------- Tauri commands (drag-out 側はまだ未実装) ----------------

#[tauri::command]
pub fn ole_dnd_register() -> AppResult<()> {
    // 実際の登録は setup から行うので、このコマンドは no-op (ヘルスチェック用)
    Ok(())
}

#[tauri::command]
pub fn ole_dnd_start_drag(_paths: Vec<String>, _allowed_effects: u32) -> AppResult<u32> {
    // TODO(v4.0 Phase 2): SHCreateDataObject + DoDragDrop で送信側を実装
    Err(AppError::Other(
        "ole_dnd_start_drag: 未実装 (drag-out は Phase 2 で対応)".into(),
    ))
}
