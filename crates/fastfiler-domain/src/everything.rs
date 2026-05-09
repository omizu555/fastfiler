// Everything HTTP Server クライアント
//
// voidtools 製 Everything のローカル HTTP Server (既定 OFF) に問い合わせ、
// 検索結果を構造化して返す。
//
// 使い方:
//   1. Everything → Tools → Options → HTTP Server を Enable
//   2. ポート番号を控える (既定 80)
//   3. このクライアントから http://127.0.0.1:<PORT>/?json=1&search=... を叩く

use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct RawResponse {
    #[serde(rename = "totalResults")]
    #[allow(dead_code)]
    total_results: Option<u64>,
    results: Option<Vec<RawHit>>,
}

#[derive(Debug, Deserialize)]
struct RawHit {
    #[serde(rename = "type")]
    kind: Option<String>, // "file" | "folder"
    name: Option<String>,
    path: Option<String>,
    #[allow(dead_code)]
    size: Option<String>,
    #[allow(dead_code)]
    date_modified: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EverythingHit {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone)]
pub struct EverythingError(pub String);

impl std::fmt::Display for EverythingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Everything HTTP error: {}", self.0)
    }
}

/// Everything HTTP Server に問い合わせて結果を返す。
///
/// * `port` - HTTP Server のポート (例: 80)
/// * `query` - フロントから来た生クエリ (substring or regex)
/// * `scope` - 検索範囲のフォルダ。指定があれば `path:"<scope>"` を AND で付与
/// * `case_sensitive` - 大小区別
/// * `regex` - 正規表現として扱う
/// * `max_results` - 返却件数上限
pub fn query(
    port: u16,
    query: &str,
    scope: Option<&str>,
    case_sensitive: bool,
    regex: bool,
    max_results: usize,
) -> Result<Vec<EverythingHit>, EverythingError> {
    // クエリ組み立て: scope を path:"..." として AND
    let mut q = String::new();
    if let Some(s) = scope {
        let trimmed = s.trim_end_matches(['\\', '/']);
        if !trimmed.is_empty() {
            // 引用符の中身は二重引用符をエスケープしないが、Everything は path: にスペース含むため "" で括る
            q.push_str(&format!("path:\"{}\" ", trimmed));
        }
    }
    if regex {
        q.push_str(&format!("regex:{}", query));
    } else {
        q.push_str(query);
    }

    let url = format!(
        "http://127.0.0.1:{}/?json=1&path_column=1&size_column=1&date_modified_column=1&count={}&match_case={}&search={}",
        port,
        max_results,
        if case_sensitive { 1 } else { 0 },
        urlencoding::encode(&q)
    );

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(800))
        .timeout(Duration::from_secs(8))
        .build();

    let resp = agent
        .get(&url)
        .call()
        .map_err(|e| EverythingError(format!("request: {}", e)))?;
    let body: RawResponse = resp
        .into_json()
        .map_err(|e| EverythingError(format!("decode: {}", e)))?;

    let raw = body.results.unwrap_or_default();
    let mut out = Vec::with_capacity(raw.len());
    for r in raw {
        let name = match r.name {
            Some(n) => n,
            None => continue,
        };
        let path = match r.path {
            Some(p) => {
                if p.is_empty() {
                    name.clone()
                } else if p.ends_with('\\') || p.ends_with('/') {
                    format!("{}{}", p, name)
                } else {
                    format!("{}\\{}", p, name)
                }
            }
            None => name.clone(),
        };
        let is_dir = matches!(r.kind.as_deref(), Some("folder"));
        out.push(EverythingHit { name, path, is_dir });
    }
    Ok(out)
}

/// Everything が応答するかを軽く確認 (ping)
pub fn ping(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{}/?json=1&count=0&search=", port);
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(400))
        .timeout(Duration::from_secs(2))
        .build();
    matches!(agent.get(&url).call(), Ok(_))
}
