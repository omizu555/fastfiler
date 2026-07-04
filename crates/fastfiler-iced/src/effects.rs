//! core の `Effect` を iced の `Task` / domain 呼び出しへ変換する層 (計画書 §5.3)。
//! I/O はすべてここ (と domain) に閉じる。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use fastfiler_core::transfer::TransferOp;
use fastfiler_core::update_app::AppMsg;
use fastfiler_core::{Effect, Entry, PaneMsg};
use fastfiler_domain::events::EventSink;
use fastfiler_domain::file_jobs::{JobItem, JobRegistry};
use fastfiler_domain::search::{SearchOptions, SearchState};
use fastfiler_domain::undo::{TrashedItem, UndoOp};
use fastfiler_domain::watcher::WatcherCore;
use fastfiler_domain::{file_ops, fs as dfs, icons, shell, templates};
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

/// ファイルジョブの共有資源 (キャンセル用レジストリ + 通し番号)。
pub struct Jobs {
    pub registry: Arc<JobRegistry>,
    next_id: AtomicU64,
}

impl Jobs {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(JobRegistry::default()),
            next_id: AtomicU64::new(1),
        }
    }

    /// ジョブ id の採番 (呼び出し側が id を先に知り、Undo 候補の登録を
    /// **スレッド起動前に**済ませられるようにする — JobDone との競合防止)。
    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

impl Default for Jobs {
    fn default() -> Self {
        Self::new()
    }
}

/// Undo 実行や操作完了の通知。record は成功時に UndoManager へ積む操作。
#[derive(Debug, Clone)]
pub enum OpOutcome {
    Done {
        record: Option<UndoOp>,
        message: Option<String>,
    },
    Failed(String),
}

/// pane 文脈が不要な Effect の実行。LoadDir/SpawnJob/PaneClosed/ScheduleSessionSave は
/// App 側 (run_effects) が資源登録と併せて処理する。
pub fn run(effect: Effect, jobs: &Jobs) -> Task<Msg> {
    match effect {
        Effect::LoadDir { .. }
        | Effect::SpawnJob { .. }
        | Effect::PaneClosed(_)
        | Effect::OpenTabFor { .. }
        | Effect::StartSearch { .. }
        | Effect::DropTransfer { .. } // update_app 内で展開済み (ここには来ない)
        | Effect::ScheduleSessionSave => Task::none(),
        Effect::LoadDrives => Task::future(async {
            let drives = tokio::task::spawn_blocking(dfs::list_drives).await;
            let list = match drives {
                Ok(Ok(v)) => v.into_iter().map(|d| (d.letter, d.label)).collect(),
                _ => vec![],
            };
            Msg::Core(AppMsg::Tree(fastfiler_core::tree::TreeMsg::DrivesLoaded(
                list,
            )))
        }),
        Effect::LoadTreeChildren { path } => Task::future(async move {
            let p = path.clone();
            let dirs = tokio::task::spawn_blocking(move || {
                dfs::list_dirs(p.to_string_lossy().to_string(), Some(false))
            })
            .await;
            let children = match dirs {
                Ok(Ok(v)) => v
                    .into_iter()
                    .filter(|e| e.kind == "dir")
                    .map(|e| fastfiler_core::tree::TreeChild {
                        path: path.join(&e.name),
                        name: e.name,
                    })
                    .collect(),
                _ => vec![], // アクセス不可は空 (矢印が消える)
            };
            Msg::Core(AppMsg::Tree(
                fastfiler_core::tree::TreeMsg::ChildrenLoaded {
                    path,
                    dirs: children,
                },
            ))
        }),
        Effect::OpenFile { path } => {
            // domain 側が専用スレッドで ShellExecuteW する (UI 再入なし)
            shell::open_with_shell_async(path.to_string_lossy().to_string());
            Task::none()
        }
        Effect::Debounce { pane, seq, millis } => Task::future(async move {
            tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
            Msg::Core(AppMsg::Pane(pane, PaneMsg::ReloadTick(seq)))
        }),
        Effect::ClipboardWrite { paths, op } => blocking_op(move || {
            let n = paths.len();
            let paths: Vec<String> = paths
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect();
            fastfiler_domain::win_clipboard::clipboard_write_paths(paths, op.clone())?;
            Ok(OpOutcome::Done {
                record: None,
                message: Some(if op == "cut" {
                    format!("{n} 項目を切り取りました")
                } else {
                    format!("{n} 項目をコピーしました")
                }),
            })
        }),
        Effect::ClipboardRead { pane } => Task::future(async move {
            let read =
                tokio::task::spawn_blocking(fastfiler_domain::win_clipboard::clipboard_read_paths)
                    .await;
            let to_pane = |m: PaneMsg| Msg::Core(AppMsg::Pane(pane, m));
            match read {
                Ok(Ok(Some(cb))) => to_pane(PaneMsg::PasteRead {
                    paths: cb.paths,
                    op: cb.op,
                }),
                // クリップボードにファイルなし → 無言 (貼り付け淡色相当)
                Ok(Ok(None)) => to_pane(PaneMsg::StatusMsg(
                    "クリップボードにファイルがありません".into(),
                )),
                Ok(Err(e)) => to_pane(PaneMsg::StatusMsg(format!("貼り付けエラー: {e}"))),
                Err(e) => to_pane(PaneMsg::StatusMsg(format!("貼り付けエラー: {e}"))),
            }
        }),
        Effect::CancelJob { id } => {
            jobs.registry.cancel(id);
            Task::none()
        }
        Effect::Rename { from, to } => blocking_op(move || {
            file_ops::rename_path_no_overwrite(&from, &to)?;
            Ok(OpOutcome::Done {
                record: Some(UndoOp::Rename { from, to }),
                message: None,
            })
        }),
        Effect::CreateDir { path } => blocking_op(move || {
            file_ops::create_dir(&path)?;
            Ok(OpOutcome::Done {
                record: None,
                message: None,
            })
        }),
        Effect::CreateFromTemplate { dir, template } => blocking_op(move || {
            templates::create_file_from_template(
                template,
                dir.to_string_lossy().to_string(),
                None,
            )?;
            Ok(OpOutcome::Done {
                record: None,
                message: None,
            })
        }),
        Effect::RunUserCommand { id, paths, cwd } => blocking_op(move || {
            let ctx = fastfiler_domain::user_commands::RunCtx {
                paths: paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
                cwd: cwd.to_string_lossy().to_string(),
            };
            fastfiler_domain::user_commands::run_user_command(id, ctx)?;
            Ok(OpOutcome::Done {
                record: None,
                message: None,
            })
        }),
        // ShowShellMenu は UI スレッド同期実行が必要 → app.rs run_effects 側で処理
        Effect::ShowShellMenu { .. } => Task::none(),
        Effect::CreateFile { dir, name } => blocking_op(move || {
            templates::create_empty_file(dir.to_string_lossy().to_string(), name, None)?;
            Ok(OpOutcome::Done {
                record: None,
                message: None,
            })
        }),
        Effect::DeleteToTrash { paths } => blocking_op(move || {
            // Undo 用のメタデータをごみ箱送り前に採取 (restore_from_trash の照合キー)
            let items: Vec<TrashedItem> = paths
                .iter()
                .filter_map(|p| {
                    let meta = std::fs::symlink_metadata(p).ok()?;
                    Some(TrashedItem {
                        original_path: p.clone(),
                        file_name: p.file_name()?.to_os_string(),
                        size: meta.len(),
                        modified: meta.modified().ok()?,
                        is_dir: meta.is_dir(),
                        deleted_at: std::time::SystemTime::now(),
                    })
                })
                .collect();
            let n = paths.len();
            let unrecoverable = n - items.len(); // メタデータ採取に失敗 = Undo 対象外
            file_ops::delete_to_trash(
                paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            )?;
            let message = if unrecoverable == 0 {
                format!("{n} 項目をごみ箱へ移動しました")
            } else {
                format!("{n} 項目をごみ箱へ移動しました ({unrecoverable} 件は Ctrl+Z で戻せません)")
            };
            Ok(OpOutcome::Done {
                record: (!items.is_empty()).then_some(UndoOp::Trash { items }),
                message: Some(message),
            })
        }),
        // PerformUndo は App が UndoManager を所有するため app.rs 側で処理する
        Effect::PerformUndo => Task::done(Msg::PerformUndo),
    }
}

/// ブロッキングなファイル操作を背景スレッドで実行し、結果を OpDone で返す。
fn blocking_op(
    f: impl FnOnce() -> Result<OpOutcome, fastfiler_domain::error::AppError> + Send + 'static,
) -> Task<Msg> {
    Task::future(async move {
        let outcome = tokio::task::spawn_blocking(f).await;
        match outcome {
            Ok(Ok(o)) => Msg::OpDone(o),
            Ok(Err(e)) => Msg::OpDone(OpOutcome::Failed(e.to_string())),
            Err(e) => Msg::OpDone(OpOutcome::Failed(format!("操作タスクの失敗: {e}"))),
        }
    })
}

/// 右クリックメニューの文脈を同期採取する (templates / commands / can_paste)。
/// いずれも軽量なローカル I/O (GPUI 版もメニュー展開時に同期取得)。
pub fn collect_menu_context() -> (
    Vec<fastfiler_core::menu::TemplateInfo>,
    Vec<fastfiler_core::menu::CommandInfo>,
    String,
    bool,
) {
    let templates = templates::list_templates()
        .map(|v| {
            v.into_iter()
                .map(|t| fastfiler_core::menu::TemplateInfo {
                    name: t.name,
                    path: t.path,
                })
                .collect()
        })
        .unwrap_or_default();
    let commands = fastfiler_domain::user_commands::list_user_commands()
        .map(|v| {
            v.into_iter()
                .map(|c| fastfiler_core::menu::CommandInfo {
                    id: c.id,
                    label: c.label,
                    when: c.when,
                    extensions: c.extensions,
                    submenu: c.submenu,
                    hidden: c.hidden,
                })
                .collect()
        })
        .unwrap_or_default();
    let tdir = templates::templates_dir().unwrap_or_default();
    let can_paste = matches!(
        fastfiler_domain::win_clipboard::clipboard_read_paths(),
        Ok(Some(_))
    );
    (templates, commands, tdir, can_paste)
}

/// 検索を開始する (Everything → 内蔵の自動フォールバックは domain 側 — F-702)。
/// 前回の検索は domain が自動キャンセルする。戻り値: job_id (結果ルーティング用)。
pub fn start_search(
    state: &SearchState,
    root: std::path::PathBuf,
    query: String,
    everything_port: u16,
) -> Option<u64> {
    let opts = SearchOptions {
        case_sensitive: false,
        use_regex: false,
        include_hidden: true,
        max_results: 2000,
        backend: "everything".into(),
        everything_port,
        everything_scope: true,
    };
    state
        .start_with_sink(
            Arc::new(ChannelSink::new()),
            root.to_string_lossy().to_string(),
            query,
            opts,
        )
        .ok()
}

/// コピー/移動ジョブを専用スレッドで起動する (GPUI 版と同じ実行形態)。
/// id は呼び出し側が採番済み — Undo 候補の登録をこの呼び出し**前**に行うこと
/// (スレッドは即走るため、後から登録すると JobDone に追い越されるレースになる)。
pub fn spawn_job(jobs: &Jobs, job_id: u64, op: TransferOp, items: Vec<(PathBuf, PathBuf)>) {
    let registry = Arc::clone(&jobs.registry);
    let job_items: Vec<JobItem> = items
        .iter()
        .map(|(from, to)| JobItem {
            from: from.to_string_lossy().to_string(),
            to: to.to_string_lossy().to_string(),
        })
        .collect();
    std::thread::spawn(move || {
        let sink = ChannelSink::new();
        let r = match op {
            TransferOp::Copy => registry.run_copy(&sink, job_id, job_items),
            TransferOp::Move => registry.run_move(&sink, job_id, job_items),
        };
        if let Err(e) = r {
            // run_job 内で done は emit 済みのはずだが、起動前失敗の保険
            sink.emit_json(
                "fs:job:done",
                serde_json::json!({
                    "job_id": job_id, "kind": "job", "ok": false,
                    "canceled": false, "error": e.to_string(),
                    "total_files": 0, "done_files": 0,
                    "total_bytes": 0, "done_bytes": 0,
                }),
            );
        }
    });
}

/// Undo の逆操作を実行する (Ctrl+Z)。App が pop した op を受け取る。
///
/// 部分失敗時は**未処理分 + 失敗項目を新しい UndoOp として record に返す**
/// (ADR 0008 S2: 履歴に積み直して再 Ctrl+Z で再試行できる)。
pub fn perform_undo(op: UndoOp) -> Task<Msg> {
    blocking_op(move || {
        let label = op.label();
        let (record, failed): (Option<UndoOp>, usize) = match op {
            UndoOp::Rename { from, to } => match file_ops::rename_path_no_overwrite(&to, &from) {
                Ok(()) => (None, 0),
                Err(e) => return Err(e),
            },
            UndoOp::Move { items } => {
                let mut remaining = Vec::new();
                for it in items.into_iter().rev() {
                    if file_ops::move_path_no_overwrite(&it.to, &it.from).is_err() {
                        remaining.push(it);
                    }
                }
                remaining.reverse();
                let failed = remaining.len();
                (
                    (!remaining.is_empty()).then_some(UndoOp::Move { items: remaining }),
                    failed,
                )
            }
            UndoOp::Trash { items } => {
                let mut remaining = Vec::new();
                for it in items {
                    if file_ops::restore_from_trash(&it).is_err() {
                        remaining.push(it);
                    }
                }
                let failed = remaining.len();
                (
                    (!remaining.is_empty()).then_some(UndoOp::Trash { items: remaining }),
                    failed,
                )
            }
        };
        let message = if failed == 0 {
            format!("{label}を元に戻しました")
        } else {
            format!("{label}を元に戻しました (失敗 {failed} 件 — Ctrl+Z で再試行できます)")
        };
        Ok(OpOutcome::Done {
            record,
            message: Some(message),
        })
    })
}

/// 一覧読み込みタスク (pane 宛に DirLoaded を返す)。
pub fn load_dir_task(
    pane: fastfiler_core::PaneId,
    generation: u64,
    path: PathBuf,
    known_icons: HashSet<String>,
    synth: Option<usize>,
) -> Task<Msg> {
    Task::future(load_dir(pane, generation, path, known_icons, synth))
}

async fn load_dir(
    pane: fastfiler_core::PaneId,
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
            pane,
            generation,
            result,
            icons,
        },
        Err(e) => Msg::DirLoaded {
            pane,
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
