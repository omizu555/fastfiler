// Phase 1: ディレクトリ監視 (notify クレート, ReadDirectoryChangesW)
// 監視対象パスをキーに RecommendedWatcher を保持し、変更を Tauri イベントで配信する。

use crate::error::{AppError, AppResult};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[derive(Serialize, Clone)]
pub struct FsChange {
    pub path: String,
    pub kind: &'static str, // "create" | "modify" | "remove" | "rename" | "any"
}

pub struct WatcherState {
    inner: Arc<Mutex<Inner>>,
    app: AppHandle,
}

struct Inner {
    watchers: HashMap<String, RecommendedWatcher>,
}

impl WatcherState {
    pub fn new(app: AppHandle) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner { watchers: HashMap::new() })),
            app,
        }
    }
}

#[tauri::command]
pub fn watch_dir(path: String, state: State<'_, WatcherState>) -> AppResult<()> {
    let mut g = state.inner.lock();
    if g.watchers.contains_key(&path) {
        return Ok(());
    }
    let app = state.app.clone();
    let p = PathBuf::from(&path);
    let path_for_event = path.clone();
    let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(ev) = res {
            let kind = match ev.kind {
                EventKind::Create(_) => "create",
                EventKind::Modify(_) => "modify",
                EventKind::Remove(_) => "remove",
                _ => "any",
            };
            let payload = FsChange { path: path_for_event.clone(), kind };
            let _ = app.emit("fs-change", payload);
        }
    })
    .map_err(|e| AppError::Watch(e.to_string()))?;
    watcher
        .watch(&p, RecursiveMode::NonRecursive)
        .map_err(|e| AppError::Watch(e.to_string()))?;
    g.watchers.insert(path, watcher);
    Ok(())
}

#[tauri::command]
pub fn unwatch_dir(path: String, state: State<'_, WatcherState>) -> AppResult<()> {
    let mut g = state.inner.lock();
    g.watchers.remove(&path);
    Ok(())
}

// suppress unused
#[allow(dead_code)]
fn _touch(_app: &AppHandle) {}
