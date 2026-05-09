// Hotkey 反映: settings.hotkeys (action -> "Ctrl+Shift+N" 形式) を
// パースして KeyCombo の Vec にし、KeyDown を受けて action 名に解決、
// dispatch_action で AppState/PaneState の対応メソッドを呼ぶ。

use floem::keyboard::{Key, KeyEvent, Modifiers, NamedKey};
use floem::prelude::*;
use floem::reactive::SignalWith;

use crate::state::AppState;

#[derive(Clone, Debug)]
pub struct KeyCombo {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
    pub key: ComboKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComboKey {
    Named(NamedKey),
    /// 比較は小文字化済みの 1 文字以上の文字列として保持
    Char(String),
}

pub fn parse_combo(spec: &str) -> Option<KeyCombo> {
    let mut combo = KeyCombo {
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
        key: ComboKey::Char(String::new()),
    };
    let mut key_set = false;
    for raw in spec.split('+') {
        let part = raw.trim();
        if part.is_empty() {
            return None;
        }
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => combo.ctrl = true,
            "shift" => combo.shift = true,
            "alt" => combo.alt = true,
            "meta" | "win" | "cmd" | "super" => combo.meta = true,
            other => {
                let named = match other {
                    "enter" | "return" => Some(NamedKey::Enter),
                    "tab" => Some(NamedKey::Tab),
                    "esc" | "escape" => Some(NamedKey::Escape),
                    "backspace" | "back" => Some(NamedKey::Backspace),
                    "delete" | "del" => Some(NamedKey::Delete),
                    "space" => Some(NamedKey::Space),
                    "left" => Some(NamedKey::ArrowLeft),
                    "right" => Some(NamedKey::ArrowRight),
                    "up" => Some(NamedKey::ArrowUp),
                    "down" => Some(NamedKey::ArrowDown),
                    "home" => Some(NamedKey::Home),
                    "end" => Some(NamedKey::End),
                    "pageup" | "pgup" => Some(NamedKey::PageUp),
                    "pagedown" | "pgdn" => Some(NamedKey::PageDown),
                    "f1" => Some(NamedKey::F1),
                    "f2" => Some(NamedKey::F2),
                    "f3" => Some(NamedKey::F3),
                    "f4" => Some(NamedKey::F4),
                    "f5" => Some(NamedKey::F5),
                    "f6" => Some(NamedKey::F6),
                    "f7" => Some(NamedKey::F7),
                    "f8" => Some(NamedKey::F8),
                    "f9" => Some(NamedKey::F9),
                    "f10" => Some(NamedKey::F10),
                    "f11" => Some(NamedKey::F11),
                    "f12" => Some(NamedKey::F12),
                    _ => None,
                };
                combo.key = match named {
                    Some(n) => ComboKey::Named(n),
                    None => ComboKey::Char(other.to_string()),
                };
                key_set = true;
            }
        }
    }
    if !key_set {
        return None;
    }
    Some(combo)
}

fn mods_match(combo: &KeyCombo, mods: &Modifiers) -> bool {
    combo.ctrl == mods.control()
        && combo.alt == mods.alt()
        && combo.meta == mods.meta()
        // shift は Char キーで自動付与される事があるので緩めに比較
        && (combo.shift == mods.shift() || matches!(combo.key, ComboKey::Char(_)))
}

fn key_match(combo: &KeyCombo, ke: &KeyEvent) -> bool {
    match (&combo.key, &ke.key.logical_key) {
        (ComboKey::Named(a), Key::Named(b)) => a == b,
        (ComboKey::Char(a), Key::Character(b)) => a.eq_ignore_ascii_case(b),
        _ => false,
    }
}

/// KeyEvent から action 名を解決。
pub fn resolve_action(app: &AppState, ke: &KeyEvent) -> Option<String> {
    let mods = &ke.modifiers;
    app.settings.hotkeys.with(|list| {
        for (action, sig) in list.iter() {
            let spec = sig.get_untracked();
            if spec.trim().is_empty() {
                continue;
            }
            if let Some(c) = parse_combo(&spec) {
                if mods_match(&c, mods) && key_match(&c, ke) {
                    return Some(action.clone());
                }
            }
        }
        None
    })
}

/// action 名を AppState/PaneState の操作にディスパッチ。
/// 処理した場合 true、無視した場合 false。
pub fn dispatch_action(app: &AppState, action: &str) -> bool {
    let pane = app.active_pane();
    match action {
        // ─────────── ペイン操作 ───────────
        "open" => {
            if let Some(p) = pane {
                if let Some(idx) = p.anchor.get_untracked() {
                    let row_opt = p.rows.with_untracked(|v| v.get(idx).cloned());
                    if let Some(row) = row_opt {
                        let cur = p.cur_path.get_untracked();
                        let target = cur.join(&row.name);
                        if row.is_dir {
                            p.navigate(target, true);
                        } else {
                            let _ = fastfiler_domain::shell::open_with_shell(
                                target.to_string_lossy().into_owned(),
                            );
                        }
                    }
                }
            }
        }
        "parent" => { if let Some(p) = pane { p.up(); } }
        "refresh" => { if let Some(p) = pane { p.reload(); } }
        "rename" => { if let Some(p) = pane { p.open_rename_modal(); } }
        "delete" => { if let Some(p) = pane { p.delete_selected(); } }
        "delete-permanent" => {
            if let Some(p) = pane {
                p.status_msg.set(String::from("(delete-permanent 未実装)"));
            }
        }
        "new-folder" => { if let Some(p) = pane { p.open_new_folder_modal(); } }
        "cut" => { if let Some(p) = pane { p.clipboard_write("move"); } }
        "copy" => { if let Some(p) = pane { p.clipboard_write("copy"); } }
        "paste" => { if let Some(p) = pane { p.clipboard_paste(); } }
        "select-all" => { if let Some(p) = pane { p.select_all(); } }
        "address-bar" => {
            if let Some(p) = pane {
                p.status_msg.set(String::from("(address-bar focus 未実装)"));
            }
        }
        "pane-back" => { if let Some(p) = pane { p.back(); } }
        "pane-forward" => { if let Some(p) = pane { p.forward(); } }
        // ─────────── タブ操作 ───────────
        "new-tab" => {
            let start = app
                .active_pane()
                .map(|p| p.cur_path.get_untracked())
                .unwrap_or_else(crate::fs_model::initial_path);
            app.add_tab(start);
        }
        "close-tab" => {
            let id = app.active.get_untracked();
            app.close_tab(id);
        }
        "next-tab" | "prev-tab" => {
            let dir = if action == "next-tab" { 1isize } else { -1 };
            let tabs = app.tabs.get_untracked();
            if !tabs.is_empty() {
                let cur_id = app.active.get_untracked();
                let cur = tabs.iter().position(|t| t.id == cur_id).unwrap_or(0) as isize;
                let n = tabs.len() as isize;
                let next = ((cur + dir) % n + n) % n;
                let next_id = tabs[next as usize].id;
                if next_id != cur_id {
                    app.active.set(next_id);
                }
            }
        }
        // ─────────── 設定 ───────────
        "open-settings" => app.settings_open.set(true),
        // ─────────── 未実装 (status_msg にだけ流す) ───────────
        "search" | "toggle-preview" | "toggle-plugin" | "toggle-tabs"
        | "toggle-tree" | "undo" | "toggle-terminal" => {
            if let Some(p) = pane {
                p.status_msg.set(format!("(action '{}' 未実装)", action));
            }
        }
        _ => return false,
    }
    true
}
