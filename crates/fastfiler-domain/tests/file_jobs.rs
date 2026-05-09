// Phase 2C-1: file_jobs の進捗 emit / キャンセルを NullSink + キャプチャ sink で検証。
use fastfiler_domain::events::EventSink;
use fastfiler_domain::file_jobs::{JobItem, JobRegistry};
use parking_lot::Mutex;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

fn s(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

/// 受信した event 名を順に積むテスト用 sink。
#[derive(Default)]
struct CaptureSink(Mutex<Vec<String>>);

impl EventSink for CaptureSink {
    fn emit_json(&self, event: &str, _payload: serde_json::Value) {
        self.0.lock().push(event.to_string());
    }
}

#[test]
fn run_copy_emits_progress_and_done() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("a.txt");
    let dst = tmp.path().join("b.txt");
    fs::write(&src, b"hello world").unwrap();

    let sink = Arc::new(CaptureSink::default());
    let reg = JobRegistry::default();
    let item = JobItem { from: s(&src), to: s(&dst) };
    reg.run_copy(sink.as_ref(), 1, vec![item]).unwrap();

    assert_eq!(fs::read(&dst).unwrap(), b"hello world");
    let evs = sink.0.lock().clone();
    assert!(evs.iter().any(|e| e == "fs:job:progress"));
    assert!(evs.last().map(|e| e.as_str()) == Some("fs:job:done"));
}

#[test]
fn run_move_relocates_file() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("a.txt");
    let dst = tmp.path().join("moved.txt");
    fs::write(&src, b"x").unwrap();

    let sink = Arc::new(CaptureSink::default());
    let reg = JobRegistry::default();
    let item = JobItem { from: s(&src), to: s(&dst) };
    reg.run_move(sink.as_ref(), 2, vec![item]).unwrap();

    assert!(!src.exists());
    assert_eq!(fs::read(&dst).unwrap(), b"x");
}

#[test]
fn run_delete_removes_file_and_emits_done() {
    let tmp = TempDir::new().unwrap();
    let p = tmp.path().join("x.txt");
    fs::write(&p, b"x").unwrap();

    let sink = Arc::new(CaptureSink::default());
    let reg = JobRegistry::default();
    reg.run_delete(sink.as_ref(), 3, vec![s(&p)]).unwrap();

    assert!(!p.exists());
    let evs = sink.0.lock().clone();
    assert_eq!(evs.last().map(|e| e.as_str()), Some("fs:job:done"));
}

#[test]
fn cancel_returns_false_for_unknown_job() {
    let reg = JobRegistry::default();
    assert!(!reg.cancel(9999));
}
