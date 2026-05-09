// Phase 2B-4: 純粋ロジックは fastfiler-domain::search に移動済。
// src-tauri 側は AppHandle / tauri::State を sink/state に橋渡しする shim のみ。

use crate::error::AppResult;
use crate::events::{self, EventSink};
use fastfiler_domain::{everything as core_everything, search as core};
use std::sync::Arc;
use tauri::AppHandle;

#[allow(unused_imports)]
pub use core::{SearchDone, SearchHit, SearchOptions, SearchState};

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn search_files(
    app: AppHandle,
    state: tauri::State<'_, SearchState>,
    root: String,
    pattern: String,
    case_sensitive: Option<bool>,
    use_regex: Option<bool>,
    include_hidden: Option<bool>,
    max_results: Option<usize>,
    backend: Option<String>,
    everything_port: Option<u16>,
    everything_scope: Option<bool>,
) -> AppResult<u64> {
    let opts = SearchOptions {
        case_sensitive: case_sensitive.unwrap_or(false),
        use_regex: use_regex.unwrap_or(false),
        include_hidden: include_hidden.unwrap_or(true),
        max_results: max_results.unwrap_or(5000),
        backend: backend.unwrap_or_else(|| "builtin".into()),
        everything_port: everything_port.unwrap_or(80),
        everything_scope: everything_scope.unwrap_or(true),
    };
    let sink: Arc<dyn EventSink> = Arc::new(events::tauri_sink(app));
    state.start_with_sink(sink, root, pattern, opts)
}

#[tauri::command]
pub fn search_cancel(state: tauri::State<'_, SearchState>) -> AppResult<()> {
    state.cancel();
    Ok(())
}

#[tauri::command]
pub fn everything_ping(port: Option<u16>) -> AppResult<bool> {
    Ok(core_everything::ping(port.unwrap_or(80)))
}
