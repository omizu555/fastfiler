// v4.0 (40a) ネイティブ IContextMenu 統合 — 本実装
//
// shell_menu_show(paths, x, y) を呼ぶと、Windows シェル拡張が登録した
// 右クリックメニューをスクリーン座標 (x, y) にネイティブ TrackPopupMenu で表示し、
// ユーザが選択した項目を InvokeCommand で実行する。
//
// 制約 / 既知:
// - 同一フォルダ配下の項目のみサポート (異なる親フォルダの混合は NG)
// - IContextMenu2/3 の HandleMenuMsg はサポートせず (一部拡張で owner-draw が崩れる)
// - メインスレッド (HWND の所属スレッド = Tauri main) で呼ぶ必要があるため
//   AppHandle.run_on_main_thread でディスパッチ

use crate::error::{AppError, AppResult};

#[tauri::command]
pub fn shell_menu_show(
    app: tauri::AppHandle,
    paths: Vec<String>,
    x: i32,
    y: i32,
) -> AppResult<bool> {
    if paths.is_empty() {
        return Err(AppError::Other("paths が空です".into()));
    }
    #[cfg(not(windows))]
    {
        let _ = (app, paths, x, y);
        return Err(AppError::Other("Windows でのみ利用可能".into()));
    }
    #[cfg(windows)]
    {
        let (tx, rx) = std::sync::mpsc::channel::<AppResult<bool>>();
        let app2 = app.clone();
        let paths2 = paths.clone();
        app.run_on_main_thread(move || {
            let res = unsafe { show_menu_impl(&app2, &paths2, x, y) };
            let _ = tx.send(res);
        })
        .map_err(|e| AppError::Other(format!("dispatch failed: {e}")))?;
        rx.recv().map_err(|e| AppError::Other(format!("recv failed: {e}")))?
    }
}

#[tauri::command]
pub fn shell_menu_query(_paths: Vec<String>) -> AppResult<()> {
    // フロント側で UI を組まず TrackPopupMenu を直接出す方式にしたため
    // 個別 query API は不要。スタブとして残す。
    Err(AppError::Other("shell_menu_query は未使用 (shell_menu_show を使用)".into()))
}

#[tauri::command]
pub fn shell_menu_invoke(_paths: Vec<String>, _id: u32) -> AppResult<()> {
    Err(AppError::Other("shell_menu_invoke は未使用 (shell_menu_show を使用)".into()))
}

#[cfg(windows)]
unsafe fn show_menu_impl(
    app: &tauri::AppHandle,
    paths: &[String],
    x: i32,
    y: i32,
) -> AppResult<bool> {
    use tauri::Manager;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::{
        Common::ITEMIDLIST, IContextMenu, ILClone, ILFindLastID, ILFree, ILRemoveLastID,
        SHBindToParent, SHParseDisplayName, IShellFolder, CMINVOKECOMMANDINFO,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreatePopupMenu, DestroyMenu, TrackPopupMenu, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    };

    // 1) tauri main window から HWND
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| AppError::Other("main window not found".into()))?;
    let raw_hwnd = window.hwnd().map_err(|e| AppError::Other(format!("hwnd: {e}")))?;
    let hwnd = HWND(raw_hwnd.0 as *mut _);

    // 2) 各パスを PIDL に変換し、最初の親フォルダを基準に child PIDL の配列を作る
    let mut pidls: Vec<*mut ITEMIDLIST> = Vec::with_capacity(paths.len());
    let mut child_pidls: Vec<*const ITEMIDLIST> = Vec::with_capacity(paths.len());
    let mut parent_folder: Option<IShellFolder> = None;
    let mut first_parent_path: Option<String> = None;

    // 後始末用 RAII
    struct PidlGuard(Vec<*mut ITEMIDLIST>);
    impl Drop for PidlGuard {
        fn drop(&mut self) {
            for p in self.0.drain(..) {
                if !p.is_null() {
                    unsafe { ILFree(Some(p)); }
                }
            }
        }
    }

    for p in paths {
        let wide: Vec<u16> = p.encode_utf16().chain(std::iter::once(0)).collect();
        let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        if SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None).is_err() || pidl.is_null() {
            // 後始末
            for q in &pidls {
                if !q.is_null() { ILFree(Some(*q)); }
            }
            return Err(AppError::Other(format!("SHParseDisplayName 失敗: {p}")));
        }
        pidls.push(pidl);
    }
    let _guard = PidlGuard(pidls.clone()); // クローンを Drop で free
    pidls.clear(); // _guard が所有

    // 親フォルダを取得し、各 PIDL の最後の ID を child として配列化
    for &full in &_guard.0 {
        let mut child_raw: *mut ITEMIDLIST = std::ptr::null_mut();
        let folder: IShellFolder = SHBindToParent(full as *const _, Some(&mut child_raw))
            .map_err(|e| AppError::Other(format!("SHBindToParent 失敗: {e}")))?;
        if parent_folder.is_none() {
            parent_folder = Some(folder);
            // 親フォルダのパスを覚える (混在チェック用 — 簡易的に元パスから親で判定)
            let p = paths.first().cloned().unwrap_or_default();
            first_parent_path = Some(parent_path_str(&p));
        } else {
            // 簡易チェック: 元パスの親が同じかどうか
            let idx = child_pidls.len();
            let p = paths.get(idx).cloned().unwrap_or_default();
            if Some(parent_path_str(&p)) != first_parent_path {
                return Err(AppError::Other(
                    "異なる親フォルダの項目が混在しています (同一フォルダ配下のみ対応)".into(),
                ));
            }
        }
        child_pidls.push(child_raw as *const _);
    }
    let parent = parent_folder
        .ok_or_else(|| AppError::Other("親フォルダの取得に失敗".into()))?;

    // 3) IContextMenu を取得
    let ctxmenu: IContextMenu = parent
        .GetUIObjectOf(hwnd, &child_pidls, None)
        .map_err(|e| AppError::Other(format!("GetUIObjectOf 失敗: {e}")))?;

    // 4) HMENU を作って QueryContextMenu
    let hmenu = CreatePopupMenu().map_err(|e| AppError::Other(format!("CreatePopupMenu: {e}")))?;
    struct MenuGuard(windows::Win32::UI::WindowsAndMessaging::HMENU);
    impl Drop for MenuGuard {
        fn drop(&mut self) { unsafe { let _ = DestroyMenu(self.0); } }
    }
    let _menu_guard = MenuGuard(hmenu);

    const ID_MIN: u32 = 1;
    const ID_MAX: u32 = 0x7FFF;
    const CMF_NORMAL: u32 = 0x0;
    const CMF_EXTENDEDVERBS: u32 = 0x100;
    ctxmenu
        .QueryContextMenu(hmenu, 0, ID_MIN, ID_MAX, CMF_NORMAL | CMF_EXTENDEDVERBS)
        .map_err(|e| AppError::Other(format!("QueryContextMenu 失敗: {e}")))?;

    // 5) TrackPopupMenu (TPM_RETURNCMD でブロッキングして選択 ID を取得)
    let cmd = TrackPopupMenu(
        hmenu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON,
        x, y, 0,
        hwnd, None,
    );
    let cmd_id = cmd.0 as u32;
    if cmd_id == 0 {
        return Ok(false); // ユーザがキャンセル
    }

    // 6) InvokeCommand
    let verb_idx = cmd_id - ID_MIN;
    let mut info = CMINVOKECOMMANDINFO::default();
    info.cbSize = std::mem::size_of::<CMINVOKECOMMANDINFO>() as u32;
    info.hwnd = hwnd;
    info.lpVerb = windows::core::PCSTR(verb_idx as usize as *const u8);
    info.nShow = windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL.0;
    ctxmenu
        .InvokeCommand(&info)
        .map_err(|e| AppError::Other(format!("InvokeCommand 失敗: {e}")))?;

    // PIDL は親由来のものは Free 不要 (parent が所有)。フル PIDL は _guard が drop で free
    let _ = (ILClone, ILFindLastID, ILRemoveLastID); // unused warn 抑制
    Ok(true)
}

fn parent_path_str(p: &str) -> String {
    let p = p.trim_end_matches(|c| c == '\\' || c == '/');
    match p.rfind(|c| c == '\\' || c == '/') {
        Some(i) => p[..i].to_string(),
        None => String::new(),
    }
}
