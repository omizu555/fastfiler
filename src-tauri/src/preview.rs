// Phase 4-b: ファイル内容取得（プレビュー用）
// テキストは UTF-8/UTF-16 LE/BE 簡易判定。最大バイト数を指定可能。

use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::io::Read;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PreviewData {
    Text { content: String, truncated: bool, encoding: String },
    Binary { hex: String, size: u64 },
    Empty,
}

const DEFAULT_LIMIT: u64 = 256 * 1024; // 256KB

#[tauri::command]
pub fn read_text_preview(path: String, max_bytes: Option<u64>) -> AppResult<PreviewData> {
    let limit = max_bytes.unwrap_or(DEFAULT_LIMIT);
    let meta = std::fs::metadata(&path)?;
    let size = meta.len();
    if size == 0 {
        return Ok(PreviewData::Empty);
    }
    let mut f = std::fs::File::open(&path)?;
    let read_len = size.min(limit) as usize;
    let mut buf = vec![0u8; read_len];
    f.read_exact(&mut buf).map_err(AppError::from)?;
    let truncated = size > limit;

    // BOM 判定
    if buf.starts_with(&[0xFF, 0xFE]) {
        let body: Vec<u16> = buf[2..]
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        let s = String::from_utf16_lossy(&body);
        return Ok(PreviewData::Text { content: s, truncated, encoding: "UTF-16LE".into() });
    }
    if buf.starts_with(&[0xFE, 0xFF]) {
        let body: Vec<u16> = buf[2..]
            .chunks_exact(2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .collect();
        let s = String::from_utf16_lossy(&body);
        return Ok(PreviewData::Text { content: s, truncated, encoding: "UTF-16BE".into() });
    }
    // UTF-8 試行
    match std::str::from_utf8(&buf) {
        Ok(s) => Ok(PreviewData::Text { content: s.to_owned(), truncated, encoding: "UTF-8".into() }),
        Err(_) => {
            // 失敗 → バイナリとして hex プレビュー（先頭 1KB のみ）
            let hex_len = buf.len().min(1024);
            let hex = buf[..hex_len].iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
            Ok(PreviewData::Binary { hex, size })
        }
    }
}
