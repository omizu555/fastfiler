// Phase 1: ディレクトリ監視 (notify クレート, ReadDirectoryChangesW)
//
// Phase 2A: Tauri 依存を局所化。
//  - 純粋な監視ロジックは `watch_with_sink` / `unwatch` に分離 (EventSink 経由)
//  - `#[tauri::command]` 版は AppHandle を sink に変換して薄くラップ
//  - 状態保持の `WatcherCore` は Tauri 非依存で floem 版から流用可能

use crate::error::{AppError, AppResult};
use crate::events::{self, EventSink};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

#[derive(Serialize, Clone)]
pub struct FsChange {
    pub path: String,
    pub kind: &'static str,
}

/// Tauri 非依存の監視コア。
#[derive(Default)]
pub struct WatcherCore {
    inner: Mutex<HashMap<String, RecommendedWatcher>>,
}

impl WatcherCore {
    pub fn watch_with_sink(&self, path: String, sink: Arc<dyn EventSink>) -> AppResult<()> {
        let mut g = self.inner.lock();
        if g.contains_key(&path) {
            return Ok(());
        }
        let path_for_event = path.clone();
        let mut watcher: RecommendedWatcher =
            notify::recommended_watcher(move |res: notify::Result<Event>| {
                if let Ok(ev) = res {
                    let kind = match ev.kind {
                        EventKind::Create(_) => "create",
                        EventKind::Modify(_) => "modify",
                        EventKind::Remove(_) => "remove",
                        _ => "any",
                    };
                    let payload = FsChange { path: path_for_event.clone(), kind };
                    events::emit(sink.as_ref(), "fs-change", &payload);
                }
            })
            .map_err(|e| AppError::Watch(e.to_string()))?;
        watcher
            .watch(&PathBuf::from(&path), RecursiveMode::NonRecursive)
            .map_err(|e| AppError::Watch(e.to_string()))?;
        g.insert(path, watcher);
        Ok(())
    }

    pub fn unwatch(&self, path: &str) {
        self.inner.lock().remove(path);
    }
}

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

#[allow(dead_code)]
fn _touch(_app: &AppHandle) {}

// Manager は将来の resolve 用 (現状は AppHandle のみで完結)
#[allow(dead_code)]
fn _touch_manager<R: tauri::Runtime>(_a: &impl Manager<R>) {}
