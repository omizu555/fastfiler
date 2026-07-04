//! 開発・検証用ユーティリティ (本番動作には関与しない)。

use iced::Task;

/// `FASTFILER_AUTOCLOSE_MS=N` が設定されていれば、N ミリ秒後に `msg` を返す Task を作る。
/// 未設定なら `Task::none()`。ウィンドウ起動確認やスパイクの自動終了に使う
/// (受け取った側で結果を stdout に出して `iced::exit()` する)。
pub fn autoclose_task<Message: Send + 'static>(msg: Message) -> Task<Message> {
    match std::env::var("FASTFILER_AUTOCLOSE_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        Some(ms) => Task::future(async move {
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            msg
        }),
        None => Task::none(),
    }
}
