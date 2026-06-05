//! ホットキー設定 (コマンド系キー割り当てのカスタマイズ)。
//!
//! 設定ファイル: `%APPDATA%\FastFiler\gpui_hotkeys.json` (無ければ既定値で自動生成)。
//! 形式: `{ "action": "combo", ... }`。combo は `ctrl+shift+n` / `f2` / `alt+left` など。
//! 反映: 背景右クリック「ホットキーを再読み込み」または再起動。
//!
//! 矢印などの移動系 (Shift で範囲選択拡張) とモーダル内の Enter/Esc は固定。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use gpui::Keystroke;

/// カスタマイズ可能なコマンド。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HotAction {
    Open,
    Parent,
    Delete,
    Rename,
    NewFolder,
    NewFile,
    Refresh,
    Search,
    Undo,
    Copy,
    Cut,
    Paste,
    SelectAll,
    Back,
    Forward,
    NextPane,
    NextTab,
    PrevTab,
}

/// (設定キー名, アクション, 既定 combo)
const ACTIONS: &[(&str, HotAction, &str)] = &[
    ("open", HotAction::Open, "enter"),
    ("parent", HotAction::Parent, "backspace"),
    ("delete", HotAction::Delete, "delete"),
    ("rename", HotAction::Rename, "f2"),
    ("new-folder", HotAction::NewFolder, "f7"),
    ("new-file", HotAction::NewFile, "f8"),
    ("refresh", HotAction::Refresh, "f5"),
    ("search", HotAction::Search, "ctrl+f"),
    ("undo", HotAction::Undo, "ctrl+z"),
    ("copy", HotAction::Copy, "ctrl+c"),
    ("cut", HotAction::Cut, "ctrl+x"),
    ("paste", HotAction::Paste, "ctrl+v"),
    ("select-all", HotAction::SelectAll, "ctrl+a"),
    ("back", HotAction::Back, "alt+left"),
    ("forward", HotAction::Forward, "alt+right"),
    ("next-pane", HotAction::NextPane, "f6"),
    ("next-tab", HotAction::NextTab, "ctrl+tab"),
    ("prev-tab", HotAction::PrevTab, "ctrl+shift+tab"),
];

static MAP: OnceLock<RwLock<HashMap<String, HotAction>>> = OnceLock::new();

fn map() -> &'static RwLock<HashMap<String, HotAction>> {
    MAP.get_or_init(|| RwLock::new(HashMap::new()))
}

fn config_path() -> Option<PathBuf> {
    let base = std::env::var("APPDATA").ok()?;
    Some(
        PathBuf::from(base)
            .join("FastFiler")
            .join("gpui_hotkeys.json"),
    )
}

/// "Ctrl + Shift+N" → "ctrl+shift+n" に正規化。不正なら None。
fn normalize(combo: &str) -> Option<String> {
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut key = String::new();
    for part in combo.split('+') {
        let p = part.trim().to_ascii_lowercase();
        match p.as_str() {
            "ctrl" | "control" => ctrl = true,
            "alt" => alt = true,
            "shift" => shift = true,
            "" => return None,
            k => {
                if !key.is_empty() {
                    return None;
                }
                key = k.to_string();
            }
        }
    }
    if key.is_empty() {
        return None;
    }
    Some(combo_key(ctrl, alt, shift, &key))
}

fn combo_key(ctrl: bool, alt: bool, shift: bool, key: &str) -> String {
    let mut s = String::new();
    if ctrl {
        s.push_str("ctrl+");
    }
    if alt {
        s.push_str("alt+");
    }
    if shift {
        s.push_str("shift+");
    }
    s.push_str(key);
    s
}

/// 設定を読み込んで対応表を構築。ファイルが無ければ既定値で生成する。
/// 不正な combo はそのアクションだけ既定値へフォールバック。
pub fn load() {
    let mut user: HashMap<String, String> = HashMap::new();
    if let Some(p) = config_path() {
        match std::fs::read_to_string(&p) {
            Ok(text) => {
                if let Ok(v) = serde_json::from_str::<HashMap<String, String>>(&text) {
                    user = v;
                }
            }
            Err(_) => {
                // 既定値でファイルを生成 (編集 → 再読み込み/再起動で反映)。
                if let Some(dir) = p.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let mut sample = serde_json::Map::new();
                sample.insert(
                    "_help".into(),
                    serde_json::Value::String(
                        "action: \"combo\" 形式。修飾は ctrl+ / alt+ / shift+、キーは \
                         a-z 0-9 f1-f12 enter backspace delete tab space left right up down \
                         home end pageup pagedown など (小文字)。"
                            .into(),
                    ),
                );
                for (name, _, def) in ACTIONS {
                    sample.insert((*name).into(), serde_json::Value::String((*def).into()));
                }
                let _ = std::fs::write(
                    &p,
                    serde_json::to_string_pretty(&serde_json::Value::Object(sample))
                        .unwrap_or_default(),
                );
            }
        }
    }
    let mut m = HashMap::new();
    for (name, act, def) in ACTIONS {
        let combo = user.get(*name).map(|s| s.as_str()).unwrap_or(def);
        if let Some(k) = normalize(combo).or_else(|| normalize(def)) {
            m.insert(k, *act);
        }
    }
    *map().write().unwrap() = m;
}

/// キーストロークに対応するアクションを引く。
pub fn lookup(ks: &Keystroke) -> Option<HotAction> {
    let k = combo_key(
        ks.modifiers.control,
        ks.modifiers.alt,
        ks.modifiers.shift,
        &ks.key.to_ascii_lowercase(),
    );
    map().read().unwrap().get(&k).copied()
}

/// 設定ファイルのパス (メニューから開く用)。無ければ生成してから返す。
pub fn config_file() -> Option<String> {
    let p = config_path()?;
    if !p.is_file() {
        load(); // 副作用でファイル生成
    }
    Some(p.to_string_lossy().to_string())
}
