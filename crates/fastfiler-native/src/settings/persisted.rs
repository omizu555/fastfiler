//! 永続化用の `PersistedSettings` (serde mirror) と RON ファイル I/O。
//!
//! - 読み込み: `PersistedSettings::load_or_default()` (`%APPDATA%/FastFiler/settings.ron`)
//! - 書き込み: `AppSettings::save()` から `PersistedSettings::from_app(&app)` で生成 → RON 保存

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use floem::reactive::SignalGet;

use super::model::AppSettings;

#[derive(Serialize, Deserialize, Clone)]
pub struct PersistedSettings {
    #[serde(default = "def_initial_path")]
    pub initial_path: String,
    #[serde(default)]
    pub show_hidden: bool,
    #[serde(default = "def_true")]
    pub show_thumbnails: bool,
    #[serde(default)]
    pub show_preview: bool,
    #[serde(default)]
    pub show_plugin_panel: bool,
    #[serde(default)]
    pub hide_pane_toolbar: bool,
    #[serde(default = "def_system")]
    pub theme: String,
    #[serde(default = "def_default")]
    pub theme_preset: String,
    #[serde(default)]
    pub accent_color: String,
    #[serde(default = "def_emoji")]
    pub icon_set: String,
    #[serde(default = "def_default")]
    pub icon_pack: String,
    #[serde(default)]
    pub ui_font: String,
    #[serde(default = "def_13")]
    pub ui_font_size: String,

    #[serde(default = "def_1")]
    pub tab_columns: String,
    #[serde(default = "def_220")]
    pub tabs_width: String,
    #[serde(default = "def_240")]
    pub tree_width: String,
    #[serde(default)]
    pub same_panel_stack: bool,
    #[serde(default = "def_tabs_left")]
    pub workspace_layout: String,
    #[serde(default = "def_left")]
    pub panel_dock_tabs: String,
    #[serde(default = "def_left")]
    pub panel_dock_tree: String,

    #[serde(default = "def_builtin")]
    pub search_backend: String,
    #[serde(default = "def_80")]
    pub everything_port: String,
    #[serde(default = "def_true")]
    pub everything_scope: bool,

    #[serde(default)]
    pub show_terminal: bool,
    #[serde(default = "def_240")]
    pub terminal_height: String,
    #[serde(default)]
    pub terminal_shell: String,
    #[serde(default)]
    pub terminal_font: String,
    #[serde(default = "def_13")]
    pub terminal_font_size: String,

    #[serde(default)]
    pub hotkeys: Vec<(String, String)>,
    #[serde(default)]
    pub plugins_enabled: Vec<(String, bool)>,

    #[serde(default)]
    pub window_x: Option<i32>,
    #[serde(default)]
    pub window_y: Option<i32>,
    #[serde(default)]
    pub window_w: Option<u32>,
    #[serde(default)]
    pub window_h: Option<u32>,
    #[serde(default)]
    pub window_maximized: bool,

    #[serde(default)]
    pub open_tabs: Vec<String>,
    #[serde(default)]
    pub tab_layouts: Vec<String>,
    /// open_tabs / tab_layouts と同順のロック状態 (true = 閉じない)。
    /// 長さが合わなくても安全側 (false 扱い) で復元する。
    #[serde(default)]
    pub tab_locked: Vec<bool>,
}

pub(super) fn def_true() -> bool {
    true
}
pub(super) fn def_initial_path() -> String {
    String::from("C:\\")
}
pub(super) fn def_system() -> String {
    String::from("system")
}
pub(super) fn def_default() -> String {
    String::from("default")
}
pub(super) fn def_emoji() -> String {
    String::from("emoji")
}
pub(super) fn def_13() -> String {
    String::from("13")
}
pub(super) fn def_1() -> String {
    String::from("1")
}
pub(super) fn def_220() -> String {
    String::from("220")
}
pub(super) fn def_240() -> String {
    String::from("240")
}
pub(super) fn def_tabs_left() -> String {
    String::from("tabsLeft")
}
pub(super) fn def_left() -> String {
    String::from("left")
}
pub(super) fn def_builtin() -> String {
    String::from("builtin")
}
pub(super) fn def_80() -> String {
    String::from("80")
}

impl Default for PersistedSettings {
    fn default() -> Self {
        Self {
            initial_path: def_initial_path(),
            show_hidden: false,
            show_thumbnails: true,
            show_preview: false,
            show_plugin_panel: false,
            hide_pane_toolbar: false,
            theme: def_system(),
            theme_preset: def_default(),
            accent_color: String::new(),
            icon_set: def_emoji(),
            icon_pack: def_default(),
            ui_font: String::new(),
            ui_font_size: def_13(),

            tab_columns: def_1(),
            tabs_width: def_220(),
            tree_width: def_240(),
            same_panel_stack: false,
            workspace_layout: def_tabs_left(),
            panel_dock_tabs: def_left(),
            panel_dock_tree: def_left(),

            search_backend: def_builtin(),
            everything_port: def_80(),
            everything_scope: true,

            show_terminal: false,
            terminal_height: def_240(),
            terminal_shell: String::new(),
            terminal_font: String::new(),
            terminal_font_size: def_13(),

            hotkeys: Vec::new(),
            plugins_enabled: Vec::new(),

            window_x: None,
            window_y: None,
            window_w: None,
            window_h: None,
            window_maximized: false,
            open_tabs: Vec::new(),
            tab_layouts: Vec::new(),
            tab_locked: Vec::new(),
        }
    }
}

impl PersistedSettings {
    pub(super) fn from_app(a: &AppSettings) -> Self {
        Self {
            initial_path: a.initial_path.get(),
            show_hidden: a.show_hidden.get(),
            show_thumbnails: a.show_thumbnails.get(),
            show_preview: a.show_preview.get(),
            show_plugin_panel: a.show_plugin_panel.get(),
            hide_pane_toolbar: a.hide_pane_toolbar.get(),
            theme: a.theme.get(),
            theme_preset: a.theme_preset.get(),
            accent_color: a.accent_color.get(),
            icon_set: a.icon_set.get(),
            icon_pack: a.icon_pack.get(),
            ui_font: a.ui_font.get(),
            ui_font_size: a.ui_font_size.get(),
            tab_columns: a.tab_columns.get(),
            tabs_width: a.tabs_width.get(),
            tree_width: a.tree_width.get(),
            same_panel_stack: a.same_panel_stack.get(),
            workspace_layout: a.workspace_layout.get(),
            panel_dock_tabs: a.panel_dock_tabs.get(),
            panel_dock_tree: a.panel_dock_tree.get(),
            search_backend: a.search_backend.get(),
            everything_port: a.everything_port.get(),
            everything_scope: a.everything_scope.get(),
            show_terminal: a.show_terminal.get(),
            terminal_height: a.terminal_height.get(),
            terminal_shell: a.terminal_shell.get(),
            terminal_font: a.terminal_font.get(),
            terminal_font_size: a.terminal_font_size.get(),
            hotkeys: a
                .hotkeys
                .get()
                .iter()
                .map(|(k, v)| (k.clone(), v.get()))
                .collect(),
            plugins_enabled: a
                .plugins_enabled
                .get()
                .iter()
                .map(|(k, v)| (k.clone(), v.get()))
                .collect(),

            window_x: a.window_x.get(),
            window_y: a.window_y.get(),
            window_w: a.window_w.get(),
            window_h: a.window_h.get(),
            window_maximized: a.window_maximized.get(),
            open_tabs: a.open_tabs.get(),
            tab_layouts: a.tab_layouts.get(),
            tab_locked: a.tab_locked.get(),
        }
    }

    pub fn load_or_default() -> Self {
        Self::load().unwrap_or_default()
    }

    pub(super) fn load() -> Option<Self> {
        let path = settings_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        ron::from_str(&text).ok()
    }
}

pub(super) fn settings_path() -> Option<PathBuf> {
    let base = dirs::config_dir()?;
    Some(base.join("FastFiler").join("settings.ron"))
}
