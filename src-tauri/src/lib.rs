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
mod error;

pub use error::AppError;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let watcher_state = watcher::WatcherState::new(app.handle().clone());
            app.manage(watcher_state);
            app.manage(search::SearchState::default());
            app.manage(file_jobs::JobRegistry::default());
            term::register(app.handle());
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running FastFiler");
}
