//! domain の文字列イベント + JSON ペイロードを型付きに変換する一元パーサ
//! (計画書 §5.3 / §10-8。GPUI 版 pane.rs on_domain_event の手掘り分岐を置き換える)。
//!
//! ペイロードの形は domain 側の Serialize 構造体が正:
//! - "fs-change"      → watcher.rs `FsChange { path, kind }`
//! - "fs:job:progress" → file_jobs.rs `JobProgress`
//! - "fs:job:done"     → file_jobs.rs `JobDone`

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum DomainEvent {
    /// 監視中フォルダに変化があった (150ms デバウンスで reload する)。
    FsChange { path: String },
    JobProgress {
        job_id: u64,
        kind: String,
        done_files: u64,
        total_files: u64,
        done_bytes: u64,
        total_bytes: u64,
        current: String,
    },
    JobDone {
        job_id: u64,
        kind: String,
        ok: bool,
        canceled: bool,
        error: Option<String>,
        done_files: u64,
        total_files: u64,
    },
    SearchHit {
        job_id: u64,
        path: String,
        name: String,
        is_dir: bool,
    },
    SearchDone {
        job_id: u64,
        total: u64,
        canceled: bool,
        fallback: bool,
        error: Option<String>,
    },
    /// 未知イベント (前方互換のため落とさず無視できる形で返す)。
    Unknown { event: String },
}

pub fn parse(event: &str, payload: &Value) -> DomainEvent {
    let s = |k: &str| payload.get(k).and_then(Value::as_str).unwrap_or_default();
    let u = |k: &str| payload.get(k).and_then(Value::as_u64).unwrap_or(0);
    let b = |k: &str| payload.get(k).and_then(Value::as_bool).unwrap_or(false);
    match event {
        "fs-change" => DomainEvent::FsChange {
            path: s("path").to_string(),
        },
        "fs:job:progress" => DomainEvent::JobProgress {
            job_id: u("job_id"),
            kind: s("kind").to_string(),
            done_files: u("done_files"),
            total_files: u("total_files"),
            done_bytes: u("done_bytes"),
            total_bytes: u("total_bytes"),
            current: s("current").to_string(),
        },
        "fs:job:done" => DomainEvent::JobDone {
            job_id: u("job_id"),
            kind: s("kind").to_string(),
            ok: b("ok"),
            canceled: b("canceled"),
            error: payload
                .get("error")
                .and_then(Value::as_str)
                .map(String::from),
            done_files: u("done_files"),
            total_files: u("total_files"),
        },
        "search-hit" => DomainEvent::SearchHit {
            job_id: u("job_id"),
            path: s("path").to_string(),
            name: s("name").to_string(),
            is_dir: b("is_dir"),
        },
        "search-done" => DomainEvent::SearchDone {
            job_id: u("job_id"),
            total: u("total"),
            canceled: b("canceled"),
            fallback: b("fallback"),
            error: payload
                .get("error")
                .and_then(Value::as_str)
                .map(String::from),
        },
        other => DomainEvent::Unknown {
            event: other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_fs_change() {
        let ev = parse("fs-change", &json!({"path": "C:\\dir", "kind": "modify"}));
        assert_eq!(
            ev,
            DomainEvent::FsChange {
                path: "C:\\dir".into()
            }
        );
    }

    #[test]
    fn parses_job_progress_and_done() {
        let p = json!({
            "job_id": 7, "kind": "copy", "phase": "copy",
            "total_files": 10, "done_files": 3,
            "total_bytes": 1000, "done_bytes": 300, "current": "a.txt"
        });
        assert_eq!(
            parse("fs:job:progress", &p),
            DomainEvent::JobProgress {
                job_id: 7,
                kind: "copy".into(),
                done_files: 3,
                total_files: 10,
                done_bytes: 300,
                total_bytes: 1000,
                current: "a.txt".into(),
            }
        );
        let d = json!({
            "job_id": 7, "kind": "move", "ok": false, "canceled": false,
            "error": "アクセス拒否", "total_files": 5, "done_files": 2,
            "total_bytes": 0, "done_bytes": 0
        });
        assert_eq!(
            parse("fs:job:done", &d),
            DomainEvent::JobDone {
                job_id: 7,
                kind: "move".into(),
                ok: false,
                canceled: false,
                error: Some("アクセス拒否".into()),
                done_files: 2,
                total_files: 5,
            }
        );
    }

    #[test]
    fn unknown_event_is_preserved_not_dropped() {
        assert_eq!(
            parse("future-event", &json!({})),
            DomainEvent::Unknown {
                event: "future-event".into()
            }
        );
    }

    #[test]
    fn missing_fields_default_instead_of_panicking() {
        let ev = parse("fs:job:done", &json!({}));
        assert_eq!(
            ev,
            DomainEvent::JobDone {
                job_id: 0,
                kind: String::new(),
                ok: false,
                canceled: false,
                error: None,
                done_files: 0,
                total_files: 0,
            }
        );
    }
}
