// FastFiler ライブラリエントリ
//
// Phase 0-7 全機能のコマンド登録ハブ。

mod fs_service;
mod file_ops;
mod file_jobs;
mod watcher;
mod shell;
mod thumbnail;
mod preview;
mod search;
mod everything;
mod plugin;
mod term;
mod win_shell;
mod ole_dnd;
mod win_clipboard;
mod templates;
mod shell_assoc;
mod error;

pub use error::AppError;

use std::sync::Mutex;
use tauri::{Emitter, Manager};

/// v1.12: 起動引数で渡されたフォルダパスを保持 (フロントが onMount で取得)
#[derive(Default)]
pub struct InitialPath(pub Mutex<Option<String>>);

/// argv からディレクトリ パスらしき引数を抽出 (最初に見つかった有効なディレクトリ)
fn extract_dir_arg(args: &[String]) -> Option<String> {
    // args[0] は実行ファイル パス
    for a in args.iter().skip(1) {
        if a.starts_with('-') {
            continue;
        }
        let p = std::path::Path::new(a);
        if p.is_dir() {
            return Some(p.to_string_lossy().into_owned());
        }
    }
    None
}

#[tauri::command]
fn cli_initial_path(state: tauri::State<'_, InitialPath>) -> Option<String> {
    state.0.lock().ok().and_then(|mut g| g.take())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
            // v1.12: 2 個目以降のプロセスから受け取った引数 → 新規タブで開く
            if let Some(dir) = extract_dir_arg(&args) {
                let _ = app.emit("ff://open-path", dir);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let watcher_state = watcher::WatcherState::new(app.handle().clone());
            app.manage(watcher_state);
            app.manage(search::SearchState::default());
            app.manage(file_jobs::JobRegistry::default());
            // v1.12: 初回起動時の argv を保存 (フロント onMount → cli_initial_path で取得)
            let initial = InitialPath(Mutex::new(
                extract_dir_arg(&std::env::args().collect::<Vec<_>>()),
            ));
            app.manage(initial);
            term::register(app.handle());
            ole_dnd::register(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // FS
            fs_service::list_dir,
            fs_service::list_dirs,
            fs_service::list_drives,
            fs_service::stat_path,
            fs_service::home_dir,
            fs_service::disk_free,
            // file ops
            file_ops::create_dir,
            file_ops::rename_path,
            file_ops::delete_path,
            file_ops::delete_to_trash,
            file_ops::copy_path,
            file_ops::move_path,
            // file jobs (with progress)
            file_jobs::job_copy,
            file_jobs::job_move,
            file_jobs::job_delete,
            file_jobs::cancel_job,
            // watcher
            watcher::watch_dir,
            watcher::unwatch_dir,
            // shell
            shell::open_with_shell,
            shell::reveal_in_explorer,
            shell::show_properties,
            // thumbnail
            thumbnail::get_thumbnail,
            // preview
            preview::read_text_preview,
            // search
            search::search_files,
            search::search_cancel,
            search::everything_ping,
            // plugins
            plugin::list_plugins,
            plugin::list_plugins_with_status,
            plugin::plugins_dir_path,
            plugin::plugin_invoke,
            plugin::import_plugin_zip,
            plugin::delete_plugin,
            // terminal
            term::term_open,
            term::term_write,
            term::term_resize,
            term::term_close,
            // v4.0
            win_shell::shell_menu_show,
            win_shell::shell_menu_query,
            win_shell::shell_menu_invoke,
            ole_dnd::ole_dnd_register,
            ole_dnd::ole_dnd_start_drag,
            win_clipboard::clipboard_write_paths,
            win_clipboard::clipboard_read_paths,
            // templates (v1.11)
            templates::templates_dir,
            templates::list_templates,
            templates::create_empty_file,
            templates::create_file_from_template,
            // v1.12: シェル統合
            cli_initial_path,
            shell_assoc::shell_assoc_status,
            shell_assoc::shell_assoc_enable,
            shell_assoc::shell_assoc_disable,
            shell_assoc::shell_assoc_diagnose,
        ])
        .run(tauri::generate_context!())
        .expect("error while running FastFiler");
}
