// OLE D&D 送信側 (FastFiler → エクスプローラ等外部アプリ)。
//
// FastFiler 内部 D&D とは独立し、ペインから window 外へカーソルが出た瞬間
// `start_drag()` を呼び出してドラッグループを開始する (PointerLeave トリガ)。
//
// 提供データ:
//   * CF_HDROP            — ドラッグ対象のパス一覧
//   * CFSTR_PREFERREDDROPEFFECT — 推奨ドロップ効果 (COPY | MOVE)
//
// 削除判定 (Move 後の元削除):
//   `pdwEffect == DROPEFFECT_MOVE` **かつ** ドロップターゲットが
//   CFSTR_PERFORMEDDROPEFFECT (SetData 経由) で MOVE を通知してきた場合のみ。
//   どちらか片方だけでは削除しない (データ損失防止)。
//   特にブラウザ (Chrome 系 / Firefox) は AI チャット等への添付時、こちらの
//   推奨効果 (MOVE) を pdwEffect にそのまま反響させてくるが、移動を実行した
//   わけではないので Performed DropEffect は送ってこない。これを「移動」と信用
//   して元を削除すると「添付しただけでファイルが消える」データ損失になる。
//
// ADR/設計メモは `plan-dnd2.md` を参照。

#![cfg(windows)]

use crate::error::{AppError, AppResult};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use windows::core::implement;
use windows::Win32::Foundation::{
    BOOL, DV_E_FORMATETC, DV_E_TYMED, E_NOTIMPL, HWND, OLE_E_ADVISENOTSUPPORTED, POINTL, S_OK,
};
use windows::Win32::System::Com::{
    IAdviseSink, IDataObject, IDataObject_Impl, IEnumFORMATETC, IEnumSTATDATA, DATADIR_GET,
    DVASPECT_CONTENT, FORMATETC, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
};
use windows::Win32::System::DataExchange::RegisterClipboardFormatA;
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::{
    DoDragDrop, IDropSource, IDropSource_Impl, IDropTarget, IDropTarget_Impl, OleInitialize,
    OleUninitialize, RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop, CF_HDROP, DROPEFFECT,
    DROPEFFECT_COPY, DROPEFFECT_MOVE, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MK_RBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Shell::SHCreateStdEnumFmtEtc;
use windows::Win32::UI::WindowsAndMessaging::IsWindow;

/// 推奨ドロップ効果 (起動側のヒント)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferredEffect {
    Copy,
    Move,
}

/// ドラッグ結果。呼出側は `Move { delete_source: true }` のときだけ元を削除する。
#[derive(Debug, Clone)]
pub enum DragOutcome {
    /// 何もしなかった (ドロップ拒否、ターゲット無し)
    None,
    /// コピー成功 (削除不要)
    Copy,
    /// 移動成功。delete_source=true のときだけ元を削除する。
    /// (pdwEffect==MOVE かつ PerformedDropEffect==MOVE のときのみ true)
    Move { delete_source: bool },
    /// ESC キャンセル
    Cancel,
    /// エラー
    Error(String),
}

/// ドラッグに使うマウスボタン。Right は「右ボタン D&D」(ターゲット側が
/// ドロップ時にコピー/移動メニューを出す Windows 標準 UX) 用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragButton {
    Left,
    Right,
}

pub struct DragRequest {
    pub paths: Vec<PathBuf>,
    pub preferred: PreferredEffect,
    /// ドラッグ継続判定に使うボタン (このボタンが離されたらドロップ)。
    pub button: DragButton,
}

// ================================================================
// CF_HDROP バイト列を構築
// ================================================================

fn build_hdrop_bytes(paths: &[PathBuf]) -> Vec<u8> {
    // 実装は hdrop.rs に集約 (win_clipboard と同一仕様を共有)
    crate::hdrop::build_hdrop_bytes(paths.iter().map(|p| p.to_string_lossy()))
}

use crate::hdrop::alloc_hglobal_from_bytes;

unsafe fn read_hglobal_dword(stgm: &STGMEDIUM) -> Option<u32> {
    if stgm.tymed != TYMED_HGLOBAL.0 as u32 {
        return None;
    }
    let h = windows::Win32::Foundation::HGLOBAL(stgm.u.hGlobal.0);
    // 規約違反の (またはなりすましの) ドロップ先が 4 バイト未満の HGLOBAL を
    // 渡してきた場合に範囲外読み取りしないようサイズを検証する。
    if GlobalSize(h) < 4 {
        return None;
    }
    let p = GlobalLock(h) as *const u8;
    if p.is_null() {
        return None;
    }
    let mut bytes = [0u8; 4];
    std::ptr::copy_nonoverlapping(p, bytes.as_mut_ptr(), 4);
    let _ = GlobalUnlock(h);
    Some(u32::from_le_bytes(bytes))
}

// ================================================================
// 受信側: IDataObject から CF_HDROP のパス一覧を取り出す
// ================================================================

/// `STGMEDIUM` を確実に解放するための RAII ガード。
/// `extract_hdrop_paths` 内で `GetData` の戻り値を包んで使う。
struct StgMediumGuard(STGMEDIUM);

impl Drop for StgMediumGuard {
    fn drop(&mut self) {
        unsafe { ReleaseStgMedium(&mut self.0) };
    }
}

/// `IDataObject` から CF_HDROP 形式でドロップされたパス一覧を取り出す。
///
/// 取得した `STGMEDIUM` は RAII で `ReleaseStgMedium` する。
/// 形式が CF_HDROP でない場合 (例: シェル内部のシェルアイテム形式のみ) は
/// `Err(AppError::Win32("CF_HDROP not available"))` を返す。
pub fn extract_hdrop_paths(data: &IDataObject) -> AppResult<Vec<PathBuf>> {
    unsafe {
        let fe = FORMATETC {
            cfFormat: CF_HDROP.0,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };
        let stgm = data
            .GetData(&fe)
            .map_err(|e| AppError::Win32(format!("GetData CF_HDROP: {e}")))?;
        let guard = StgMediumGuard(stgm);
        let stgm = &guard.0;
        if stgm.tymed != TYMED_HGLOBAL.0 as u32 {
            return Err(AppError::Win32(format!(
                "CF_HDROP tymed={} (expected HGLOBAL)",
                stgm.tymed
            )));
        }
        // DragQueryFileW ベース (win_clipboard の読取と同一流儀)。
        // 旧実装の手書きポインタ走査は GlobalSize と照合しない範囲外読み取りの
        // 余地があった (任意の外部 IDataObject が入力になる攻撃面)。
        // ANSI 形式 (fWide=0) も DragQueryFileW が OS 側で処理する
        use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
        let hdrop = HDROP(stgm.u.hGlobal.0);
        let count = DragQueryFileW(hdrop, 0xFFFFFFFF, None);
        let mut paths: Vec<PathBuf> = Vec::with_capacity(count as usize);
        for i in 0..count {
            let needed = DragQueryFileW(hdrop, i, None);
            if needed == 0 {
                continue;
            }
            let mut buf: Vec<u16> = vec![0u16; (needed + 1) as usize];
            let written = DragQueryFileW(hdrop, i, Some(&mut buf));
            if written == 0 {
                continue;
            }
            paths.push(PathBuf::from(String::from_utf16_lossy(
                &buf[..written as usize],
            )));
        }
        Ok(paths)
    }
}

// ================================================================
// Clipboard format IDs (lazy)
// ================================================================

pub(crate) fn cf_preferred_drop_effect() -> u16 {
    unsafe { RegisterClipboardFormatA(windows::core::s!("Preferred DropEffect")) as u16 }
}

fn cf_performed_drop_effect() -> u16 {
    unsafe { RegisterClipboardFormatA(windows::core::s!("Performed DropEffect")) as u16 }
}

fn cf_logical_performed_drop_effect() -> u16 {
    unsafe { RegisterClipboardFormatA(windows::core::s!("Logical Performed DropEffect")) as u16 }
}

// ================================================================
// CDataObject: IDataObject 実装
// ================================================================

struct DataState {
    hdrop_bytes: Vec<u8>,
    preferred: u32, // DROPEFFECT_COPY | DROPEFFECT_MOVE
    /// ドロップターゲットが SetData(PerformedDropEffect) で書き込んだ値
    performed: Option<u32>,
    /// ドロップターゲットが SetData(LogicalPerformedDropEffect) で書き込んだ値
    logical_performed: Option<u32>,
}

#[implement(IDataObject)]
struct CDataObject {
    inner: Arc<Mutex<DataState>>,
    cf_pref: u16,
    cf_perf: u16,
    cf_log_perf: u16,
}

impl CDataObject {
    fn new(hdrop_bytes: Vec<u8>, preferred: u32) -> Self {
        Self {
            inner: Arc::new(Mutex::new(DataState {
                hdrop_bytes,
                preferred,
                performed: None,
                logical_performed: None,
            })),
            cf_pref: cf_preferred_drop_effect(),
            cf_perf: cf_performed_drop_effect(),
            cf_log_perf: cf_logical_performed_drop_effect(),
        }
    }

    fn supported_formats(&self) -> [FORMATETC; 2] {
        [
            FORMATETC {
                cfFormat: CF_HDROP.0,
                ptd: std::ptr::null_mut(),
                dwAspect: DVASPECT_CONTENT.0,
                lindex: -1,
                tymed: TYMED_HGLOBAL.0 as u32,
            },
            FORMATETC {
                cfFormat: self.cf_pref,
                ptd: std::ptr::null_mut(),
                dwAspect: DVASPECT_CONTENT.0,
                lindex: -1,
                tymed: TYMED_HGLOBAL.0 as u32,
            },
        ]
    }
}

impl IDataObject_Impl for CDataObject_Impl {
    fn GetData(&self, pformatetc: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
        let fe = unsafe { &*pformatetc };
        if fe.dwAspect != DVASPECT_CONTENT.0 || (fe.tymed & TYMED_HGLOBAL.0 as u32) == 0 {
            return Err(DV_E_FORMATETC.into());
        }
        // 中間 Vec へ clone せず、ロック保持のまま直接 HGLOBAL へコピーする
        // (1 万件ドラッグの CF_HDROP ~1.6MB で GetData ごとの二重コピーだった。
        //  alloc_hglobal_from_bytes はロックを取らないためデッドロックしない)
        let state = self.inner.lock().unwrap();
        unsafe {
            let alloc_result = if fe.cfFormat == CF_HDROP.0 {
                alloc_hglobal_from_bytes(&state.hdrop_bytes)
            } else if fe.cfFormat == self.cf_pref {
                alloc_hglobal_from_bytes(&state.preferred.to_le_bytes())
            } else {
                return Err(DV_E_FORMATETC.into());
            };
            drop(state);
            let h = alloc_result.map_err(|e| {
                windows::core::Error::new(windows::Win32::Foundation::E_OUTOFMEMORY, e.to_string())
            })?;
            let mut medium: STGMEDIUM = std::mem::zeroed();
            medium.tymed = TYMED_HGLOBAL.0 as u32;
            medium.u = STGMEDIUM_0 {
                hGlobal: windows::Win32::Foundation::HGLOBAL(h.0),
            };
            medium.pUnkForRelease = std::mem::ManuallyDrop::new(None);
            Ok(medium)
        }
    }

    fn GetDataHere(
        &self,
        _pformatetc: *const FORMATETC,
        _pmedium: *mut STGMEDIUM,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, pformatetc: *const FORMATETC) -> windows::core::HRESULT {
        let fe = unsafe { &*pformatetc };
        if fe.dwAspect != DVASPECT_CONTENT.0 || (fe.tymed & TYMED_HGLOBAL.0 as u32) == 0 {
            return DV_E_TYMED;
        }
        if fe.cfFormat == CF_HDROP.0 || fe.cfFormat == self.cf_pref {
            S_OK
        } else {
            DV_E_FORMATETC
        }
    }

    fn GetCanonicalFormatEtc(
        &self,
        _pformatectin: *const FORMATETC,
        pformatetcout: *mut FORMATETC,
    ) -> windows::core::HRESULT {
        unsafe {
            if !pformatetcout.is_null() {
                (*pformatetcout).ptd = std::ptr::null_mut();
            }
        }
        windows::Win32::Foundation::DATA_S_SAMEFORMATETC
    }

    fn SetData(
        &self,
        pformatetc: *const FORMATETC,
        pmedium: *const STGMEDIUM,
        frelease: BOOL,
    ) -> windows::core::Result<()> {
        let fe = unsafe { &*pformatetc };
        let medium = unsafe { &*pmedium };
        if (fe.tymed & TYMED_HGLOBAL.0 as u32) == 0 {
            return Err(DV_E_TYMED.into());
        }
        let value = unsafe { read_hglobal_dword(medium) };
        if let Some(v) = value {
            let mut state = self.inner.lock().unwrap();
            if fe.cfFormat == self.cf_perf {
                state.performed = Some(v);
            } else if fe.cfFormat == self.cf_log_perf {
                state.logical_performed = Some(v);
            }
        }
        // COM 契約: fRelease=TRUE なら所有権はこちらへ移り、解放責務を負う。
        // 値はコピー済みなので即解放する (放置すると HGLOBAL が呼び出しごとにリーク)
        if frelease.as_bool() {
            unsafe { ReleaseStgMedium(pmedium as *mut STGMEDIUM) };
        }
        Ok(())
    }

    fn EnumFormatEtc(&self, dwdirection: u32) -> windows::core::Result<IEnumFORMATETC> {
        if dwdirection != DATADIR_GET.0 as u32 {
            return Err(E_NOTIMPL.into());
        }
        let formats = self.supported_formats();
        unsafe { SHCreateStdEnumFmtEtc(&formats) }
    }

    fn DAdvise(
        &self,
        _pformatetc: *const FORMATETC,
        _advf: u32,
        _padvsink: Option<&IAdviseSink>,
    ) -> windows::core::Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _dwconnection: u32) -> windows::core::Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> windows::core::Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

// ================================================================
// CDropSource: IDropSource 実装
// ================================================================

#[implement(IDropSource)]
struct CDropSource {
    /// ドラッグ継続判定のボタンマスク (MK_LBUTTON or MK_RBUTTON)。
    button_mask: u32,
}

impl IDropSource_Impl for CDropSource_Impl {
    fn QueryContinueDrag(
        &self,
        fescapepressed: BOOL,
        grfkeystate: MODIFIERKEYS_FLAGS,
    ) -> windows::core::HRESULT {
        if fescapepressed.as_bool() {
            return windows::Win32::Foundation::DRAGDROP_S_CANCEL;
        }
        // 対象ボタンが離されたらドロップ
        if (grfkeystate.0 & self.button_mask) == 0 {
            return windows::Win32::Foundation::DRAGDROP_S_DROP;
        }
        S_OK
    }

    fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> windows::core::HRESULT {
        windows::Win32::Foundation::DRAGDROP_S_USEDEFAULTCURSORS
    }
}

// ================================================================
// 公開 API
// ================================================================

/// 件数 / payload サイズの guard (UI フリーズ・OOM 防止)。
/// 超過時は `Err` を返し、呼出側で「件数が多すぎます」ステータスを出す。
const MAX_PATHS: usize = 10_000;
const MAX_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;

pub fn start_drag(req: DragRequest) -> AppResult<DragOutcome> {
    if req.paths.is_empty() {
        return Ok(DragOutcome::None);
    }
    if req.paths.len() > MAX_PATHS {
        return Err(AppError::Win32(format!(
            "ドラッグ対象が多すぎます ({} 件 > 上限 {})",
            req.paths.len(),
            MAX_PATHS
        )));
    }
    let hdrop = build_hdrop_bytes(&req.paths);
    if hdrop.len() > MAX_PAYLOAD_BYTES {
        return Err(AppError::Win32(format!(
            "ドラッグデータが大きすぎます ({} bytes > 上限 {})",
            hdrop.len(),
            MAX_PAYLOAD_BYTES
        )));
    }

    let preferred_dword = match req.preferred {
        PreferredEffect::Copy => DROPEFFECT_COPY.0,
        PreferredEffect::Move => DROPEFFECT_MOVE.0,
    };
    let allowed = DROPEFFECT(DROPEFFECT_COPY.0 | DROPEFFECT_MOVE.0);

    let data_obj_impl = CDataObject::new(hdrop, preferred_dword);
    let state_handle = Arc::clone(&data_obj_impl.inner);
    let data_obj: IDataObject = data_obj_impl.into();
    let button_mask = match req.button {
        DragButton::Left => MK_LBUTTON.0,
        DragButton::Right => MK_RBUTTON.0,
    };
    let drop_src: IDropSource = CDropSource { button_mask }.into();

    let mut effect = DROPEFFECT_NONE;
    let hr = unsafe {
        DoDragDrop(
            &data_obj,
            &drop_src,
            allowed,
            &mut effect as *mut DROPEFFECT,
        )
    };

    // hr: DRAGDROP_S_DROP (drop)| DRAGDROP_S_CANCEL (esc) | エラー
    if hr == windows::Win32::Foundation::DRAGDROP_S_CANCEL {
        return Ok(DragOutcome::Cancel);
    }
    if hr.is_err() {
        return Ok(DragOutcome::Error(format!("DoDragDrop hr=0x{:08X}", hr.0)));
    }

    // pdwEffect 判定 + PerformedDropEffect 一致確認
    let state = state_handle.lock().unwrap();
    let performed = state.performed.or(state.logical_performed);
    drop(state);

    // 診断ログ: ドロップ先 (Explorer / ブラウザ等) が返した効果を確認する。
    // デバッグビルド限定 (リリースでは出さない)。`cargo run` 等のターミナル
    // 起動時のみ見える。挙動を調べたいときはここを見る。
    #[cfg(debug_assertions)]
    eprintln!(
        "[ole_dnd] DoDragDrop 完了: pdwEffect=0x{:X} performed={performed:?}",
        effect.0
    );

    if (effect.0 & DROPEFFECT_MOVE.0) != 0 {
        // pdwEffect が MOVE でも、元を削除してよいのは「ドロップ先が
        // CFSTR_PERFORMEDDROPEFFECT で MOVE を **明示**した」ときだけ。
        // Performed が来ない (None) 相手 — 代表例がブラウザの添付 — は移動を
        // 実行していないので削除しない (= 添付/コピー扱い)。これがデータ損失防止の要。
        //
        // なお Explorer 等が最適化ムーブで既に元を移動済みでも performed=MOVE を
        // 返すため delete=true になるが、その場合は呼出側が「元は既に存在しない」
        // ことを確認してゴミ箱送りを空振りさせる (二重削除にならない)。
        let delete = matches!(performed, Some(p) if (p & DROPEFFECT_MOVE.0) != 0);
        Ok(DragOutcome::Move {
            delete_source: delete,
        })
    } else if (effect.0 & DROPEFFECT_COPY.0) != 0 {
        Ok(DragOutcome::Copy)
    } else {
        Ok(DragOutcome::None)
    }
}

// 型推論補助は不要

// ================================================================
// OLE 初期化 (UI スレッド 1 回のみ)
// ================================================================

static OLE_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// ドロップ時点の物理修飾キー状態 (Ctrl, Shift) を返す。
///
/// エクスプローラ等の外部からのドラッグ中はキーボードフォーカスが相手側にあり、
/// GUI フレームワークのイベント追跡では修飾キーが更新されない (Ctrl+ドロップの
/// 強制コピーが効かない)。そのため GetKeyState で物理状態を直接読む。
/// ドロップ処理は OLE の Drop コールバック中に同期実行されるため、この時点の
/// キー状態 = ドロップ瞬間のキー状態とみなせる。
pub fn drop_modifiers() -> (bool, bool) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_SHIFT};
    unsafe {
        let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
        let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
        (ctrl, shift)
    }
}

/// プロセス起動時 (UI スレッド) に 1 回だけ呼ぶ。
///
/// `OleInitialize` が成功すれば `is_ole_available()` が true を返し、
/// `start_drag()` を呼び出せるようになる。
/// 失敗 (例: `RPC_E_CHANGED_MODE` で別 apartment 既に初期化済み) のときは
/// false のまま `start_drag()` 呼出が抑止される。
pub fn init_ole() {
    let hr = unsafe { OleInitialize(None) };
    if hr.is_ok() {
        OLE_AVAILABLE.store(true, Ordering::Release);
    } else {
        OLE_AVAILABLE.store(false, Ordering::Release);
        let err = hr.err();
        eprintln!(
            "[ole_dnd] OleInitialize failed: {:?} (外部 D&D 送信は無効化されます)",
            err
        );
    }
}

/// プロセス終了時に呼ぶ (任意)。
pub fn shutdown_ole() {
    if OLE_AVAILABLE.swap(false, Ordering::AcqRel) {
        unsafe { OleUninitialize() };
    }
}

pub fn is_ole_available() -> bool {
    OLE_AVAILABLE.load(Ordering::Acquire)
}

// ================================================================
// 受信側: IDropTarget 実装 (Phase 2-recv)
// ================================================================

/// 各 drag メソッドが返す「希望 effect」を計算するクロージャ。
pub type DropEffectFn = Box<dyn Fn(&[PathBuf], POINTL, u32, u32) -> u32 + Send + Sync>;
/// `DragLeave` 用 (引数なし)。
pub type DropLeaveFn = Box<dyn Fn() + Send + Sync>;

/// IDropTarget の各メソッドから呼ばれるコールバック群。
///
/// すべてのコールバックは UI スレッド (OleInitialize したスレッド) で呼ばれる前提。
/// `*pdwEffect` は呼び出し元 (OS) から渡される **許可される効果のマスク** で、
/// コールバックは「希望する効果」を返し、COM 層側で `desired & allowed_mask`
/// を取って書き戻す。
pub struct DropTargetCallbacks {
    /// `IDropTarget::DragEnter` — paths は IDataObject から抽出済 (CF_HDROP)、
    /// pt はスクリーン座標。戻り値は希望 effect (DROPEFFECT_COPY / MOVE / NONE)。
    pub on_enter: DropEffectFn,
    /// `IDropTarget::DragOver` — paths は前回 enter 時のキャッシュ。
    pub on_over: DropEffectFn,
    /// `IDropTarget::DragLeave` — ハイライト解除のみ。
    pub on_leave: DropLeaveFn,
    /// `IDropTarget::Drop` — paths は前回 enter のキャッシュまたは再抽出。
    /// コールバック内で fops 実行・reload・status_msg 更新を行う。
    pub on_drop: DropEffectFn,
}

#[implement(IDropTarget)]
struct CDropTarget {
    callbacks: Arc<DropTargetCallbacks>,
    // Arc 共有 — DragOver はマウス移動毎に呼ばれるため、Vec の deep clone だと
    // 1,000 ファイルのドラッグで毎イベント ~1,000 alloc になる
    cached_paths: Mutex<Arc<Vec<PathBuf>>>,
    registered_thread: u32,
}

impl CDropTarget {
    fn new(callbacks: Arc<DropTargetCallbacks>) -> Self {
        let tid = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
        Self {
            callbacks,
            cached_paths: Mutex::new(Arc::new(Vec::new())),
            registered_thread: tid,
        }
    }

    fn assert_thread(&self) {
        let cur = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
        debug_assert_eq!(
            cur, self.registered_thread,
            "IDropTarget called from non-UI thread"
        );
    }
}

impl IDropTarget_Impl for CDropTarget_Impl {
    fn DragEnter(
        &self,
        pdataobj: Option<&IDataObject>,
        grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        self.assert_thread();
        let allowed = unsafe { (*pdweffect).0 };
        let pt = *pt;
        let key = grfkeystate.0;
        let data = match pdataobj {
            Some(d) => d,
            None => {
                unsafe { *pdweffect = DROPEFFECT_NONE };
                return Ok(());
            }
        };
        // CF_HDROP がない場合は受け付けない (シェルアイテム形式のみ等)
        let paths = match extract_hdrop_paths(data) {
            Ok(p) if !p.is_empty() => p,
            _ => {
                unsafe { *pdweffect = DROPEFFECT_NONE };
                return Ok(());
            }
        };
        let paths = Arc::new(paths);
        {
            let mut cache = self.cached_paths.lock().unwrap();
            *cache = Arc::clone(&paths);
        }
        let callbacks = Arc::clone(&self.callbacks);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (callbacks.on_enter)(&paths, pt, key, allowed)
        }));
        let desired = result.unwrap_or(DROPEFFECT_NONE.0);
        unsafe { *pdweffect = DROPEFFECT(desired & allowed) };
        Ok(())
    }

    fn DragOver(
        &self,
        grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        self.assert_thread();
        let allowed = unsafe { (*pdweffect).0 };
        let pt = *pt;
        let key = grfkeystate.0;
        let paths = { Arc::clone(&self.cached_paths.lock().unwrap()) };
        if paths.is_empty() {
            unsafe { *pdweffect = DROPEFFECT_NONE };
            return Ok(());
        }
        let callbacks = Arc::clone(&self.callbacks);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (callbacks.on_over)(&paths, pt, key, allowed)
        }));
        let desired = result.unwrap_or(DROPEFFECT_NONE.0);
        unsafe { *pdweffect = DROPEFFECT(desired & allowed) };
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        self.assert_thread();
        {
            *self.cached_paths.lock().unwrap() = Arc::new(Vec::new());
        }
        let callbacks = Arc::clone(&self.callbacks);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (callbacks.on_leave)()));
        Ok(())
    }

    fn Drop(
        &self,
        pdataobj: Option<&IDataObject>,
        grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        self.assert_thread();
        let allowed = unsafe { (*pdweffect).0 };
        let pt = *pt;
        let key = grfkeystate.0;
        // Drop 時は cached_paths が空でも IDataObject から再抽出する
        // (一部の Source は Drop で初めて確定したパスを送る場合がある)
        let paths = {
            let cached = Arc::clone(&self.cached_paths.lock().unwrap());
            if !cached.is_empty() {
                cached
            } else if let Some(d) = pdataobj {
                Arc::new(extract_hdrop_paths(d).unwrap_or_default())
            } else {
                Arc::new(Vec::new())
            }
        };
        if paths.is_empty() {
            unsafe { *pdweffect = DROPEFFECT_NONE };
            *self.cached_paths.lock().unwrap() = Arc::new(Vec::new());
            return Ok(());
        }
        let callbacks = Arc::clone(&self.callbacks);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (callbacks.on_drop)(&paths, pt, key, allowed)
        }));
        let desired = result.unwrap_or(DROPEFFECT_NONE.0);
        unsafe { *pdweffect = DROPEFFECT(desired & allowed) };
        *self.cached_paths.lock().unwrap() = Arc::new(Vec::new());
        Ok(())
    }
}

/// `RegisterDragDrop` した IDropTarget を RAII で解放するハンドル。
///
/// Drop 時に `IsWindow(hwnd)` を確認して `RevokeDragDrop` を呼ぶ。
/// winit が WM_DESTROY 側で再度 RevokeDragDrop しても、二重 Revoke は
/// `DRAGDROP_E_NOTREGISTERED` で無視される (実害なし)。
pub struct DropTargetRegistration {
    hwnd: HWND,
    /// keep-alive。Drop 時に解放される。
    _target: IDropTarget,
}

impl DropTargetRegistration {
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }
}

impl Drop for DropTargetRegistration {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(self.hwnd).as_bool() {
                let hr = RevokeDragDrop(self.hwnd);
                if let Err(e) = hr {
                    eprintln!("[ole_dnd] RevokeDragDrop failed (ignored): {:?}", e);
                }
            }
        }
    }
}

/// 自前 IDropTarget を `hwnd` に登録する。
///
/// winit (floem-winit) が既に登録した IDropTarget は `RevokeDragDrop` で外す。
/// 戻り値の `DropTargetRegistration` を破棄すると登録も解除される (RAII)。
///
/// 呼び出しは **OleInitialize 済の UI スレッド** から行うこと。
pub fn register_drop_target(
    hwnd: HWND,
    callbacks: DropTargetCallbacks,
) -> AppResult<DropTargetRegistration> {
    unsafe {
        // 既存登録 (winit) を外す。未登録の場合は DRAGDROP_E_NOTREGISTERED が返るが無視。
        let _ = RevokeDragDrop(hwnd);
        let target_impl = CDropTarget::new(Arc::new(callbacks));
        let target: IDropTarget = target_impl.into();
        RegisterDragDrop(hwnd, &target)
            .map_err(|e| AppError::Win32(format!("RegisterDragDrop: {e}")))?;
        Ok(DropTargetRegistration {
            hwnd,
            _target: target,
        })
    }
}
