// v3.5: ターミナル統合 (portable-pty)
use parking_lot::Mutex;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

pub struct TermSession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    /// 子プロセスを保持。drop で終了
    _child: Arc<Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>>,
}

#[derive(Default)]
pub struct TermRegistry {
    sessions: Mutex<HashMap<u64, Arc<Mutex<TermSession>>>>,
    next_id: Mutex<u64>,
}

#[derive(Serialize, Clone)]
struct TermDataPayload {
    id: u64,
    data: String,
}

#[derive(Serialize, Clone)]
struct TermExitPayload {
    id: u64,
    code: Option<i32>,
}

#[tauri::command]
pub fn term_open(
    cwd: Option<String>,
    shell: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
    app: AppHandle,
    reg: State<'_, TermRegistry>,
) -> Result<u64, String> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: rows.unwrap_or(24),
            cols: cols.unwrap_or(80),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("openpty failed: {e}"))?;

    let shell_path = shell.unwrap_or_else(|| {
        if cfg!(windows) {
            std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".into())
        } else {
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into())
        }
    });
    let mut cmd = CommandBuilder::new(shell_path);
    if let Some(c) = cwd {
        if std::path::Path::new(&c).is_dir() {
            cmd.cwd(c);
        }
    }
    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("spawn failed: {e}"))?;
    drop(pair.slave);

    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("take_writer: {e}"))?;
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("clone_reader: {e}"))?;

    let id = {
        let mut n = reg.next_id.lock();
        *n += 1;
        *n
    };

    let child_arc: Arc<Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>> =
        Arc::new(Mutex::new(Some(child)));

    let session = TermSession {
        master: pair.master,
        writer,
        _child: child_arc.clone(),
    };
    reg.sessions
        .lock()
        .insert(id, Arc::new(Mutex::new(session)));

    // 出力読み取りスレッド
    let app_for_read = app.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = app_for_read.emit("term:data", TermDataPayload { id, data });
                }
                Err(_) => break,
            }
        }
        // 終了監視
        let code = {
            let mut guard = child_arc.lock();
            if let Some(mut c) = guard.take() {
                c.wait().ok().and_then(|s| s.exit_code().try_into().ok())
            } else {
                None
            }
        };
        let _ = app_for_read.emit("term:exit", TermExitPayload { id, code });
    });

    Ok(id)
}

#[tauri::command]
pub fn term_write(id: u64, data: String, reg: State<'_, TermRegistry>) -> Result<(), String> {
    let s = reg
        .sessions
        .lock()
        .get(&id)
        .cloned()
        .ok_or_else(|| format!("term {id} not found"))?;
    let mut s = s.lock();
    s.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    s.writer.flush().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn term_resize(
    id: u64,
    cols: u16,
    rows: u16,
    reg: State<'_, TermRegistry>,
) -> Result<(), String> {
    let s = reg
        .sessions
        .lock()
        .get(&id)
        .cloned()
        .ok_or_else(|| format!("term {id} not found"))?;
    let s = s.lock();
    s.master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn term_close(id: u64, reg: State<'_, TermRegistry>) -> Result<(), String> {
    let removed = reg.sessions.lock().remove(&id);
    if let Some(s) = removed {
        let child_holder = { s.lock()._child.clone() };
        let taken = { child_holder.lock().take() };
        if let Some(mut c) = taken {
            let _ = c.kill();
        }
    }
    Ok(())
}

pub fn register(app: &AppHandle) {
    app.manage(TermRegistry::default());
}
