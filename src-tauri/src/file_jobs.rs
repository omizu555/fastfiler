// v3.2: ファイル操作の進捗イベント / キャンセル対応
//
// frontend が job_id を採番して invoke する。
// 操作中は "fs:job:progress" を、完了時に "fs:job:done" をペイロード付きで emit する。
// cancel_job(job_id) を呼ぶと AtomicBool が立ち、ループが中断されエラー (Cancelled) を返す。

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter, State};

#[derive(Default)]
pub struct JobRegistry {
    inner: Mutex<HashMap<u64, Arc<AtomicBool>>>,
}

impl JobRegistry {
    fn register(&self, id: u64) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.inner.lock().unwrap().insert(id, flag.clone());
        flag
    }
    fn unregister(&self, id: u64) {
        self.inner.lock().unwrap().remove(&id);
    }
    fn cancel(&self, id: u64) -> bool {
        if let Some(f) = self.inner.lock().unwrap().get(&id) {
            f.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct JobItem {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct JobProgress {
    pub job_id: u64,
    pub kind: String, // "copy" | "move" | "delete"
    pub phase: String, // "scan" | "run" | "done"
    pub total_files: u64,
    pub done_files: u64,
    pub total_bytes: u64,
    pub done_bytes: u64,
    pub current: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct JobDone {
    pub job_id: u64,
    pub kind: String,
    pub ok: bool,
    pub canceled: bool,
    pub error: Option<String>,
    pub total_files: u64,
    pub done_files: u64,
    pub total_bytes: u64,
    pub done_bytes: u64,
}

#[tauri::command]
pub fn cancel_job(job_id: u64, state: State<'_, JobRegistry>) -> bool {
    state.cancel(job_id)
}

fn scan_size(path: &Path, total_files: &mut u64, total_bytes: &mut u64) {
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.is_dir() {
            if let Ok(rd) = fs::read_dir(path) {
                for ent in rd.flatten() {
                    scan_size(&ent.path(), total_files, total_bytes);
                }
            }
        } else {
            *total_files += 1;
            *total_bytes += meta.len();
        }
    }
}

fn emit_progress(app: &AppHandle, p: &JobProgress) {
    let _ = app.emit("fs:job:progress", p);
}

struct Counters {
    total_files: u64,
    done_files: u64,
    total_bytes: u64,
    done_bytes: u64,
    last_emit: Instant,
}

fn maybe_emit(app: &AppHandle, kind: &str, job_id: u64, c: &mut Counters, current: &str, force: bool) {
    if !force && c.last_emit.elapsed().as_millis() < 80 { return; }
    c.last_emit = Instant::now();
    emit_progress(app, &JobProgress {
        job_id,
        kind: kind.into(),
        phase: "run".into(),
        total_files: c.total_files,
        done_files: c.done_files,
        total_bytes: c.total_bytes,
        done_bytes: c.done_bytes,
        current: current.into(),
    });
}

fn copy_file_with_progress(
    src: &Path,
    dst: &Path,
    cancel: &AtomicBool,
    app: &AppHandle,
    job_id: u64,
    kind: &str,
    c: &mut Counters,
) -> AppResult<()> {
    use std::io::{Read, Write};
    if let Some(parent) = dst.parent() { fs::create_dir_all(parent)?; }
    let mut sf = fs::File::open(src)?;
    let mut df = fs::File::create(dst)?;
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        if cancel.load(Ordering::SeqCst) { return Err(AppError::Other("canceled".into())); }
        let n = sf.read(&mut buf)?;
        if n == 0 { break; }
        df.write_all(&buf[..n])?;
        c.done_bytes += n as u64;
        maybe_emit(app, kind, job_id, c, &src.display().to_string(), false);
    }
    df.flush()?;
    c.done_files += 1;
    Ok(())
}

fn copy_recursive(
    src: &Path,
    dst: &Path,
    cancel: &AtomicBool,
    app: &AppHandle,
    job_id: u64,
    kind: &str,
    c: &mut Counters,
) -> AppResult<()> {
    if cancel.load(Ordering::SeqCst) { return Err(AppError::Other("canceled".into())); }
    let meta = fs::symlink_metadata(src)?;
    if meta.is_dir() {
        fs::create_dir_all(dst)?;
        for ent in fs::read_dir(src)? {
            let ent = ent?;
            let from = ent.path();
            let to = dst.join(ent.file_name());
            copy_recursive(&from, &to, cancel, app, job_id, kind, c)?;
        }
    } else {
        copy_file_with_progress(src, dst, cancel, app, job_id, kind, c)?;
    }
    Ok(())
}

fn delete_recursive(
    path: &Path,
    cancel: &AtomicBool,
    app: &AppHandle,
    job_id: u64,
    c: &mut Counters,
) -> AppResult<()> {
    if cancel.load(Ordering::SeqCst) { return Err(AppError::Other("canceled".into())); }
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        for ent in fs::read_dir(path)? {
            let ent = ent?;
            delete_recursive(&ent.path(), cancel, app, job_id, c)?;
        }
        fs::remove_dir(path)?;
    } else {
        let sz = meta.len();
        fs::remove_file(path)?;
        c.done_bytes += sz;
        c.done_files += 1;
        maybe_emit(app, "delete", job_id, c, &path.display().to_string(), false);
    }
    Ok(())
}

fn run_job<F>(
    app: AppHandle,
    state: State<'_, JobRegistry>,
    job_id: u64,
    kind: &str,
    items_for_scan: Vec<PathBuf>,
    body: F,
) -> AppResult<()>
where
    F: FnOnce(&Arc<AtomicBool>, &AppHandle, &mut Counters) -> AppResult<()>,
{
    let cancel = state.register(job_id);

    // scan
    let mut total_files = 0u64;
    let mut total_bytes = 0u64;
    for p in &items_for_scan { scan_size(p, &mut total_files, &mut total_bytes); }
    let mut c = Counters { total_files, done_files: 0, total_bytes, done_bytes: 0, last_emit: Instant::now() };
    emit_progress(&app, &JobProgress {
        job_id, kind: kind.into(), phase: "scan".into(),
        total_files, done_files: 0, total_bytes, done_bytes: 0, current: String::new(),
    });

    let result = body(&cancel, &app, &mut c);

    let canceled = cancel.load(Ordering::SeqCst);
    state.unregister(job_id);

    let (ok, err) = match &result {
        Ok(_) => (true, None),
        Err(e) => (false, Some(e.to_string())),
    };
    let _ = app.emit("fs:job:done", JobDone {
        job_id, kind: kind.into(), ok, canceled,
        error: err,
        total_files: c.total_files, done_files: c.done_files,
        total_bytes: c.total_bytes, done_bytes: c.done_bytes,
    });
    if canceled { return Err(AppError::Other("canceled".into())); }
    result
}

#[tauri::command]
pub async fn job_copy(
    job_id: u64,
    items: Vec<JobItem>,
    app: AppHandle,
    state: State<'_, JobRegistry>,
) -> AppResult<()> {
    let scan_paths: Vec<PathBuf> = items.iter().map(|i| PathBuf::from(&i.from)).collect();
    run_job(app.clone(), state, job_id, "copy", scan_paths, |cancel, app, c| {
        for it in &items {
            copy_recursive(Path::new(&it.from), Path::new(&it.to), cancel, app, job_id, "copy", c)?;
        }
        Ok(())
    })
}

#[tauri::command]
pub async fn job_move(
    job_id: u64,
    items: Vec<JobItem>,
    app: AppHandle,
    state: State<'_, JobRegistry>,
) -> AppResult<()> {
    let scan_paths: Vec<PathBuf> = items.iter().map(|i| PathBuf::from(&i.from)).collect();
    run_job(app.clone(), state, job_id, "move", scan_paths, |cancel, app, c| {
        for it in &items {
            let src = Path::new(&it.from);
            let dst = Path::new(&it.to);
            if let Some(parent) = dst.parent() { fs::create_dir_all(parent)?; }
            // 同一ボリュームなら rename を試みる (高速)
            if fs::rename(src, dst).is_ok() {
                let meta = fs::symlink_metadata(dst)?;
                if meta.is_file() {
                    c.done_files += 1;
                    c.done_bytes += meta.len();
                } else {
                    let mut tf = 0u64; let mut tb = 0u64;
                    scan_size(dst, &mut tf, &mut tb);
                    c.done_files += tf;
                    c.done_bytes += tb;
                }
                maybe_emit(app, "move", job_id, c, &it.to, true);
                continue;
            }
            // 別ボリューム: copy + delete
            copy_recursive(src, dst, cancel, app, job_id, "move", c)?;
            // src 側は削除 (進捗には含めず)
            if src.is_dir() { fs::remove_dir_all(src)?; } else { fs::remove_file(src)?; }
        }
        Ok(())
    })
}

#[tauri::command]
pub async fn job_delete(
    job_id: u64,
    paths: Vec<String>,
    app: AppHandle,
    state: State<'_, JobRegistry>,
) -> AppResult<()> {
    let scan_paths: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    run_job(app.clone(), state, job_id, "delete", scan_paths, |cancel, app, c| {
        for p in &paths {
            delete_recursive(Path::new(p), cancel, app, job_id, c)?;
        }
        Ok(())
    })
}
