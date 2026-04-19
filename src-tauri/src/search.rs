// Phase 5: 検索
// ignore クレート (gitignore 解釈) ベースの再帰検索 + ストリーミング配信。
// Tauri Emitter で `search-hit` / `search-done` イベントを送出する。
// v1.1: Everything HTTP Server をバックエンドとして利用するモードを追加。

use crate::error::{AppError, AppResult};
use crate::everything;
use ignore::WalkBuilder;
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

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
    pub fallback: bool, // everything 指定で失敗 → builtin にフォールバックした
    pub error: Option<String>,
}

#[derive(Default)]
pub struct SearchState {
    current: Mutex<Option<Arc<AtomicU64>>>, // 0 = running, 1 = canceled
    next_id: AtomicU64,
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn search_files(
    app: AppHandle,
    state: tauri::State<'_, SearchState>,
    root: String,
    pattern: String,
    case_sensitive: Option<bool>,
    use_regex: Option<bool>,
    include_hidden: Option<bool>,
    max_results: Option<usize>,
    backend: Option<String>,         // "builtin" | "everything"
    everything_port: Option<u16>,
    everything_scope: Option<bool>, // root をスコープ条件として AND するか (既定 true)
) -> AppResult<u64> {
    if pattern.is_empty() {
        return Err(AppError::Other("empty pattern".into()));
    }
    let job_id = state.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let cancel = Arc::new(AtomicU64::new(0));
    {
        let mut cur = state.current.lock();
        if let Some(prev) = cur.take() {
            prev.store(1, Ordering::Relaxed);
        }
        *cur = Some(cancel.clone());
    }

    let max_results = max_results.unwrap_or(5000);
    let cs = case_sensitive.unwrap_or(false);
    let regex_mode = use_regex.unwrap_or(false);
    let inc_hidden = include_hidden.unwrap_or(true);
    let backend = backend.unwrap_or_else(|| "builtin".into());
    let port = everything_port.unwrap_or(80);
    let use_scope = everything_scope.unwrap_or(true);
    let app2 = app.clone();

    std::thread::spawn(move || {
        if backend == "everything" {
            let scope = if use_scope { Some(root.as_str()) } else { None };
            match everything::query(port, &pattern, scope, cs, regex_mode, max_results) {
                Ok(hits) => {
                    let mut total = 0usize;
                    for h in hits {
                        if cancel.load(Ordering::Relaxed) == 1 {
                            break;
                        }
                        let hit = SearchHit {
                            job_id,
                            path: h.path,
                            name: h.name,
                            is_dir: h.is_dir,
                        };
                        let _ = app2.emit("search-hit", &hit);
                        total += 1;
                    }
                    let canceled = cancel.load(Ordering::Relaxed) == 1;
                    let _ = app2.emit(
                        "search-done",
                        SearchDone {
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
                    // フォールバック: 内蔵検索で続行
                    let err_msg = format!("{}", e);
                    let total = run_builtin(
                        &app2, job_id, &cancel, &root, &pattern, cs, regex_mode, inc_hidden,
                        max_results,
                    );
                    let canceled = cancel.load(Ordering::Relaxed) == 1;
                    let _ = app2.emit(
                        "search-done",
                        SearchDone {
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
        let total = run_builtin(
            &app2, job_id, &cancel, &root, &pattern, cs, regex_mode, inc_hidden, max_results,
        );
        let canceled = cancel.load(Ordering::Relaxed) == 1;
        let _ = app2.emit(
            "search-done",
            SearchDone {
                job_id,
                total,
                canceled,
                backend: "builtin".into(),
                fallback: false,
                error: None,
            },
        );
    });

    Ok(job_id)
}

#[allow(clippy::too_many_arguments)]
fn run_builtin(
    app: &AppHandle,
    job_id: u64,
    cancel: &Arc<AtomicU64>,
    root: &str,
    pattern: &str,
    cs: bool,
    regex_mode: bool,
    inc_hidden: bool,
    max_results: usize,
) -> usize {
    let matcher = build_matcher(pattern, cs, regex_mode);
    let walker = WalkBuilder::new(root)
        .hidden(!inc_hidden)
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
        let _ = app.emit("search-hit", &hit);
        total += 1;
        if total >= max_results {
            break;
        }
    }
    total
}

#[tauri::command]
pub fn search_cancel(state: tauri::State<'_, SearchState>) -> AppResult<()> {
    let mut cur = state.current.lock();
    if let Some(c) = cur.take() {
        c.store(1, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub fn everything_ping(port: Option<u16>) -> AppResult<bool> {
    Ok(everything::ping(port.unwrap_or(80)))
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

