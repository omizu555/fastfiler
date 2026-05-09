// Phase 2B: trait / NullSink / emit ヘルパは fastfiler-domain に移動済。
// 既存コードの `use crate::events::{self, EventSink};` を維持するため re-export。
// `tauri_sink` だけは Tauri 依存なのでこちら側に残す。

#[allow(unused_imports)]
pub use fastfiler_domain::events::{emit, EventSink, NullSink};

/// Tauri AppHandle を EventSink に橋渡しする。
pub fn tauri_sink(app: tauri::AppHandle) -> impl EventSink {
    move |event: &str, payload: serde_json::Value| {
        use tauri::Emitter;
        let _ = app.emit(event, payload);
    }
}
