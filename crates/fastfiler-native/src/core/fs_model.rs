//! ファイル/ディレクトリの値型と純粋関数群。
//!
//! UI/シグナルに依存しない、ドメインに近いユーティリティ層。
//! ここに置くもの:
//!   - 1 行を表す `FileRow`
//!   - ロード統計 `Stats`
//!   - ソートキー `SortKey`
//!   - 履歴 `History`
//!   - フォーマッタ (`human_size` / `format_mtime` 等)
//!   - フォルダ列挙 (`read_folder` / `sort_rows`)
//!   - ドライブ列挙 (`list_drives`) と初期パス推定 (`initial_path` / `pretty_title`)
//!   - 同名衝突を避ける宛先名生成 (`unique_dest`)

use std::path::{Path, PathBuf};

use fastfiler_domain::fs as ffs;

#[derive(Clone, Debug)]
pub struct FileRow {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: i64,
    pub size_text: String,
    pub mtime_text: String,
    /// 検索結果用の絶対パス。通常 (read_folder) では None。
    /// Everything 検索結果や builtin filter 表示で path 列を出すために使用。
    pub full_path: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct Stats {
    pub load_ms: f64,
    pub count: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortKey {
    Name,
    Size,
    Modified,
}

#[derive(Default, Clone)]
pub struct History {
    pub back: im::Vector<PathBuf>,
    pub forward: im::Vector<PathBuf>,
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.1} {}", v, UNITS[u])
    }
}

pub fn format_mtime(unix_secs: i64) -> String {
    if unix_secs <= 0 {
        return String::new();
    }
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(unix_secs, 0) {
        chrono::LocalResult::Single(dt) => format_dt(dt),
        chrono::LocalResult::Ambiguous(dt, _) => format_dt(dt),
        chrono::LocalResult::None => String::new(),
    }
}

pub fn format_dt(dt: chrono::DateTime<chrono::Local>) -> String {
    dt.format("%Y-%m-%d %H:%M").to_string()
}

pub fn read_folder(path: &Path, show_hidden: bool) -> Result<im::Vector<FileRow>, String> {
    let s = path.to_string_lossy().into_owned();
    let entries = ffs::list_dir(s).map_err(|e| e.to_string())?;
    let tmp: Vec<FileRow> = entries
        .into_iter()
        .filter(|e| show_hidden || !e.hidden)
        .map(|e| {
            let is_dir = e.kind == "dir";
            let size_text = if is_dir {
                String::from("<DIR>")
            } else {
                human_size(e.size)
            };
            let mtime_text = format_mtime(e.modified);
            FileRow {
                name: e.name,
                is_dir,
                size: e.size,
                modified: e.modified,
                size_text,
                mtime_text,
                full_path: None,
            }
        })
        .collect();
    Ok(tmp.into())
}

pub fn sort_rows(rows: &mut im::Vector<FileRow>, key: SortKey, desc: bool) {
    let mut tmp: Vec<FileRow> = rows.iter().cloned().collect();
    tmp.sort_by(|a, b| {
        // ディレクトリ優先 (常に上)
        match (a.is_dir, b.is_dir) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
        let ord = match key {
            SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortKey::Size => a.size.cmp(&b.size),
            SortKey::Modified => a.modified.cmp(&b.modified),
        };
        if desc {
            ord.reverse()
        } else {
            ord
        }
    });
    *rows = tmp.into();
}

#[cfg(windows)]
pub fn list_drives() -> Vec<String> {
    let mut out = Vec::new();
    for c in b'C'..=b'Z' {
        let p = format!("{}:\\", c as char);
        if Path::new(&p).is_dir() {
            out.push(p);
        }
    }
    out
}
#[cfg(not(windows))]
pub fn list_drives() -> Vec<String> {
    vec![String::from("/")]
}

pub fn initial_path() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn pretty_title(p: &Path) -> String {
    let s = p.to_string_lossy();
    let trimmed = s.trim_end_matches(['\\', '/']);
    // UNC パス (\\server\share\... or //server/share/...)
    let is_unc = trimmed.starts_with("\\\\") || trimmed.starts_with("//");
    // ドライブ文字 (C:, D:, ...) を抽出
    let drive: Option<char> = {
        let bytes = trimmed.as_bytes();
        if bytes.len() >= 2 && bytes[1] == b':' && (bytes[0] as char).is_ascii_alphabetic() {
            Some((bytes[0] as char).to_ascii_uppercase())
        } else {
            None
        }
    };
    let name = p
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| trimmed.to_string());
    if is_unc {
        // ネットワーク印 (グローブ絵文字)
        format!("🌐 {}", name)
    } else if let Some(d) = drive {
        // ドライブルート ("C:" 等) は重複を避けて "C:\" 表示
        if name.len() <= 2 && name.starts_with(d) {
            format!("{}:\\", d)
        } else {
            format!("{}: {}", d, name)
        }
    } else {
        name
    }
}

/// 同名ファイルがあれば " (2)", " (3)"... を付与してユニークな宛先を返す。
pub fn unique_dest(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (base, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for n in 2..=9999u32 {
        let cand = dir.join(format!("{} ({}){}", base, n, ext));
        if !cand.exists() {
            return cand;
        }
    }
    p
}
