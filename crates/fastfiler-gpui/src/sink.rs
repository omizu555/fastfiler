//! domain の `EventSink` を GPUI へ橋渡しするチャネルシンク。
//!
//! watcher / 検索 / ファイルジョブ等は別スレッドから `emit_json` を呼ぶ。それを
//! async-channel に流し、UI 側の `cx.spawn` ループが受けて `Entity` を更新する。
//!
//! **リーク防止の要**: 送信端 (このシンク) は PaneView と watcher 登録が保持する。
//! PaneView が drop されると両方が落ち、チャネルが閉じて受信ループが自然終了する。
//! floem 版の `create_signal_from_channel` のようにスレッド/シグナルが残り続けない。

use fastfiler_domain::events::EventSink;

/// UI へ届くドメインイベント: (イベント名, JSON ペイロード)。
pub type DomainEvent = (String, serde_json::Value);

/// `EventSink` 実装。clone して watcher 等へ渡す。
#[derive(Clone)]
pub struct ChannelSink {
    tx: async_channel::Sender<DomainEvent>,
}

impl ChannelSink {
    pub fn new() -> (Self, async_channel::Receiver<DomainEvent>) {
        let (tx, rx) = async_channel::unbounded();
        (Self { tx }, rx)
    }
}

impl EventSink for ChannelSink {
    fn emit_json(&self, event: &str, payload: serde_json::Value) {
        // 受信側が閉じていても無視 (ペインを閉じた後に届く遅延イベント)。
        let _ = self.tx.try_send((event.to_string(), payload));
    }
}
