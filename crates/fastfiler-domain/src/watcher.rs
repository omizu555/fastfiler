// Phase 2B-4: ディレクトリ監視 (notify, ReadDirectoryChangesW) の純粋部分。
//
// Tauri 非依存。EventSink 経由で fs-change イベントを発火する。
// AppHandle/State を持つ薄いラッパーは src-tauri 側に存在する。

use crate::error::{AppError, AppResult};
use crate::events::{self, EventSink};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

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
