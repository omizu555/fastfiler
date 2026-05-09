// Phase 2A: Tauri 依存を局所化するためのイベント抽象。
//
// 目的:
//   `#[tauri::command]` 関数本体が `AppHandle::emit` を直接呼ぶのをやめ、
//   `&dyn EventSink` 経由で送出できるようにする。これにより、将来の floem 版から
//   同じ関数本体を Tauri 抜きで呼び出せるようになる。
//
// 命名規則:
//   - 純粋ロジック側 (sink を受け取る側): `pub fn xxx_with_sink(sink: &dyn EventSink, ...)`
//   - Tauri shim 側: 既存名 `pub fn xxx(...)` を維持し、内部で `make_tauri_sink(&app)` を渡す

use serde::Serialize;

/// 任意のイベントを emit するための抽象。
/// Send + Sync を要求するのは、長時間タスク (検索・ファイルジョブ) が
/// 別スレッドから sink を呼ぶため。
pub trait EventSink: Send + Sync {
    fn emit_json(&self, event: &str, payload: serde_json::Value);
}

impl<F> EventSink for F
where
    F: Fn(&str, serde_json::Value) + Send + Sync,
{
    fn emit_json(&self, event: &str, payload: serde_json::Value) {
        (self)(event, payload)
    }
}

/// 任意の Serialize 値を JSON に変換して emit するヘルパ。
pub fn emit<T: Serialize>(sink: &dyn EventSink, event: &str, payload: &T) {
    if let Ok(v) = serde_json::to_value(payload) {
        sink.emit_json(event, v);
    }
}

/// テスト / floem 版用の no-op sink。
#[allow(dead_code)]
pub struct NullSink;
impl EventSink for NullSink {
    fn emit_json(&self, _: &str, _: serde_json::Value) {}
}

/// Tauri AppHandle を EventSink に橋渡しする。
pub fn tauri_sink(app: tauri::AppHandle) -> impl EventSink {
    move |event: &str, payload: serde_json::Value| {
        use tauri::Emitter;
        let _ = app.emit(event, payload);
    }
}
