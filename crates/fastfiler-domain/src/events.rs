//! 任意のイベントを emit するための抽象 (Tauri 非依存)。
//!
//! Tauri アダプタ (`tauri_sink`) は src-tauri 側に存在する。

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
