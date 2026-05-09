// 軽量動作ログ。`flog!` マクロで書き込み。
//
// 出力先: %APPDATA%/FastFiler/fastfiler.log (なければカレントディレクトリ)
// 起動時に init() を呼び、open / clear する (必要に応じてローテート)。

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use std::sync::OnceLock;

static LOG_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();

pub fn log_path() -> PathBuf {
    if let Some(base) = dirs::config_dir() {
        return base.join("FastFiler").join("fastfiler.log");
    }
    PathBuf::from("fastfiler.log")
}

pub fn init() {
    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // 起動時にローテート (前回ログを .1 に退避)
    if path.exists() {
        let prev = path.with_extension("log.1");
        let _ = std::fs::remove_file(&prev);
        let _ = std::fs::rename(&path, &prev);
    }
    let f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok();
    let _ = LOG_FILE.set(Mutex::new(f));
    // panic も拾う
    std::panic::set_hook(Box::new(|info| {
        write_line(format!("PANIC: {}", info));
        eprintln!("PANIC logged: {}", info);
    }));
    write_line(format!("=== FastFiler started (pid={}) ===", std::process::id()));
}

pub fn write_line(msg: String) {
    eprintln!("{}", msg);
    if let Some(cell) = LOG_FILE.get() {
        if let Ok(mut g) = cell.lock() {
            if let Some(f) = g.as_mut() {
                let ts = chrono_like_now();
                let _ = writeln!(f, "{} {}", ts, msg);
                let _ = f.flush();
            }
        }
    }
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs() as i64;
    let ms = dur.subsec_millis();
    // JST (UTC+9)
    let jst = secs + 9 * 3600;
    let day = jst / 86400;
    let sod = jst % 86400;
    let hh = sod / 3600;
    let mm = (sod % 3600) / 60;
    let ss = sod % 60;
    // 簡易日付計算 (1970-01-01 起点)
    let (y, mo, d) = days_to_ymd(day);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}+09:00",
        y, mo, d, hh, mm, ss, ms
    )
}

fn days_to_ymd(mut days: i64) -> (i64, u32, u32) {
    let mut y: i64 = 1970;
    loop {
        let dy = if is_leap(y) { 366 } else { 365 };
        if days < dy {
            break;
        }
        days -= dy;
        y += 1;
    }
    let dim = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut mo: u32 = 0;
    while mo < 12 {
        let mut dm = dim[mo as usize] as i64;
        if mo == 1 && is_leap(y) {
            dm = 29;
        }
        if days < dm {
            break;
        }
        days -= dm;
        mo += 1;
    }
    (y, mo + 1, (days + 1) as u32)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

#[macro_export]
macro_rules! flog {
    ($($arg:tt)*) => {{
        $crate::logger::write_line(format!($($arg)*));
    }};
}
