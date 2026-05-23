//! AppSettings: ランタイム上の設定値 (各フィールドは [`floem::reactive::RwSignal`])。
//!
//! - 起動時に [`super::persisted::PersistedSettings`] からロード → `from_persisted` で初期化
//! - [`AppSettings::save`] で現在値を `PersistedSettings::from_app` に変換し RON 保存
//! - 各シグナルは UI / effect から直接 read/write される

use floem::prelude::*;

use super::persisted::{settings_path, PersistedSettings};

#[derive(Clone)]
pub struct AppSettings {
    // General
    pub initial_path: RwSignal<String>,
    pub show_hidden: RwSignal<bool>,
    pub show_thumbnails: RwSignal<bool>,
    pub show_preview: RwSignal<bool>,
    pub show_plugin_panel: RwSignal<bool>,
    pub hide_pane_toolbar: RwSignal<bool>,
    pub theme: RwSignal<String>,        // "system" | "dark" | "light"
    pub theme_preset: RwSignal<String>, // "default" | "dracula" | ...
    pub accent_color: RwSignal<String>, // "#rrggbb" or ""
    pub icon_set: RwSignal<String>,     // "emoji" | "minimal" | "colored"
    pub icon_pack: RwSignal<String>,    // "default" | "emoji" | ...
    pub ui_font: RwSignal<String>,
    pub ui_font_size: RwSignal<String>, // 文字列で保持 (text_input 用)

    // Workspace
    pub tab_columns: RwSignal<String>, // "1".."4"
    pub tabs_width: RwSignal<String>,
    pub tree_width: RwSignal<String>,
    pub same_panel_stack: RwSignal<bool>,
    pub workspace_layout: RwSignal<String>, // "tabsLeft" | "tabsRight" | "tabsHidden"
    pub panel_dock_tabs: RwSignal<String>,  // "left" | "right" | "top" | "bottom" | "hidden"
    pub panel_dock_tree: RwSignal<String>,

    // Search
    pub search_backend: RwSignal<String>, // "builtin" | "everything"
    pub everything_port: RwSignal<String>,
    pub everything_scope: RwSignal<bool>,

    // Terminal
    pub show_terminal: RwSignal<bool>,
    pub terminal_height: RwSignal<String>,
    pub terminal_shell: RwSignal<String>,
    pub terminal_font: RwSignal<String>,
    pub terminal_font_size: RwSignal<String>,

    // Hotkeys (action -> combo)
    pub hotkeys: RwSignal<im::Vector<(String, RwSignal<String>)>>,

    // Plugins (ID -> enabled)
    pub plugins_enabled: RwSignal<im::Vector<(String, RwSignal<bool>)>>,

    // Window state (位置/サイズ復元用)
    pub window_x: RwSignal<Option<i32>>,
    pub window_y: RwSignal<Option<i32>>,
    pub window_w: RwSignal<Option<u32>>,
    pub window_h: RwSignal<Option<u32>>,
    pub window_maximized: RwSignal<bool>,

    // 開いていたタブのパス一覧 (起動時復元用)
    pub open_tabs: RwSignal<Vec<String>>,
    /// 各タブの BSP レイアウト JSON 文字列 (open_tabs と同順)。空なら open_tabs から復元 (単一ペイン)。
    pub tab_layouts: RwSignal<Vec<String>>,
    /// 各タブのロック状態 (open_tabs と同順)。長さが合わない場合は false 扱い。
    pub tab_locked: RwSignal<Vec<bool>>,
    /// ワークスペースツリーに登録された UNC share root (正規化済 `\\server\share`)。
    pub tree_unc_shares: RwSignal<Vec<String>>,
}

impl AppSettings {
    pub fn new() -> Self {
        let persisted = PersistedSettings::load().unwrap_or_default();
        Self::from_persisted(&persisted)
    }

    fn from_persisted(p: &PersistedSettings) -> Self {
        let hotkeys: im::Vector<(String, RwSignal<String>)> = {
            let defaults = default_hotkeys();
            // 永続化された hotkeys をマップ化、なければデフォルトを使う
            let mut map: std::collections::HashMap<String, String> = defaults
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            for (k, v) in &p.hotkeys {
                map.insert(k.clone(), v.clone());
            }
            defaults
                .iter()
                .map(|(k, _)| {
                    let v = map.remove(*k).unwrap_or_default();
                    (k.to_string(), RwSignal::new(v))
                })
                .collect()
        };
        let plugins_enabled: im::Vector<(String, RwSignal<bool>)> = p
            .plugins_enabled
            .iter()
            .map(|(k, v)| (k.clone(), RwSignal::new(*v)))
            .collect();
        Self {
            initial_path: RwSignal::new(p.initial_path.clone()),
            show_hidden: RwSignal::new(p.show_hidden),
            show_thumbnails: RwSignal::new(p.show_thumbnails),
            show_preview: RwSignal::new(p.show_preview),
            show_plugin_panel: RwSignal::new(p.show_plugin_panel),
            hide_pane_toolbar: RwSignal::new(p.hide_pane_toolbar),
            theme: RwSignal::new(p.theme.clone()),
            theme_preset: RwSignal::new(p.theme_preset.clone()),
            accent_color: RwSignal::new(p.accent_color.clone()),
            icon_set: RwSignal::new(p.icon_set.clone()),
            icon_pack: RwSignal::new(p.icon_pack.clone()),
            ui_font: RwSignal::new(p.ui_font.clone()),
            ui_font_size: RwSignal::new(p.ui_font_size.clone()),

            tab_columns: RwSignal::new(p.tab_columns.clone()),
            tabs_width: RwSignal::new(p.tabs_width.clone()),
            tree_width: RwSignal::new(p.tree_width.clone()),
            same_panel_stack: RwSignal::new(p.same_panel_stack),
            workspace_layout: RwSignal::new(p.workspace_layout.clone()),
            panel_dock_tabs: RwSignal::new(p.panel_dock_tabs.clone()),
            panel_dock_tree: RwSignal::new(p.panel_dock_tree.clone()),

            search_backend: RwSignal::new(p.search_backend.clone()),
            everything_port: RwSignal::new(p.everything_port.clone()),
            everything_scope: RwSignal::new(p.everything_scope),

            show_terminal: RwSignal::new(p.show_terminal),
            terminal_height: RwSignal::new(p.terminal_height.clone()),
            terminal_shell: RwSignal::new(p.terminal_shell.clone()),
            terminal_font: RwSignal::new(p.terminal_font.clone()),
            terminal_font_size: RwSignal::new(p.terminal_font_size.clone()),

            hotkeys: RwSignal::new(hotkeys),
            plugins_enabled: RwSignal::new(plugins_enabled),

            window_x: RwSignal::new(p.window_x),
            window_y: RwSignal::new(p.window_y),
            window_w: RwSignal::new(p.window_w),
            window_h: RwSignal::new(p.window_h),
            window_maximized: RwSignal::new(p.window_maximized),
            open_tabs: RwSignal::new(p.open_tabs.clone()),
            tab_layouts: RwSignal::new(p.tab_layouts.clone()),
            tab_locked: RwSignal::new(p.tab_locked.clone()),
            tree_unc_shares: RwSignal::new(p.tree_unc_shares.clone()),
        }
    }

    /// 全シグナルから値を読み出して ron ファイルへ保存。
    pub fn save(&self) -> Result<(), String> {
        let p = PersistedSettings::from_app(self);
        let path = settings_path().ok_or_else(|| String::from("config dir 未取得"))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let s = ron::ser::to_string_pretty(&p, ron::ser::PrettyConfig::default())
            .map_err(|e| e.to_string())?;
        std::fs::write(&path, s).map_err(|e| e.to_string())?;
        Ok(())
    }
}

pub(super) fn default_hotkeys() -> Vec<(&'static str, &'static str)> {
    vec![
        ("open", "Enter"),
        ("parent", "Backspace"),
        ("refresh", "F5"),
        ("rename", "F2"),
        ("delete", "Delete"),
        ("delete-permanent", "Shift+Delete"),
        ("new-folder", "Ctrl+Shift+N"),
        ("cut", "Ctrl+X"),
        ("copy", "Ctrl+C"),
        ("paste", "Ctrl+V"),
        ("select-all", "Ctrl+A"),
        ("search", "Ctrl+F"),
        ("open-settings", "Ctrl+,"),
        ("new-tab", "Ctrl+T"),
        ("close-tab", "Ctrl+W"),
        ("next-tab", "Ctrl+Tab"),
        ("prev-tab", "Ctrl+Shift+Tab"),
        ("toggle-tabs", "Ctrl+B"),
        ("toggle-tree", "Ctrl+Shift+E"),
        ("address-bar", "Ctrl+L"),
        ("undo", "Ctrl+Z"),
        ("pane-back", "Alt+Left"),
        ("pane-forward", "Alt+Right"),
    ]
}
