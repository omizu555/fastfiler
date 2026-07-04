//! ホットキー設定 (コマンド系キー割り当てのカスタマイズ — GPUI 版 hotkeys.rs 互換)。
//!
//! 設定ファイル: `%APPDATA%\FastFiler\hotkeys.json` (無ければ旧ファイル
//! (iced_/gpui_ 接頭辞) から自動移行 (N-05)、それも無ければ既定値で自動生成)。
//! 形式: `{ "action": "combo", ... }`。combo は `ctrl+shift+n` / `f2` / `alt+left` など。
//! 反映: 設定画面の「再読み込み」または再起動。
//!
//! 矢印などの移動系 (Shift で範囲選択拡張) とモーダル内の Enter/Esc は固定。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

use iced::keyboard::{key::Named, Key, Modifiers};
use once_cell::sync::Lazy;

/// カスタマイズ可能なコマンド (GPUI 版 HotAction と同一)。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

/// (設定キー名, アクション, 既定 combo) — GPUI 版と同一。
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

/// 正規化した combo → アクション。
static MAP: Lazy<RwLock<HashMap<Combo, HotAction>>> = Lazy::new(|| RwLock::new(load()));

/// 正規化済みキー組み合わせ。文字キーは小文字 1 文字、特殊キーは名前。
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct Combo {
    ctrl: bool,
    shift: bool,
    alt: bool,
    key: String,
}

/// `ctrl+shift+n` / `f2` / `alt+left` を正規化する。
fn parse_combo(s: &str) -> Option<Combo> {
    let mut ctrl = false;
    let mut shift = false;
    let mut alt = false;
    let mut key = None;
    for part in s.split('+') {
        match part.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => ctrl = true,
            "shift" => shift = true,
            "alt" => alt = true,
            "" => return None,
            k => key = Some(k.to_string()),
        }
    }
    key.map(|key| Combo {
        ctrl,
        shift,
        alt,
        key,
    })
}

/// iced のキーイベントを正規化キー名へ (parse_combo と同じ語彙)。
fn key_name(key: &Key<&str>) -> Option<String> {
    Some(match key {
        Key::Character(c) => c.to_ascii_lowercase(),
        Key::Named(n) => match n {
            Named::Enter => "enter".into(),
            Named::Backspace => "backspace".into(),
            Named::Delete => "delete".into(),
            Named::Tab => "tab".into(),
            Named::Space => "space".into(),
            Named::ArrowLeft => "left".into(),
            Named::ArrowRight => "right".into(),
            Named::ArrowUp => "up".into(),
            Named::ArrowDown => "down".into(),
            Named::Home => "home".into(),
            Named::End => "end".into(),
            Named::PageUp => "pageup".into(),
            Named::PageDown => "pagedown".into(),
            Named::Insert => "insert".into(),
            Named::F1 => "f1".into(),
            Named::F2 => "f2".into(),
            Named::F3 => "f3".into(),
            Named::F4 => "f4".into(),
            Named::F5 => "f5".into(),
            Named::F6 => "f6".into(),
            Named::F7 => "f7".into(),
            Named::F8 => "f8".into(),
            Named::F9 => "f9".into(),
            Named::F10 => "f10".into(),
            Named::F11 => "f11".into(),
            Named::F12 => "f12".into(),
            _ => return None,
        },
        _ => return None,
    })
}

fn config_dir() -> Option<PathBuf> {
    let base = std::env::var("APPDATA").ok()?;
    Some(PathBuf::from(base).join("FastFiler"))
}

fn defaults_json() -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for (name, _, combo) in ACTIONS {
        m.insert(name.to_string(), serde_json::json!(combo));
    }
    serde_json::Value::Object(m)
}

fn load() -> HashMap<Combo, HotAction> {
    let raw: HashMap<String, String> = (|| {
        let dir = config_dir()?;
        let own = dir.join("hotkeys.json");
        if let Ok(text) = std::fs::read_to_string(&own) {
            return serde_json::from_str(&text).ok();
        }
        // 初回: 開発期の iced_ 名 → GPUI 版から自動移行して hotkeys.json を生成
        let migrated: Option<HashMap<String, String>> =
            ["iced_hotkeys.json", "gpui_hotkeys.json"].iter().find_map(|n| {
                std::fs::read_to_string(dir.join(n))
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok())
            });
        let value = match &migrated {
            Some(m) => serde_json::to_value(m).unwrap_or_else(|_| defaults_json()),
            None => defaults_json(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&value) {
            let _ = std::fs::create_dir_all(&dir);
            let _ = fastfiler_core::persist::write_atomic(&own, &json);
        }
        migrated.or_else(|| serde_json::from_value(defaults_json()).ok())
    })()
    .unwrap_or_default();

    let mut map = HashMap::new();
    for (name, action, default) in ACTIONS {
        let combo = raw.get(*name).map(|s| s.as_str()).unwrap_or(default);
        if let Some(c) = parse_combo(combo) {
            map.insert(c, *action);
        }
    }
    map
}

/// キー入力からコマンドを解決する (見つからなければ None = 固定キー処理へ)。
pub fn action_for(key: &Key<&str>, m: Modifiers) -> Option<HotAction> {
    let combo = Combo {
        ctrl: m.control(),
        shift: m.shift(),
        alt: m.alt(),
        key: key_name(key)?,
    };
    MAP.read().ok()?.get(&combo).copied()
}

/// hotkeys.json を再読込する。
pub fn reload() {
    let new = load();
    if let Ok(mut g) = MAP.write() {
        *g = new;
    }
}

/// 設定ファイルパス (設定画面の「ファイルを開く」用)。
pub fn config_path() -> Option<PathBuf> {
    Some(config_dir()?.join("hotkeys.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combo_parse_normalizes() {
        assert_eq!(
            parse_combo("Ctrl+Shift+N"),
            Some(Combo {
                ctrl: true,
                shift: true,
                alt: false,
                key: "n".into()
            })
        );
        assert_eq!(
            parse_combo("f2"),
            Some(Combo {
                ctrl: false,
                shift: false,
                alt: false,
                key: "f2".into()
            })
        );
        assert_eq!(
            parse_combo("alt+left"),
            Some(Combo {
                ctrl: false,
                shift: false,
                alt: true,
                key: "left".into()
            })
        );
        assert_eq!(parse_combo(""), None);
    }

    #[test]
    fn key_names_match_combo_vocabulary() {
        assert_eq!(
            key_name(&Key::Character("N")).as_deref(),
            Some("n") // CapsLock/Shift でも小文字に正規化
        );
        assert_eq!(key_name(&Key::Named(Named::F2)).as_deref(), Some("f2"));
        assert_eq!(
            key_name(&Key::Named(Named::ArrowLeft)).as_deref(),
            Some("left")
        );
    }
}
