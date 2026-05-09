// Phase 2B-4: 純粋ロジックは fastfiler-domain::watcher に移動済。
// src-tauri 側は AppHandle をひも付ける WatcherState と #[tauri::command] のみ。

use crate::error::AppResult;
use crate::events::{self, EventSink};
use fastfiler_domain::watcher as core;
use std::sync::Arc;
use tauri::{AppHandle, State};

#[allow(unused_imports)]
pub use core::{FsChange, WatcherCore};

/// Tauri 用の登録時状態 (AppHandle 同梱版)。
pub struct WatcherState {
    core: Arc<WatcherCore>,
    app: AppHandle,
}

impl WatcherState {
    pub fn new(app: AppHandle) -> Self {
        Self { core: Arc::new(WatcherCore::default()), app }
    }
}

#[tauri::command]
pub fn watch_dir(path: String, state: State<'_, WatcherState>) -> AppResult<()> {
    let sink: Arc<dyn EventSink> = Arc::new(events::tauri_sink(state.app.clone()));
    state.core.watch_with_sink(path, sink)
}

#[tauri::command]
pub fn unwatch_dir(path: String, state: State<'_, WatcherState>) -> AppResult<()> {
    state.core.unwatch(&path);
    Ok(())
}
