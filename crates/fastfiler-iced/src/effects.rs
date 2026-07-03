//! core の `Effect` を iced の `Task` / domain 呼び出しへ変換する層 (計画書 §5.3)。
//! I/O はすべてここ (と domain) に閉じる。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use fastfiler_core::{Effect, Entry};
use fastfiler_domain::{fs as dfs, icons, shell};
use iced::Task;

use crate::app::Msg;

/// アイコンの生バイト列 (key → PNG)。ハンドル化は UI 側で行う。
pub type IconBytes = Vec<(String, Vec<u8>)>;

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
