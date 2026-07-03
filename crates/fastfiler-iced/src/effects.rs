//! core の `Effect` を iced の `Task` / domain 呼び出しへ変換する層 (計画書 §5.3)。
//! I/O はすべてここ (と domain) に閉じる。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fastfiler_core::{Effect, Entry, PaneMsg};
use fastfiler_domain::events::EventSink;
use fastfiler_domain::watcher::WatcherCore;
use fastfiler_domain::{fs as dfs, icons, shell};
use iced::Task;

use crate::app::Msg;

/// アイコンの生バイト列 (key → PNG)。ハンドル化は UI 側で行う。
pub type IconBytes = Vec<(String, Vec<u8>)>;

/// domain → GUI のイベント配管 (計画書 §5.3)。
/// 送信端は watcher 等の sink、受信端は `Subscription::run` が 1 回だけ取り出す。
pub mod domain_channel {
    use std::sync::Mutex;

    use async_channel::{Receiver, Sender};
    use once_cell::sync::Lazy;
    use serde_json::Value;

    pub type DomainRawEvent = (String, Value);
    type ChannelPair = (
        Sender<DomainRawEvent>,
        Mutex<Option<Receiver<DomainRawEvent>>>,
    );

    static CHANNEL: Lazy<ChannelPair> = Lazy::new(|| {
        let (tx, rx) = async_channel::unbounded();
        (tx, Mutex::new(Some(rx)))
    });

    pub fn sender() -> Sender<DomainRawEvent> {
        CHANNEL.0.clone()
    }

    /// Subscription 用の受信ストリーム (1 回だけ取り出せる)。
    pub fn take_receiver() -> Receiver<DomainRawEvent> {
        CHANNEL
            .1
            .lock()
            .unwrap()
            .take()
            .expect("domain_channel receiver already taken")
    }
}

/// `EventSink` → アプリ単一チャネルのブリッジ (GPUI 版 sink.rs の移植)。
pub struct ChannelSink(async_channel::Sender<domain_channel::DomainRawEvent>);

impl ChannelSink {
    pub fn new() -> Self {
        Self(domain_channel::sender())
    }
}

impl Default for ChannelSink {
    fn default() -> Self {
        Self::new()
    }
}

impl EventSink for ChannelSink {
    fn emit_json(&self, event: &str, payload: serde_json::Value) {
        let _ = self.0.try_send((event.to_string(), payload));
    }
}

/// ペインが持つ watcher 資源。navigate のたびに監視先を付け替え、
/// drop で監視が止まる (メモリ健全性 N-02 の検証対象)。
pub struct PaneWatcher {
    core: WatcherCore,
    sink: Arc<dyn EventSink>,
    watching: Option<String>,
}

impl PaneWatcher {
    pub fn new() -> Self {
        Self {
            core: WatcherCore::default(),
            sink: Arc::new(ChannelSink::new()),
            watching: None,
        }
    }

    /// 監視先を path へ付け替える (同一パスなら何もしない)。
    pub fn watch(&mut self, path: &Path) {
        let path = path.to_string_lossy().to_string();
        if self.watching.as_deref() == Some(path.as_str()) {
            return;
        }
        if let Some(old) = self.watching.take() {
            self.core.unwatch(&old);
        }
        // ネットワークドライブ等で watch できない場合は自動更新なし (F5 で手動更新 —
        // USAGE.md §6 のトラブルシューティングと同じ縮退)
        if self
            .core
            .watch_with_sink(path.clone(), self.sink.clone())
            .is_ok()
        {
            self.watching = Some(path);
        }
    }
}

impl Default for PaneWatcher {
    fn default() -> Self {
        Self::new()
    }
}

pub fn run(effect: Effect, known_icons: HashSet<String>, synth: Option<usize>) -> Task<Msg> {
    match effect {
        Effect::LoadDir { generation, path } => {
            Task::future(load_dir(generation, path, known_icons, synth))
        }
        Effect::OpenFile { path } => {
            // domain 側が専用スレッドで ShellExecuteW する (UI 再入なし)
            shell::open_with_shell_async(path.to_string_lossy().to_string());
            Task::none()
        }
        Effect::Debounce { seq, millis } => Task::future(async move {
            tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
            Msg::Pane(PaneMsg::ReloadTick(seq))
        }),
    }
}

async fn load_dir(
    generation: u64,
    path: PathBuf,
    known_icons: HashSet<String>,
    synth: Option<usize>,
) -> Msg {
    let job = tokio::task::spawn_blocking(move || {
        let entries = match synth {
            Some(n) => Ok(synth_entries(n)),
            None => list_real(&path),
        };
        match entries {
            Ok(entries) => {
                let icons = fetch_icons(&entries, &known_icons);
                (Ok(entries), icons)
            }
            Err(e) => (Err(e), Vec::new()),
        }
    });
    match job.await {
        Ok((result, icons)) => Msg::DirLoaded {
            generation,
            result,
            icons,
        },
        Err(e) => Msg::DirLoaded {
            generation,
            result: Err(format!("load task panicked: {e}")),
            icons: Vec::new(),
        },
    }
}

fn list_real(path: &Path) -> Result<Vec<Entry>, String> {
    let raw = dfs::list_dir(path.to_string_lossy().to_string()).map_err(|e| e.to_string())?;
    Ok(raw
        .into_iter()
        .map(|f| Entry::new(f.name, f.kind == "dir", f.size, f.modified, f.ext, f.hidden))
        .collect())
}

/// B-2 用の合成一覧 (FASTFILER_SYNTH=n)。FS を介さず件数スケールだけを測る。
fn synth_entries(n: usize) -> Vec<Entry> {
    (0..n)
        .map(|i| {
            Entry::new(
                format!("合成ファイル_{i:06}.txt"),
                i % 50 == 0,
                (i as u64 * 37) % 9_999_999,
                1_700_000_000 + i as i64,
                Some("txt".into()),
                false,
            )
        })
        .collect()
}

/// 一覧に現れる未知のアイコンキーだけ取得する (拡張子単位の共有 — GPUI 版と同じ)。
fn fetch_icons(entries: &[Entry], known: &HashSet<String>) -> IconBytes {
    let mut keys: HashSet<&str> = HashSet::new();
    for e in entries {
        if !known.contains(&e.icon_key) {
            keys.insert(e.icon_key.as_str());
        }
    }
    keys.into_iter()
        .filter_map(|key| {
            let bytes = if key == "/" {
                icons::folder_icon_png(false).ok()?
            } else if key.is_empty() {
                icons::system_icon_png("file", false, true).ok()?
            } else {
                icons::system_icon_png(&format!("f.{key}"), false, true).ok()?
            };
            Some((key.to_string(), bytes.as_ref().clone()))
        })
        .collect()
}
