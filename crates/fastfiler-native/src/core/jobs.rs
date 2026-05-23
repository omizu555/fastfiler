//! プログレスダイアログ用のジョブ管理。
//!
//! `fastfiler-domain::file_jobs::JobRegistry` を worker スレッドで回し、
//! 進捗イベントを `Arc<Mutex<Vec<JobEvent>>>` の inbox に push する。
//! UI スレッドは `exec_after(100ms)` の再帰 polling で inbox を drain し、
//! 該当 `JobView` の floem signal を更新する。
//!
//! 設計メモ:
//! - floem の signal を worker から直接触らないことで、Send/Sync 上の
//!   問題と再入の問題を回避している。
//! - 閾値判定は scan_size の完了後 (= worker 内) に行う。閾値未満なら
//!   `JobView` を生成せず静かに完了する。

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use floem::action::exec_after;
use floem::reactive::{RwSignal, Scope, SignalGet, SignalUpdate};

use fastfiler_domain::events::EventSink;
use fastfiler_domain::file_jobs::{JobItem, JobProgress, JobRegistry};

/// 件数 100 件以上 もしくは 50MB 以上のとき進捗ダイアログを表示する。
pub const THRESHOLD_FILES: u64 = 100;
pub const THRESHOLD_BYTES: u64 = 50 * 1024 * 1024;

/// ダイアログに表示する 1 ジョブの状態。
#[derive(Clone)]
pub struct JobView {
    pub id: u64,
    pub kind: String,
    /// 削除 (trash) のように進捗計測が不可能なジョブは true。
    pub indeterminate: bool,
    pub total_files: RwSignal<u64>,
    pub done_files: RwSignal<u64>,
    pub total_bytes: RwSignal<u64>,
    pub done_bytes: RwSignal<u64>,
    pub current: RwSignal<String>,
    pub status: RwSignal<JobStatus>,
}

#[derive(Clone, PartialEq)]
pub enum JobStatus {
    Running,
    Success,
    Canceled,
    Error(String),
}

/// worker から UI に渡るイベント。
enum JobEvent {
    Progress(JobProgress),
    Done {
        job_id: u64,
        ok: bool,
        canceled: bool,
        error: Option<String>,
        total_files: u64,
        done_files: u64,
        total_bytes: u64,
        done_bytes: u64,
    },
}

/// `EventSink` 実装。worker スレッドから呼ばれる。
struct InboxSink {
    inbox: Arc<Mutex<Vec<JobEvent>>>,
}

impl EventSink for InboxSink {
    fn emit_json(&self, event: &str, payload: serde_json::Value) {
        let evt = match event {
            "fs:job:progress" => {
                let v = payload;
                Some(JobEvent::Progress(JobProgress {
                    job_id: v.get("job_id").and_then(|x| x.as_u64()).unwrap_or(0),
                    kind: v
                        .get("kind")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    phase: v
                        .get("phase")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    total_files: v.get("total_files").and_then(|x| x.as_u64()).unwrap_or(0),
                    done_files: v.get("done_files").and_then(|x| x.as_u64()).unwrap_or(0),
                    total_bytes: v.get("total_bytes").and_then(|x| x.as_u64()).unwrap_or(0),
                    done_bytes: v.get("done_bytes").and_then(|x| x.as_u64()).unwrap_or(0),
                    current: v
                        .get("current")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                }))
            }
            "fs:job:done" => {
                let v = payload;
                Some(JobEvent::Done {
                    job_id: v.get("job_id").and_then(|x| x.as_u64()).unwrap_or(0),
                    ok: v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false),
                    canceled: v.get("canceled").and_then(|x| x.as_bool()).unwrap_or(false),
                    error: v
                        .get("error")
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string()),
                    total_files: v.get("total_files").and_then(|x| x.as_u64()).unwrap_or(0),
                    done_files: v.get("done_files").and_then(|x| x.as_u64()).unwrap_or(0),
                    total_bytes: v.get("total_bytes").and_then(|x| x.as_u64()).unwrap_or(0),
                    done_bytes: v.get("done_bytes").and_then(|x| x.as_u64()).unwrap_or(0),
                })
            }
            _ => None,
        };
        if let Some(e) = evt {
            self.inbox.lock().unwrap().push(e);
        }
    }
}

/// AppState に持たせるジョブ管理本体。
#[derive(Clone)]
pub struct JobsState {
    pub list: RwSignal<im::Vector<JobView>>,
    registry: Arc<JobRegistry>,
    inbox: Arc<Mutex<Vec<JobEvent>>>,
    next_id: Arc<AtomicU64>,
    polling_started: Arc<Mutex<bool>>,
}

impl Default for JobsState {
    fn default() -> Self {
        Self::new()
    }
}

impl JobsState {
    pub fn new() -> Self {
        let s = Scope::new();
        Self {
            list: s.create_rw_signal(im::Vector::new()),
            registry: Arc::new(JobRegistry::default()),
            inbox: Arc::new(Mutex::new(Vec::new())),
            next_id: Arc::new(AtomicU64::new(1)),
            polling_started: Arc::new(Mutex::new(false)),
        }
    }

    /// UI スレッドから 1 回呼ぶことで、100ms 間隔の inbox drain ループを起動する。
    /// 多重起動を避けるためフラグでガードする。
    pub fn start_polling(&self) {
        {
            let mut g = self.polling_started.lock().unwrap();
            if *g {
                return;
            }
            *g = true;
        }
        self.schedule_next();
    }

    fn schedule_next(&self) {
        let me = self.clone();
        exec_after(Duration::from_millis(100), move |_| {
            me.drain_inbox();
            me.schedule_next();
        });
    }

    fn drain_inbox(&self) {
        let events: Vec<JobEvent> = {
            let mut g = self.inbox.lock().unwrap();
            std::mem::take(&mut *g)
        };
        if events.is_empty() {
            return;
        }
        // 現在 list にある JobView を id で索けるようスナップショット
        let list = self.list.get_untracked();
        for evt in events {
            match evt {
                JobEvent::Progress(p) => {
                    if let Some(jv) = list.iter().find(|j| j.id == p.job_id) {
                        jv.total_files.set(p.total_files);
                        jv.done_files.set(p.done_files);
                        jv.total_bytes.set(p.total_bytes);
                        jv.done_bytes.set(p.done_bytes);
                        jv.current.set(p.current);
                    }
                }
                JobEvent::Done {
                    job_id,
                    ok,
                    canceled,
                    error,
                    total_files,
                    done_files,
                    total_bytes,
                    done_bytes,
                } => {
                    if let Some(jv) = list.iter().find(|j| j.id == job_id) {
                        jv.total_files.set(total_files);
                        jv.done_files.set(done_files);
                        jv.total_bytes.set(total_bytes);
                        jv.done_bytes.set(done_bytes);
                        let status = if canceled {
                            JobStatus::Canceled
                        } else if ok {
                            JobStatus::Success
                        } else {
                            JobStatus::Error(error.unwrap_or_else(|| String::from("error")))
                        };
                        jv.status.set(status.clone());
                        // 成功は 500ms 後に自動で list から外す
                        if status == JobStatus::Success {
                            let list_sig = self.list;
                            exec_after(Duration::from_millis(500), move |_| {
                                list_sig.update(|v| v.retain(|j| j.id != job_id));
                            });
                        }
                    }
                }
            }
        }
    }

    pub fn cancel(&self, job_id: u64) {
        self.registry.cancel(job_id);
    }

    /// コピー / 移動 をバックグラウンドで実行する。
    ///
    /// scan_size による事前計測を **UI スレッドで同期実行** し、閾値を超えた
    /// 場合のみ `JobView` を表示する。閾値未満なら静かに worker を回す。
    /// `on_done` は完了時に worker スレッドから呼ばれる (bool は ok かどうか)。
    pub fn spawn_copy(
        &self,
        items: Vec<(PathBuf, PathBuf)>,
        on_done: impl FnOnce(bool) + Send + 'static,
    ) {
        self.spawn_inner(items, "copy", on_done);
    }

    pub fn spawn_move(
        &self,
        items: Vec<(PathBuf, PathBuf)>,
        on_done: impl FnOnce(bool) + Send + 'static,
    ) {
        self.spawn_inner(items, "move", on_done);
    }

    fn spawn_inner(
        &self,
        items: Vec<(PathBuf, PathBuf)>,
        kind: &'static str,
        on_done: impl FnOnce(bool) + Send + 'static,
    ) {
        if items.is_empty() {
            on_done(true);
            return;
        }

        // 1) UI スレッドで scan_total を実行 (件数 100 程度なら一瞬)
        let (total_files, total_bytes) = scan_total(&items);
        let show_dialog = total_files >= THRESHOLD_FILES || total_bytes >= THRESHOLD_BYTES;

        // 2) 閾値超えなら JobView を作成して list に追加
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if show_dialog {
            let s = Scope::new();
            let jv = JobView {
                id,
                kind: kind.to_string(),
                indeterminate: false,
                total_files: s.create_rw_signal(total_files),
                done_files: s.create_rw_signal(0),
                total_bytes: s.create_rw_signal(total_bytes),
                done_bytes: s.create_rw_signal(0),
                current: s.create_rw_signal(String::new()),
                status: s.create_rw_signal(JobStatus::Running),
            };
            self.list.update(|v| v.push_back(jv));
        }

        // 3) worker でジョブを実行
        let registry = self.registry.clone();
        let inbox = self.inbox.clone();
        let job_items: Vec<JobItem> = items
            .iter()
            .map(|(from, to)| JobItem {
                from: from.to_string_lossy().into_owned(),
                to: to.to_string_lossy().into_owned(),
            })
            .collect();

        std::thread::spawn(move || {
            let sink = InboxSink {
                inbox: inbox.clone(),
            };
            let res = match kind {
                "copy" => registry.run_copy(&sink, id, job_items),
                "move" => registry.run_move(&sink, id, job_items),
                _ => unreachable!(),
            };
            on_done(res.is_ok());
        });
    }

    /// 件数しきい値だけで判定する indeterminate ジョブ用の view を作成する。
    /// 削除 (trash) のように外部 API に投げてしまうと進捗が取れないケースで使う。
    /// 戻り値はキャンセル要求を確認するための `Option<Arc<AtomicBool>>` で、
    /// `None` のときは閾値未満で表示なし。
    pub fn open_indeterminate(&self, kind: &str, total_files: u64) -> Option<u64> {
        if total_files < THRESHOLD_FILES {
            return None;
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let s = Scope::new();
        let jv = JobView {
            id,
            kind: kind.to_string(),
            indeterminate: true,
            total_files: s.create_rw_signal(total_files),
            done_files: s.create_rw_signal(0),
            total_bytes: s.create_rw_signal(0),
            done_bytes: s.create_rw_signal(0),
            current: s.create_rw_signal(String::new()),
            status: s.create_rw_signal(JobStatus::Running),
        };
        self.list.update(|v| v.push_back(jv));
        Some(id)
    }

    /// indeterminate ジョブの進捗 (件数) を進める。
    pub fn bump_indeterminate(&self, id: u64, done_files: u64, current: &str) {
        let list = self.list.get_untracked();
        if let Some(jv) = list.iter().find(|j| j.id == id) {
            jv.done_files.set(done_files);
            jv.current.set(current.to_string());
        }
    }

    /// indeterminate ジョブを完了させる。`error` が Some ならエラー扱い。
    pub fn finish_indeterminate(&self, id: u64, error: Option<String>) {
        let list = self.list.get_untracked();
        if let Some(jv) = list.iter().find(|j| j.id == id) {
            let status = match error {
                Some(e) => JobStatus::Error(e),
                None => JobStatus::Success,
            };
            jv.status.set(status.clone());
            if status == JobStatus::Success {
                let list_sig = self.list;
                exec_after(Duration::from_millis(500), move |_| {
                    list_sig.update(|v| v.retain(|j| j.id != id));
                });
            }
        }
    }
}

/// `items` の from 側を再帰スキャンしてファイル数と合計バイト数を返す。
fn scan_total(items: &[(PathBuf, PathBuf)]) -> (u64, u64) {
    let mut tf = 0u64;
    let mut tb = 0u64;
    for (from, _) in items {
        scan_one(from, &mut tf, &mut tb);
    }
    (tf, tb)
}

fn scan_one(path: &std::path::Path, tf: &mut u64, tb: &mut u64) {
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.is_dir() {
            if let Ok(rd) = std::fs::read_dir(path) {
                for ent in rd.flatten() {
                    scan_one(&ent.path(), tf, tb);
                }
            }
        } else {
            *tf += 1;
            *tb += meta.len();
        }
    }
}
