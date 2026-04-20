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

// ---------------- drag-out (Files → エクスプローラ等) ----------------
//
// SHCreateDataObject で CF_HDROP 相当の IDataObject を作り、
// 自前の IDropSource と一緒に DoDragDrop に渡す。
// DoDragDrop はブロッキングでメインスレッド (STA) 必須なので
// run_on_main_thread でディスパッチする。

#[tauri::command]
pub fn ole_dnd_start_drag(
    app: tauri::AppHandle,
    paths: Vec<String>,
    allowed_effects: u32,
) -> AppResult<u32> {
    if paths.is_empty() {
        return Err(AppError::Other("paths が空です".into()));
    }
    #[cfg(not(windows))]
    {
        let _ = (app, allowed_effects);
        return Err(AppError::Other("Windows でのみ利用可能".into()));
    }
    #[cfg(windows)]
    {
        let (tx, rx) = std::sync::mpsc::channel::<AppResult<u32>>();
        let allowed = if allowed_effects == 0 { 0x7 } else { allowed_effects }; // COPY|MOVE|LINK
        app.run_on_main_thread(move || {
            let r = unsafe { do_drag_drop(&paths, allowed) };
            let _ = tx.send(r);
        })
        .map_err(|e| AppError::Other(format!("dispatch failed: {e}")))?;
        rx.recv().map_err(|e| AppError::Other(format!("recv failed: {e}")))?
    }
}

#[cfg(windows)]
unsafe fn do_drag_drop(paths: &[String], allowed_effects: u32) -> AppResult<u32> {
    use windows::core::{ComObject, PCWSTR};
    use windows::Win32::System::Com::IDataObject;
    use windows::Win32::System::Ole::{DoDragDrop, DROPEFFECT};
    use windows::Win32::UI::Shell::{
        Common::ITEMIDLIST, ILFree, SHCreateDataObject, SHParseDisplayName,
    };

    // 各 path → 完全 PIDL
    let mut full_pidls: Vec<*mut ITEMIDLIST> = Vec::with_capacity(paths.len());
    struct PidlGuard(Vec<*mut ITEMIDLIST>);
    impl Drop for PidlGuard {
        fn drop(&mut self) {
            for p in self.0.drain(..) {
                if !p.is_null() { unsafe { ILFree(Some(p)); } }
            }
        }
    }

    for p in paths {
        let wide: Vec<u16> = p.encode_utf16().chain(std::iter::once(0)).collect();
        let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        if SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None).is_err()
            || pidl.is_null()
        {
            for q in &full_pidls { if !q.is_null() { ILFree(Some(*q)); } }
            return Err(AppError::Other(format!("SHParseDisplayName 失敗: {p}")));
        }
        full_pidls.push(pidl);
    }
    let pidl_guard = PidlGuard(full_pidls.clone());
    full_pidls.clear();

    // 親 PIDL を作る (最初の項目をクローンして最後の ID を削る)
    let parent_pidl: *mut ITEMIDLIST = clone_parent_pidl(pidl_guard.0[0])?;
    struct ParentGuard(*mut ITEMIDLIST);
    impl Drop for ParentGuard {
        fn drop(&mut self) { if !self.0.is_null() { unsafe { ILFree(Some(self.0)); } } }
    }
    let _parent_guard = ParentGuard(parent_pidl);

    // 各 child の last ID ポインタを取り出し配列に
    let mut children: Vec<*const ITEMIDLIST> = Vec::with_capacity(pidl_guard.0.len());
    let parent_path = parent_path_from(&paths[0]);
    for (i, &full) in pidl_guard.0.iter().enumerate() {
        // 同一親フォルダチェック
        if parent_path_from(&paths[i]) != parent_path {
            return Err(AppError::Other(
                "異なる親フォルダの項目が混在しています (drag-out は同一フォルダのみ対応)".into(),
            ));
        }
        let last = il_find_last_id(full);
        if last.is_null() {
            return Err(AppError::Other("ILFindLastID 失敗".into()));
        }
        children.push(last as *const _);
    }

    let data: IDataObject = SHCreateDataObject(
        Some(parent_pidl as *const _),
        Some(&children),
        None,
    )
    .map_err(|e| AppError::Other(format!("SHCreateDataObject: {e}")))?;

    let source: ComObject<DropSource> = DropSource::new().into();
    let isrc: windows::Win32::System::Ole::IDropSource = source.to_interface();

    let mut effect = DROPEFFECT(0);
    let allowed_eff = DROPEFFECT(allowed_effects);
    let hr = DoDragDrop(&data, &isrc, allowed_eff, &mut effect);
    // DRAGDROP_S_DROP = 0x40100, DRAGDROP_S_CANCEL = 0x40101
    let _ = hr;
    Ok(effect.0)
}

#[cfg(windows)]
unsafe fn clone_parent_pidl(
    full: *mut windows::Win32::UI::Shell::Common::ITEMIDLIST,
) -> AppResult<*mut windows::Win32::UI::Shell::Common::ITEMIDLIST> {
    use windows::Win32::UI::Shell::{ILClone, ILRemoveLastID};
    let cloned = ILClone(full as *const _);
    if cloned.is_null() {
        return Err(AppError::Other("ILClone 失敗".into()));
    }
    let _ = ILRemoveLastID(Some(cloned));
    Ok(cloned)
}

#[cfg(windows)]
unsafe fn il_find_last_id(
    pidl: *mut windows::Win32::UI::Shell::Common::ITEMIDLIST,
) -> *mut windows::Win32::UI::Shell::Common::ITEMIDLIST {
    use windows::Win32::UI::Shell::ILFindLastID;
    ILFindLastID(pidl as *const _)
}

fn parent_path_from(p: &str) -> String {
    let p = p.trim_end_matches(|c| c == '\\' || c == '/');
    match p.rfind(|c| c == '\\' || c == '/') {
        Some(i) => p[..i].to_string(),
        None => String::new(),
    }
}

#[cfg(windows)]
mod impl_source {
    use windows::core::implement;
    use windows::Win32::Foundation::{BOOL, S_OK};
    use windows::Win32::System::Ole::{IDropSource, IDropSource_Impl};
    use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;

    // HRESULT 定数 (windows-rs 0.58 では Win32::System::Ole に無いため自前定義)
    const DRAGDROP_S_DROP: windows::core::HRESULT = windows::core::HRESULT(0x00040100u32 as i32);
    const DRAGDROP_S_CANCEL: windows::core::HRESULT = windows::core::HRESULT(0x00040101u32 as i32);
    const DRAGDROP_S_USEDEFAULTCURSORS: windows::core::HRESULT = windows::core::HRESULT(0x00040102u32 as i32);

    #[implement(IDropSource)]
    pub struct DropSource;
    impl DropSource {
        pub fn new() -> Self { Self }
    }

    impl IDropSource_Impl for DropSource_Impl {
        fn QueryContinueDrag(
            &self,
            f_escape_pressed: BOOL,
            grf_key_state: MODIFIERKEYS_FLAGS,
        ) -> windows::core::HRESULT {
            const MK_LBUTTON: u32 = 0x0001;
            const MK_RBUTTON: u32 = 0x0002;
            if f_escape_pressed.as_bool() { return DRAGDROP_S_CANCEL; }
            // ボタンがどちらも離されたら drop
            if grf_key_state.0 & (MK_LBUTTON | MK_RBUTTON) == 0 {
                return DRAGDROP_S_DROP;
            }
            S_OK
        }
        fn GiveFeedback(&self, _dw_effect: windows::Win32::System::Ole::DROPEFFECT) -> windows::core::HRESULT {
            DRAGDROP_S_USEDEFAULTCURSORS
        }
    }
}

#[cfg(windows)]
use impl_source::DropSource;
