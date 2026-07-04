//! OLE D&D 受信 (IDropTarget) の GUI 層向け配線 (計画書 §8)。
//!
//! 実体は `fastfiler_domain::ole_dnd::register_drop_target` (floem 時代の実装を正式採用)。
//! ここでは 2 つの変換だけを行う:
//!
//! 1. HWND を `isize` で受け、windows crate の `HWND` へ変換する
//!    (GUI 層に windows crate の型を漏らさない)。
//! 2. 登録ハンドル `DropTargetRegistration` は COM 参照を含み `!Send` のため、
//!    **UI スレッドの thread_local** に保持する (iced の update() は UI スレッドで
//!    走るのでそこから呼べば安全)。
//!
//! 呼び出し前提: `fastfiler_domain::ole_dnd::init_ole()` 済みの UI スレッド。

use std::cell::RefCell;
use std::path::PathBuf;

use fastfiler_domain::ole_dnd::{
    register_drop_target, DropTargetCallbacks, DropTargetRegistration,
};
use windows::Win32::Foundation::HWND;

pub use fastfiler_domain::ole_dnd::DropLeaveFn;

// GUI 層が windows crate に依存せず MK_*/DROPEFFECT_* を扱えるよう、素の u32 で公開する。
// (値は Win32 の定義そのもの。ドラッグ中の grfKeyState / *pdwEffect に対応)
/// 右ボタンドラッグ判別 (`keys & MK_RBUTTON != 0`)。
pub const MK_RBUTTON: u32 = 0x02;
pub const MK_SHIFT: u32 = 0x04;
pub const MK_CONTROL: u32 = 0x08;
pub const DROPEFFECT_NONE: u32 = 0;
pub const DROPEFFECT_COPY: u32 = 1;
pub const DROPEFFECT_MOVE: u32 = 2;

/// enter/over/drop 共通のコールバック型。
/// 引数は (paths, (x, y) スクリーン座標, MK_* フラグ, 許可 DROPEFFECT マスク)、
/// 戻り値は希望 DROPEFFECT。COM 層側で `希望 & 許可マスク` に丸められるため、
/// 許可されない効果を返すと NONE (ドロップ拒否) になる — `allowed` を見て選ぶこと。
pub type DropOverFn = Box<dyn Fn(&[PathBuf], (i32, i32), u32, u32) -> u32 + Send + Sync>;

/// GUI 層向けに簡略化したコールバック一式。
pub struct DropCallbacks {
    pub on_enter: DropOverFn,
    pub on_over: DropOverFn,
    pub on_leave: DropLeaveFn,
    pub on_drop: DropOverFn,
}

thread_local! {
    /// UI スレッド上の (hwnd, 登録) 保持 (COM 参照は !Send のためスレッド外へ出せない)。
    static REGISTRATIONS: RefCell<Vec<(isize, DropTargetRegistration)>> =
        const { RefCell::new(Vec::new()) };
}

/// 自前 IDropTarget を `hwnd` へ登録する。
///
/// 同じ hwnd への再登録は「古い登録を先に解除してから新規登録」になる
/// (古い RAII ハンドルを残すと、その Drop 時の RevokeDragDrop が新しい登録を
/// 巻き添えで解除してしまうため)。
///
/// winit が既に登録した IDropTarget があっても domain 側が `RevokeDragDrop` で
/// 外してから登録する (ole_dnd.rs 参照)。二重の保険として、iced 側でも
/// `window::Settings.platform_specific.drag_and_drop = false` を推奨。
///
/// **OleInitialize 済みの UI スレッドから呼ぶこと** (iced では `update()` 内が該当)。
pub fn register(hwnd: isize, cb: DropCallbacks) -> Result<(), String> {
    // 前提の動的検査: init_ole() 失敗 (例: 先行の CoInitializeEx(MTA) と衝突) を
    // 握り潰すと「D&D が黙って効かない」事故になるため、ここで明示的に弾く。
    if !fastfiler_domain::ole_dnd::is_ole_available() {
        return Err(
            "OLE が初期化されていません (init_ole() 未呼出または失敗)。D&D 受信は無効".into(),
        );
    }
    // 同一 hwnd の既存登録を先に破棄 (Drop → RevokeDragDrop)
    revoke(hwnd);

    let hwnd_win = HWND(hwnd as *mut core::ffi::c_void);
    let DropCallbacks {
        on_enter,
        on_over,
        on_leave,
        on_drop,
    } = cb;
    let callbacks = DropTargetCallbacks {
        on_enter: Box::new(move |paths, pt, keys, allowed| {
            on_enter(paths, (pt.x, pt.y), keys, allowed)
        }),
        on_over: Box::new(move |paths, pt, keys, allowed| {
            on_over(paths, (pt.x, pt.y), keys, allowed)
        }),
        on_leave,
        on_drop: Box::new(move |paths, pt, keys, allowed| {
            on_drop(paths, (pt.x, pt.y), keys, allowed)
        }),
    };
    let registration = register_drop_target(hwnd_win, callbacks).map_err(|e| e.to_string())?;
    REGISTRATIONS.with(|r| r.borrow_mut().push((hwnd, registration)));
    Ok(())
}

/// 指定 hwnd の登録を解除する (RAII 経由で RevokeDragDrop)。
///
/// **ウィンドウを閉じるときに必ず呼ぶこと**: Windows は破棄済み HWND 値を新しい
/// ウィンドウに再利用しうるため、古い登録を残すと再利用先の D&D を巻き添えで
/// 解除する事故になる (register() 側の同一 hwnd 置換は保険にすぎない)。
pub fn revoke(hwnd: isize) {
    REGISTRATIONS.with(|r| r.borrow_mut().retain(|(h, _)| *h != hwnd));
}

/// このスレッドで保持している登録を全て解除する (アプリ終了時に呼ぶ)。
pub fn revoke_all() {
    REGISTRATIONS.with(|r| r.borrow_mut().clear());
}
