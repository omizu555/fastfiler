// Phase 5: 検索
// ignore クレート (gitignore 解釈) ベースの再帰検索 + ストリーミング配信。
// v1.1: Everything HTTP Server をバックエンドとして利用するモードを追加。
//
// Phase 2A: Tauri 依存を局所化。
//  - 純粋ロジックは `SearchCore::start_with_sink` に集約 (EventSink 経由)
//  - `#[tauri::command]` 版は AppHandle を sink に変換して薄くラップ

use crate::error::{AppError, AppResult};
use crate::events::{self, EventSink};
use crate::everything;
use ignore::WalkBuilder;
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Serialize, Clone)]
pub struct SearchHit {
    pub job_id: u64,
    pub path: String,
    pub name: String,
    pub is_dir: bool,
}

#[derive(Serialize, Clone)]
pub struct SearchDone {
    pub job_id: u64,
    pub total: usize,
    pub canceled: bool,
    pub backend: String,
    pub fallback: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct SearchState {
    current: Mutex<Option<Arc<AtomicU64>>>,
    next_id: AtomicU64,
}

#[derive(Clone, Default)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub use_regex: bool,
    pub include_hidden: bool,
    pub max_results: usize,
    pub backend: String,             // "builtin" | "everything"
    pub everything_port: u16,
    pub everything_scope: bool,
}

impl SearchState {
    /// 純粋関数版: AppHandle 不要の検索開始。
    /// バックグラウンドスレッドで sink に対して "search-hit" / "search-done" を emit する。
    pub fn start_with_sink(
        &self,
        sink: Arc<dyn EventSink>,
        root: String,
        pattern: String,
        opts: SearchOptions,
    ) -> AppResult<u64> {
        if pattern.is_empty() {
            return Err(AppError::Other("empty pattern".into()));
        }
        let job_id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let cancel = Arc::new(AtomicU64::new(0));
        {
            let mut cur = self.current.lock();
            if let Some(prev) = cur.take() {
                prev.store(1, Ordering::Relaxed);
            }
            *cur = Some(cancel.clone());
        }

        std::thread::spawn(move || {
            run_job(sink, job_id, cancel, root, pattern, opts);
        });

        Ok(job_id)
    }

    pub fn cancel(&self) {
        let mut cur = self.current.lock();
        if let Some(c) = cur.take() {
            c.store(1, Ordering::Relaxed);
        }
    }
}

fn run_job(
    sink: Arc<dyn EventSink>,
    job_id: u64,
    cancel: Arc<AtomicU64>,
    root: String,
    pattern: String,
    opts: SearchOptions,
) {
    if opts.backend == "everything" {
        let scope = if opts.everything_scope { Some(root.as_str()) } else { None };
        match everything::query(
            opts.everything_port,
            &pattern,
            scope,
            opts.case_sensitive,
            opts.use_regex,
            opts.max_results,
        ) {
            Ok(hits) => {
                let mut total = 0usize;
                for h in hits {
                    if cancel.load(Ordering::Relaxed) == 1 {
                        break;
                    }
                    let hit = SearchHit { job_id, path: h.path, name: h.name, is_dir: h.is_dir };
                    events::emit(sink.as_ref(), "search-hit", &hit);
                    total += 1;
                }
                let canceled = cancel.load(Ordering::Relaxed) == 1;
                events::emit(
                    sink.as_ref(),
                    "search-done",
                    &SearchDone {
                        job_id,
                        total,
                        canceled,
                        backend: "everything".into(),
                        fallback: false,
                        error: None,
                    },
                );
                return;
            }
            Err(e) => {
                let err_msg = format!("{}", e);
                let total = run_builtin(&sink, job_id, &cancel, &root, &pattern, &opts);
                let canceled = cancel.load(Ordering::Relaxed) == 1;
                events::emit(
                    sink.as_ref(),
                    "search-done",
                    &SearchDone {
                        job_id,
                        total,
                        canceled,
                        backend: "builtin".into(),
                        fallback: true,
                        error: Some(err_msg),
                    },
                );
                return;
            }
        }
    }
    // builtin
    let total = run_builtin(&sink, job_id, &cancel, &root, &pattern, &opts);
    let canceled = cancel.load(Ordering::Relaxed) == 1;
    events::emit(
        sink.as_ref(),
        "search-done",
        &SearchDone {
            job_id,
            total,
            canceled,
            backend: "builtin".into(),
            fallback: false,
            error: None,
        },
    );
}

fn run_builtin(
    sink: &Arc<dyn EventSink>,
    job_id: u64,
    cancel: &Arc<AtomicU64>,
    root: &str,
    pattern: &str,
    opts: &SearchOptions,
) -> usize {
    let matcher = build_matcher(pattern, opts.case_sensitive, opts.use_regex);
    let walker = WalkBuilder::new(root)
        .hidden(!opts.include_hidden)
        .ignore(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .build();
    let mut total = 0usize;
    for ent in walker {
        if cancel.load(Ordering::Relaxed) == 1 {
            break;
        }
        let ent = match ent {
            Ok(e) => e,
            Err(_) => continue,
        };
        let p = ent.path();
        let name = match p.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_owned(),
            None => continue,
        };
        if !matcher(&name) {
            continue;
        }
        let is_dir = ent.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let hit = SearchHit {
            job_id,
            path: p.to_string_lossy().to_string(),
            name,
            is_dir,
        };
        events::emit(sink.as_ref(), "search-hit", &hit);
        total += 1;
        if total >= opts.max_results {
            break;
        }
    }
    total
}

fn build_matcher(pattern: &str, case_sensitive: bool, regex_mode: bool) -> Box<dyn Fn(&str) -> bool + Send> {
    if regex_mode {
        let mut builder = regex::RegexBuilder::new(pattern);
        builder.case_insensitive(!case_sensitive);
        match builder.build() {
            Ok(re) => Box::new(move |name| re.is_match(name)),
            Err(_) => {
                let needle = pattern.to_owned();
                Box::new(move |name| name.contains(&needle))
            }
        }
    } else if case_sensitive {
        let needle = pattern.to_owned();
        Box::new(move |name| name.contains(&needle))
    } else {
        let needle = pattern.to_lowercase();
        Box::new(move |name| name.to_lowercase().contains(&needle))
    }
}
