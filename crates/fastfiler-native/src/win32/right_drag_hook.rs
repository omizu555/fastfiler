//! 右ボタン D&D 用 Win32 サブクラス (ADR 0011)。
//!
//! floem 0.2 は secondary (右ボタン) `PointerUp` を `EventListener::PointerUp`
//! に配信しないため、右ボタンドラッグの drop タイミング検出には Windows
//! メッセージレベルでフックする必要がある。`SetWindowSubclass` で
//! `WM_RBUTTONUP` を直接拾い、`AppState.right_drag` が `Some` なら
//! ドロップメニューを表示してメッセージを消費する (シェルメニュー抑制)。

#![cfg(windows)]

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use floem::kurbo::Point;
use floem::reactive::{SignalGet, SignalUpdate};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::WM_RBUTTONUP;

use crate::state::AppState;

/// サブクラス識別 ID (任意の定数だが、他と衝突しない値を選ぶ)。
const SUBCLASS_ID: usize = 0xFA57_F11E;

/// グローバルに 1 つだけ保持する `AppState` 参照 (UI スレッドからのみアクセス)。
/// サブクラスプロシージャは `extern "system" fn` で context を渡せないため、
/// ここに保管する。`install` 呼び出しで set される。
static APP_STATE_PTR: AtomicIsize = AtomicIsize::new(0);
/// 二重登録防止フラグ。
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// 起動時に 1 回だけ呼ぶ。floem ウィンドウの HWND に WM_RBUTTONUP フックを仕掛ける。
///
/// 失敗してもアプリは続行する (右ボタン D&D メニューが出ないだけ)。
pub fn install(hwnd: HWND, app: AppState) {
    if INSTALLED.load(Ordering::Acquire) {
        return;
    }

    // `AppState` はプロセス終了まで生存させたいので Box::leak で 'static 化。
    let leaked: &'static AppState = Box::leak(Box::new(app));
    let ptr = leaked as *const AppState as isize;
    APP_STATE_PTR.store(ptr, Ordering::Release);

    let ok = unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0) };
    if ok.as_bool() {
        INSTALLED.store(true, Ordering::Release);
        crate::flog!("[right-drag-hook] SetWindowSubclass 成功 hwnd={:?}", hwnd.0);
    } else {
        crate::flog!("[right-drag-hook] SetWindowSubclass 失敗 hwnd={:?}", hwnd.0);
        APP_STATE_PTR.store(0, Ordering::Release);
    }
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _uid_subclass: usize,
    _dw_ref_data: usize,
) -> LRESULT {
    if msg == WM_RBUTTONUP {
        if let Some(app) = app_state() {
            if try_show_right_drop_menu(app, lparam) {
                // メニュー表示済。DefSubclassProc に渡さないことで
                // SecondaryClick / context_menu 経路 (シェルメニュー) を抑制。
                return LRESULT(0);
            }
        }
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

/// `right_drag` が `Some` ならドロップメニューを表示する。
/// 戻り値は「メッセージを消費したか」(メニュー表示した場合 `true`)。
fn try_show_right_drop_menu(app: &AppState, lparam: LPARAM) -> bool {
    let state = match app.right_drag.get_untracked() {
        Some(s) => s,
        None => return false,
    };

    // メニュー表示前にクリア (再入防止 + 状態の一貫性)。
    app.right_drag.set(None);
    // dragging も終わらせる (PointerUp listener が来ないため自前でクリア)。
    if app.dragging.get_untracked().is_some() {
        app.dragging.set(None);
    }
    // Spring-loaded hover も解除。
    crate::ui::spring::disarm(app);

    let Some(target_id) = state.hover_pane else {
        crate::flog!("[right-drag-hook] WM_RBUTTONUP: hover_pane なし、キャンセル");
        return true;
    };
    if target_id == state.source_pane {
        // 同一ペインドロップは無視 (左ボタン D&D と同じ挙動)。
        crate::flog!("[right-drag-hook] WM_RBUTTONUP: 同一ペイン、キャンセル");
        return true;
    }
    let Some(target_pane) = app.find_pane(target_id) else {
        crate::flog!(
            "[right-drag-hook] WM_RBUTTONUP: target pane (id={}) 未発見",
            target_id
        );
        return true;
    };

    // メニュー表示位置は WM_RBUTTONUP の LPARAM (クライアント座標) を使う。
    // LOWORD = x, HIWORD = y (符号付き 16bit)。
    let xy = lparam.0 as u32;
    let x = (xy & 0xFFFF) as i16 as f64;
    let y = ((xy >> 16) & 0xFFFF) as i16 as f64;
    let menu_pos = Point::new(x, y);

    crate::flog!(
        "[right-drag-hook] show menu target_id={} src={} paths={} pos=({:.0},{:.0})",
        target_id,
        state.source_pane,
        state.paths.len(),
        menu_pos.x,
        menu_pos.y
    );

    crate::ui::drop_exec::show_right_drop_menu(
        app.clone(),
        target_pane,
        Some(state.source_pane),
        state.paths,
        menu_pos,
    );
    true
}

fn app_state() -> Option<&'static AppState> {
    let p = APP_STATE_PTR.load(Ordering::Acquire);
    if p == 0 {
        None
    } else {
        // SAFETY: install で Box::leak した 'static 参照のポインタを保管している。
        unsafe { Some(&*(p as *const AppState)) }
    }
}
