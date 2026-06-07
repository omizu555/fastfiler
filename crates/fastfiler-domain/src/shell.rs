// Phase 3: シェル統合
//
// - open_with_shell: ShellExecuteW("open", path) — 既定アプリで開く
// - reveal_in_explorer: explorer.exe /select で「エクスプローラで表示」
// - show_properties: SHObjectProperties でプロパティダイアログ
// - show_shell_context_menu: IContextMenu による Windows 標準右クリックメニュー
//   (ADR 0007: Shift+右クリックで呼び出す想定。GUI 非依存 — HWND は isize で受ける)

use crate::error::{AppError, AppResult};

/// Windows 標準のシェルコンテキストメニュー (IContextMenu) を現在のカーソル位置に
/// 表示し、選択されたコマンドを実行する。ブロッキング (メニューを閉じるまで戻らない)。
///
/// * `hwnd` - オーナーウィンドウの HWND (isize)。
/// * `paths` - 対象 (同一フォルダ内の複数選択可)。
///
/// 制限: IContextMenu2/3 のメッセージ転送は未実装のため、一部の動的サブメニュー
/// (「新規作成」等) は表示されない場合がある。
pub fn show_shell_context_menu(hwnd: isize, paths: Vec<String>) -> AppResult<()> {
    #[cfg(windows)]
    {
        unsafe { shell_context_menu_impl(hwnd, &paths) }
    }
    #[cfg(not(windows))]
    {
        let _ = (hwnd, paths);
        Err(AppError::Other("Windows でのみ利用可能".into()))
    }
}

#[cfg(windows)]
unsafe fn shell_context_menu_impl(hwnd_raw: isize, paths: &[String]) -> AppResult<()> {
    use windows::Win32::Foundation::{HWND, POINT};
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{
        CMF_NORMAL, CMINVOKECOMMANDINFO, IContextMenu, ILFindLastID, ILFree, IShellFolder,
        SHBindToParent, SHParseDisplayName,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreatePopupMenu, DestroyMenu, GetCursorPos, SW_SHOWNORMAL, TPM_RETURNCMD, TPM_RIGHTBUTTON,
        TrackPopupMenuEx,
    };
    use windows::core::{PCSTR, PCWSTR};

    if paths.is_empty() {
        return Err(AppError::Other("対象がありません".into()));
    }
    let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    // 各パス → 絶対 PIDL (失敗したらそこまでに確保した分を解放して返す)。
    let mut abs_pidls: Vec<*mut ITEMIDLIST> = Vec::with_capacity(paths.len());
    let free_all = |pidls: &[*mut ITEMIDLIST]| {
        for &p in pidls {
            unsafe { ILFree(Some(p)) };
        }
    };
    for p in paths {
        let wide = to_wide(p);
        let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        if let Err(e) =
            SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None)
        {
            free_all(&abs_pidls);
            return Err(AppError::Win32(format!("SHParseDisplayName({p}): {e}")));
        }
        abs_pidls.push(pidl);
    }

    let result = (|| -> AppResult<()> {
        // 先頭の親フォルダへバインド (複数選択は同一フォルダ前提)。
        let parent: IShellFolder = SHBindToParent(abs_pidls[0], None)
            .map_err(|e| AppError::Win32(format!("SHBindToParent: {e}")))?;

        // 各項目の相対 PIDL (最後の要素)。
        let rel_pidls: Vec<*const ITEMIDLIST> = abs_pidls
            .iter()
            .map(|&p| ILFindLastID(p) as *const ITEMIDLIST)
            .collect();

        // IContextMenu を取得。
        let pcm: IContextMenu = parent
            .GetUIObjectOf(hwnd, &rel_pidls, None)
            .map_err(|e| AppError::Win32(format!("GetUIObjectOf: {e}")))?;

        let menu = CreatePopupMenu().map_err(|e| AppError::Win32(format!("{e}")))?;
        let menu_result = (|| -> AppResult<()> {
            pcm.QueryContextMenu(menu, 0, 1, 0x7fff, CMF_NORMAL)
                .map_err(|e| AppError::Win32(format!("QueryContextMenu: {e}")))?;

            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let cmd =
                TrackPopupMenuEx(menu, (TPM_RETURNCMD | TPM_RIGHTBUTTON).0, pt.x, pt.y, hwnd, None);

            let id = cmd.0;
            if id > 0 {
                let info = CMINVOKECOMMANDINFO {
                    cbSize: std::mem::size_of::<CMINVOKECOMMANDINFO>() as u32,
                    lpVerb: PCSTR((id as usize - 1) as *const u8), // MAKEINTRESOURCE
                    nShow: SW_SHOWNORMAL.0,
                    hwnd,
                    ..Default::default()
                };
                pcm.InvokeCommand(&info)
                    .map_err(|e| AppError::Win32(format!("InvokeCommand: {e}")))?;
            }
            Ok(())
        })();
        let _ = DestroyMenu(menu);
        menu_result
    })();

    free_all(&abs_pidls);
    result
}

pub fn open_with_shell(path: String) -> AppResult<()> {
    #[cfg(windows)]
    {
        // ディスクイメージ (iso/img/vhd/vhdx) は "mount" verb を使用し
        // Windows エクスプローラと同じ「マウント」挙動にする (Win8 以降)。
        // Office テンプレート (xltx/xltm/dotx/dotm/potx/potm 等) は verb を
        // None にしてレジストリ上の既定 verb (通常 "new" = テンプレートから新規作成)
        // を使用する。"open" を明示するとテンプレート自体が編集モードで開いてしまう。
        // それ以外は従来通り "open" verb を使用。
        let ext = std::path::Path::new(&path)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        let verb: Option<&str> = match ext.as_str() {
            "iso" | "img" | "vhd" | "vhdx" => Some("mount"),
            "xltx" | "xltm" | "xlt" | "dotx" | "dotm" | "dot" | "potx" | "potm" | "pot" => None,
            _ => Some("open"),
        };
        // 作業ディレクトリはファイルの親フォルダを指定する。
        // 指定しないと FastFiler の cwd が継承され、.bat 等で相対パスが
        // 想定外の場所基準になる (エクスプローラのダブルクリック動作と揃える)。
        //
        // 例外:
        // - .lnk / .url: ショートカット自身が「作業フォルダ」を持つので
        //   ここで上書きせず shell に解決を任せる。
        // - ディレクトリ: explorer 起動で cwd は無意味なので None にする
        //   (親フォルダを渡してしまうと意味的に紛らわしい)。
        let pth = std::path::Path::new(&path);
        let skip_cwd = matches!(ext.as_str(), "lnk" | "url") || pth.is_dir();
        let cwd = if skip_cwd {
            None
        } else {
            pth.parent().and_then(|p| p.to_str()).map(|s| s.to_owned())
        };
        win::shell_exec(verb, &path, None, cwd.as_deref())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(AppError::NotSupported("shell open is windows-only".into()))
    }
}

/// `open_with_shell` の非同期版: 専用 STA スレッドで実行し JoinHandle を返す。
///
/// ShellExecuteW は関連付け先 (Office の DDE 起動等) によってメッセージポンプを
/// 回すことがあり、GUI の update サイクル中 (UI スレッド) に直接呼ぶと wndproc
/// 再入で "RefCell already borrowed" panic になる (DoDragDrop と同じ類型)。
/// UI スレッドからはこちらを使い、結果が必要なら別スレッドで join すること。
pub fn open_with_shell_async(path: String) -> std::thread::JoinHandle<AppResult<()>> {
    #[cfg(windows)]
    {
        win::open_with_shell_sta_thread(path)
    }
    #[cfg(not(windows))]
    {
        std::thread::spawn(move || open_with_shell(path))
    }
}

/// 任意の実行ファイルを ShellExecuteW (既定 verb) で起動する。専用 STA スレッドで
/// 実行して join するため、呼び出し元は UI スレッド以外であること。
///
/// エクスプローラと同じ起動経路なので、powershell 等のコンソールアプリは
/// 自分の新しいコンソールへ正しく接続される。CreateProcess 直接起動だと
/// 親の標準ハンドルが継承され、「新しいウィンドウは開くが入出力は親側」と
/// なって開いた直後に閉じてしまう (デバッグ実行時に顕在化)。
pub fn launch_with_shell(
    exec: String,
    params: Option<String>,
    cwd: Option<String>,
) -> AppResult<()> {
    #[cfg(windows)]
    {
        win::launch_sta_thread(exec, params, cwd)
            .join()
            .map_err(|_| AppError::Other("shell 起動スレッドが異常終了しました".into()))?
    }
    #[cfg(not(windows))]
    {
        let _ = (exec, params, cwd);
        Err(AppError::NotSupported("shell launch is windows-only".into()))
    }
}

pub fn reveal_in_explorer(path: String) -> AppResult<()> {
    #[cfg(windows)]
    {
        // explorer.exe /select,"path" でファイルを選択状態でフォルダを開く
        let arg = format!("/select,\"{}\"", path);
        win::shell_exec(Some("open"), "explorer.exe", Some(&arg), None)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(AppError::NotSupported("reveal is windows-only".into()))
    }
}

pub fn show_properties(path: String) -> AppResult<()> {
    #[cfg(windows)]
    {
        win::show_properties(&path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(AppError::NotSupported("properties is windows-only".into()))
    }
}

/// プロパティダイアログを専用 STA スレッドで開く (UI スレッド再入対策)。
/// `SHObjectProperties` はダイアログを閉じるまでブロックするため、呼び出し側は
/// 戻り値の JoinHandle を join せず投げっぱなしにする (open_with_shell_async と同様)。
pub fn show_properties_async(path: String) -> std::thread::JoinHandle<AppResult<()>> {
    #[cfg(windows)]
    {
        win::show_properties_thread(path)
    }
    #[cfg(not(windows))]
    {
        std::thread::spawn(move || show_properties(path))
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::thread;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::UI::Shell::{SHObjectProperties, ShellExecuteW, SHOP_FILEPATH};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub fn shell_exec(
        op: Option<&str>,
        file: &str,
        args: Option<&str>,
        cwd: Option<&str>,
    ) -> AppResult<()> {
        let op_w = op.map(wide);
        let file_w = wide(file);
        let args_w = args.map(wide);
        let cwd_w = cwd.map(wide);
        let hinst = unsafe {
            ShellExecuteW(
                None,
                op_w.as_ref()
                    .map(|w| PCWSTR(w.as_ptr()))
                    .unwrap_or(PCWSTR::null()),
                PCWSTR(file_w.as_ptr()),
                args_w
                    .as_ref()
                    .map(|w| PCWSTR(w.as_ptr()))
                    .unwrap_or(PCWSTR::null()),
                cwd_w
                    .as_ref()
                    .map(|w| PCWSTR(w.as_ptr()))
                    .unwrap_or(PCWSTR::null()),
                SW_SHOWNORMAL,
            )
        };
        if hinst.0 as isize <= 32 {
            return Err(AppError::Win32(format!(
                "ShellExecuteW failed ({})",
                hinst.0 as isize
            )));
        }
        Ok(())
    }

    /// 任意の実行ファイルを ShellExecuteW で起動する専用 STA スレッドを立てる。
    pub fn launch_sta_thread(
        exec: String,
        params: Option<String>,
        cwd: Option<String>,
    ) -> thread::JoinHandle<AppResult<()>> {
        thread::spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
            }
            let r = shell_exec(None, &exec, params.as_deref(), cwd.as_deref());
            unsafe { CoUninitialize() };
            r
        })
    }

    /// `open_with_shell` を専用 STA スレッドで実行する (UI スレッド再入対策)。
    pub fn open_with_shell_sta_thread(path: String) -> thread::JoinHandle<AppResult<()>> {
        thread::spawn(move || {
            // ShellExecuteW はシェル拡張へ委譲するため STA を要求する。
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
            }
            let r = super::open_with_shell(path);
            unsafe { CoUninitialize() };
            r
        })
    }

    /// プロパティ表示を行う STA スレッドを生成して返す (join は呼び出し側に委ねる)。
    pub fn show_properties_thread(path: String) -> thread::JoinHandle<AppResult<()>> {
        thread::spawn(move || -> AppResult<()> {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
                let path_w = wide(&path);
                let res = SHObjectProperties(
                    HWND::default(),
                    SHOP_FILEPATH,
                    PCWSTR(path_w.as_ptr()),
                    PCWSTR::null(),
                );
                CoUninitialize();
                if res.as_bool() {
                    Ok(())
                } else {
                    Err(AppError::Win32("SHObjectProperties returned FALSE".into()))
                }
            }
        })
    }

    pub fn show_properties(path: &str) -> AppResult<()> {
        show_properties_thread(path.to_owned())
            .join()
            .map_err(|_| AppError::Win32("properties thread panicked".into()))?
    }
}
