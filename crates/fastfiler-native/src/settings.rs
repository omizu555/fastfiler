// 設定ダイアログ。
//
// 現状は UI のみで、設定値は AppSettings 構造体に保持するだけ。
// 実際の動作 (保存 / 各機能への反映) は後続フェーズで配線する。
//
// 設定値は元 Tauri 版 (src/store/core.ts の AppState) を踏襲。

use floem::peniko::Color;
use floem::prelude::*;
use floem::style::CursorStyle;
use floem::views::{
    button, container, dyn_container, h_stack, label, scroll, text_input, v_stack, Decorators,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ────────────────────────────────────────────────────────────────
// AppSettings (元 Tauri 版の AppState を踏襲)
// ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppSettings {
    // General
    pub initial_path: RwSignal<String>,
    pub show_hidden: RwSignal<bool>,
    pub show_thumbnails: RwSignal<bool>,
    pub show_preview: RwSignal<bool>,
    pub show_plugin_panel: RwSignal<bool>,
    pub hide_pane_toolbar: RwSignal<bool>,
    pub theme: RwSignal<String>,         // "system" | "dark" | "light"
    pub theme_preset: RwSignal<String>,  // "default" | "dracula" | ...
    pub accent_color: RwSignal<String>,  // "#rrggbb" or ""
    pub icon_set: RwSignal<String>,      // "emoji" | "minimal" | "colored"
    pub icon_pack: RwSignal<String>,     // "default" | "emoji" | ...
    pub ui_font: RwSignal<String>,
    pub ui_font_size: RwSignal<String>,  // 文字列で保持 (text_input 用)

    // Workspace
    pub tab_columns: RwSignal<String>,   // "1".."4"
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
                    let v = map
                        .remove(*k)
                        .unwrap_or_default();
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

// ────────────────────────────────────────────────────────────────
// 永続化用 (serde mirror struct)
// ────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
pub struct PersistedSettings {
    #[serde(default = "def_initial_path")] pub initial_path: String,
    #[serde(default)] pub show_hidden: bool,
    #[serde(default = "def_true")] pub show_thumbnails: bool,
    #[serde(default)] pub show_preview: bool,
    #[serde(default)] pub show_plugin_panel: bool,
    #[serde(default)] pub hide_pane_toolbar: bool,
    #[serde(default = "def_system")] pub theme: String,
    #[serde(default = "def_default")] pub theme_preset: String,
    #[serde(default)] pub accent_color: String,
    #[serde(default = "def_emoji")] pub icon_set: String,
    #[serde(default = "def_default")] pub icon_pack: String,
    #[serde(default)] pub ui_font: String,
    #[serde(default = "def_13")] pub ui_font_size: String,

    #[serde(default = "def_1")] pub tab_columns: String,
    #[serde(default = "def_220")] pub tabs_width: String,
    #[serde(default = "def_240")] pub tree_width: String,
    #[serde(default)] pub same_panel_stack: bool,
    #[serde(default = "def_tabs_left")] pub workspace_layout: String,
    #[serde(default = "def_left")] pub panel_dock_tabs: String,
    #[serde(default = "def_left")] pub panel_dock_tree: String,

    #[serde(default = "def_builtin")] pub search_backend: String,
    #[serde(default = "def_80")] pub everything_port: String,
    #[serde(default = "def_true")] pub everything_scope: bool,

    #[serde(default)] pub show_terminal: bool,
    #[serde(default = "def_240")] pub terminal_height: String,
    #[serde(default)] pub terminal_shell: String,
    #[serde(default)] pub terminal_font: String,
    #[serde(default = "def_13")] pub terminal_font_size: String,

    #[serde(default)] pub hotkeys: Vec<(String, String)>,
    #[serde(default)] pub plugins_enabled: Vec<(String, bool)>,

    #[serde(default)] pub window_x: Option<i32>,
    #[serde(default)] pub window_y: Option<i32>,
    #[serde(default)] pub window_w: Option<u32>,
    #[serde(default)] pub window_h: Option<u32>,
    #[serde(default)] pub window_maximized: bool,
}

fn def_true() -> bool { true }
fn def_initial_path() -> String { String::from("C:\\") }
fn def_system() -> String { String::from("system") }
fn def_default() -> String { String::from("default") }
fn def_emoji() -> String { String::from("emoji") }
fn def_13() -> String { String::from("13") }
fn def_1() -> String { String::from("1") }
fn def_220() -> String { String::from("220") }
fn def_240() -> String { String::from("240") }
fn def_tabs_left() -> String { String::from("tabsLeft") }
fn def_left() -> String { String::from("left") }
fn def_builtin() -> String { String::from("builtin") }
fn def_80() -> String { String::from("80") }

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
        }
    }
}

impl PersistedSettings {
    fn from_app(a: &AppSettings) -> Self {
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
        }
    }

    pub fn load_or_default() -> Self {
        Self::load().unwrap_or_default()
    }

    fn load() -> Option<Self> {
        let path = settings_path()?;
        let text = std::fs::read_to_string(&path).ok()?;
        ron::from_str(&text).ok()
    }
}

fn settings_path() -> Option<PathBuf> {
    let base = dirs::config_dir()?;
    Some(base.join("FastFiler").join("settings.ron"))
}

fn default_hotkeys() -> Vec<(&'static str, &'static str)> {
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
        ("toggle-preview", "Ctrl+P"),
        ("toggle-plugin", "Ctrl+Shift+P"),
        ("open-settings", "Ctrl+,"),
        ("new-tab", "Ctrl+T"),
        ("close-tab", "Ctrl+W"),
        ("next-tab", "Ctrl+Tab"),
        ("prev-tab", "Ctrl+Shift+Tab"),
        ("toggle-tabs", "Ctrl+B"),
        ("toggle-tree", "Ctrl+Shift+E"),
        ("address-bar", "Ctrl+L"),
        ("undo", "Ctrl+Z"),
        ("toggle-terminal", "Ctrl+`"),
        ("pane-back", "Alt+Left"),
        ("pane-forward", "Alt+Right"),
    ]
}

// ────────────────────────────────────────────────────────────────
// View helpers
// ────────────────────────────────────────────────────────────────

fn section_label(text_str: &'static str) -> impl IntoView {
    label(move || text_str.to_string()).style(|s| {
        s.padding(8)
            .font_bold()
            .color(Color::rgb8(220, 220, 220))
            .border_bottom(1)
            .border_color(Color::rgb8(60, 60, 60))
    })
}

fn row_input(title: &'static str, sig: RwSignal<String>) -> impl IntoView {
    h_stack((
        label(move || title.to_string())
            .style(|s| s.width(220).padding(6).color(Color::rgb8(200, 200, 200))),
        text_input(sig).style(|s| {
            s.flex_grow(1.0)
                .padding(4)
                .border(1)
                .border_color(Color::rgb8(120, 120, 120))
        }),
    ))
    .style(|s| s.padding(4).items_center().gap(8))
}

fn row_check(title: &'static str, sig: RwSignal<bool>) -> impl IntoView {
    h_stack((
        label(move || {
            let mark = if sig.get() { "[v]" } else { "[ ]" };
            format!("{} {}", mark, title)
        })
        .style(|s| s.padding(6).cursor(CursorStyle::Pointer).color(Color::rgb8(220, 220, 220)))
        .on_click_stop(move |_| sig.set(!sig.get())),
    ))
    .style(|s| s.padding(4))
}

fn row_select(
    title: &'static str,
    sig: RwSignal<String>,
    options: Vec<&'static str>,
) -> impl IntoView {
    let buttons: Vec<floem::AnyView> = options
        .into_iter()
        .map(|opt| {
            let s = sig;
            label(move || opt.to_string())
                .style(move |st| {
                    let active = s.get() == opt;
                    let bg = if active { Color::rgb8(58, 96, 158) } else { Color::rgb8(40, 40, 44) };
                    st.padding_horiz(10)
                        .padding_vert(4)
                        .background(bg)
                        .border(1)
                        .border_color(Color::rgb8(60, 60, 60))
                        .cursor(CursorStyle::Pointer)
                        .color(Color::rgb8(220, 220, 220))
                })
                .on_click_stop(move |_| s.set(opt.to_string()))
                .into_any()
        })
        .collect();

    h_stack((
        label(move || title.to_string())
            .style(|s| s.width(220).padding(6).color(Color::rgb8(200, 200, 200))),
        floem::views::stack_from_iter(buttons).style(|s| s.flex_row().gap(2)),
    ))
    .style(|s| s.padding(4).items_center().gap(8))
}

// ────────────────────────────────────────────────────────────────
// Tabs
// ────────────────────────────────────────────────────────────────

fn tab_general(s: &AppSettings) -> floem::AnyView {
    let body = v_stack((
        section_label("General"),
        row_input("起動パス (initialPath)", s.initial_path),
        row_check("隠しファイルを表示 (showHidden)", s.show_hidden),
        row_check("サムネイル表示 (showThumbnails)", s.show_thumbnails),
        row_check("プレビュー表示 (showPreview)", s.show_preview),
        row_check("プラグインパネル表示 (showPluginPanel)", s.show_plugin_panel),
        row_check("ペインツールバーを隠す (hidePaneToolbar)", s.hide_pane_toolbar),
        section_label("Theme"),
        row_select("テーマ (theme)", s.theme, vec!["system", "dark", "light"]),
        row_select(
            "プリセット (themePreset)",
            s.theme_preset,
            vec!["default", "dracula", "solarizedDark", "solarizedLight", "nord", "monokai"],
        ),
        row_input("アクセントカラー (#rrggbb)", s.accent_color),
        row_select("アイコンセット (iconSet)", s.icon_set, vec!["emoji", "minimal", "colored"]),
        row_input("アイコンパック (iconPack)", s.icon_pack),
        section_label("Font"),
        row_input("UI フォント (uiFont)", s.ui_font),
        row_input("UI フォントサイズ (uiFontSize)", s.ui_font_size),
    ))
    .style(|s| s.flex_col());
    container(body).style(|s| s.padding(8)).into_any()
}

fn tab_workspace(s: &AppSettings) -> floem::AnyView {
    let body = v_stack((
        section_label("Workspace"),
        row_input("タブ列数 (tabColumns)", s.tab_columns),
        row_input("タブ幅 px (tabsWidth)", s.tabs_width),
        row_input("ツリー幅 px (treeWidth)", s.tree_width),
        row_check("同パネル積み重ね (samePanelStack)", s.same_panel_stack),
        row_select(
            "レイアウト (workspace.layout)",
            s.workspace_layout,
            vec!["tabsLeft", "tabsRight", "tabsHidden"],
        ),
        row_select(
            "タブパネル位置 (panelDock.tabs)",
            s.panel_dock_tabs,
            vec!["left", "right", "top", "bottom", "float", "hidden"],
        ),
        row_select(
            "ツリーパネル位置 (panelDock.tree)",
            s.panel_dock_tree,
            vec!["left", "right", "top", "bottom", "float", "hidden"],
        ),
    ))
    .style(|s| s.flex_col());
    container(body).style(|s| s.padding(8)).into_any()
}

fn tab_search(s: &AppSettings) -> floem::AnyView {
    let body = v_stack((
        section_label("Search"),
        row_select("バックエンド (searchBackend)", s.search_backend, vec!["builtin", "everything"]),
        row_input("Everything ポート (everythingPort)", s.everything_port),
        row_check("Everything スコープ (everythingScope)", s.everything_scope),
    ))
    .style(|s| s.flex_col());
    container(body).style(|s| s.padding(8)).into_any()
}

fn tab_terminal(s: &AppSettings) -> floem::AnyView {
    let body = v_stack((
        section_label("Terminal"),
        row_check("ターミナル表示 (showTerminal)", s.show_terminal),
        row_input("ターミナル高さ px (terminalHeight)", s.terminal_height),
        row_input("シェル (terminalShell)", s.terminal_shell),
        row_input("フォント (terminalFont)", s.terminal_font),
        row_input("フォントサイズ (terminalFontSize)", s.terminal_font_size),
    ))
    .style(|s| s.flex_col());
    container(body).style(|s| s.padding(8)).into_any()
}

fn tab_hotkeys(s: &AppSettings) -> floem::AnyView {
    let hotkeys = s.hotkeys.get();
    let mut rows: Vec<floem::AnyView> = Vec::new();
    rows.push(section_label("Hotkeys").into_any());
    for (action, sig) in hotkeys.iter() {
        let action_text = action.clone();
        let sig = *sig;
        let row = h_stack((
            label(move || action_text.clone())
                .style(|s| s.width(220).padding(6).color(Color::rgb8(200, 200, 200))),
            text_input(sig).style(|s| {
                s.flex_grow(1.0)
                    .padding(4)
                    .border(1)
                    .border_color(Color::rgb8(120, 120, 120))
            }),
        ))
        .style(|s| s.padding(4).items_center().gap(8));
        rows.push(row.into_any());
    }
    container(floem::views::stack_from_iter(rows).style(|s| s.flex_col()))
        .style(|s| s.padding(8))
        .into_any()
}

fn tab_plugins(s: &AppSettings) -> floem::AnyView {
    let plugins = s.plugins_enabled.get();
    let body: floem::AnyView = if plugins.is_empty() {
        v_stack((
            section_label("Plugins"),
            label(|| String::from("(プラグインは検出されていません)"))
                .style(|s| s.padding(12).color(Color::rgb8(180, 180, 180))),
        ))
        .style(|s| s.flex_col())
        .into_any()
    } else {
        let mut rows: Vec<floem::AnyView> = Vec::new();
        rows.push(section_label("Plugins").into_any());
        for (id, sig) in plugins.iter() {
            let id_text = id.clone();
            let sig = *sig;
            rows.push(
                label(move || {
                    let mark = if sig.get() { "[v]" } else { "[ ]" };
                    format!("{} {}", mark, id_text)
                })
                .style(|s| s.padding(6).cursor(CursorStyle::Pointer).color(Color::rgb8(220, 220, 220)))
                .on_click_stop(move |_| sig.set(!sig.get()))
                .into_any(),
            );
        }
        floem::views::stack_from_iter(rows).style(|s| s.flex_col()).into_any()
    };
    container(body).style(|s| s.padding(8)).into_any()
}

// ────────────────────────────────────────────────────────────────
// Settings dialog (full-screen pane)
// ────────────────────────────────────────────────────────────────

pub fn settings_view(
    settings: AppSettings,
    open: RwSignal<bool>,
) -> impl IntoView {
    let active_tab: RwSignal<&'static str> = RwSignal::new("general");

    let make_tab = move |id: &'static str, title: &'static str| {
        let active_tab = active_tab;
        label(move || title.to_string())
            .style(move |s| {
                let on = active_tab.get() == id;
                let bg = if on { Color::rgb8(58, 96, 158) } else { Color::rgb8(34, 34, 38) };
                s.height(32)
                    .width_full()
                    .items_center()
                    .padding_horiz(12)
                    .background(bg)
                    .border_bottom(1)
                    .border_color(Color::rgb8(60, 60, 60))
                    .cursor(CursorStyle::Pointer)
                    .color(Color::rgb8(220, 220, 220))
            })
            .on_click_stop(move |_| active_tab.set(id))
    };

    let tabs_col = v_stack((
        label(|| String::from("Settings"))
            .style(|s| s.padding(12).font_bold().color(Color::rgb8(220, 220, 220)).font_size(15.0)),
        make_tab("general", "General"),
        make_tab("workspace", "Workspace"),
        make_tab("search", "Search"),
        make_tab("terminal", "Terminal"),
        make_tab("hotkeys", "Hotkeys"),
        make_tab("plugins", "Plugins"),
    ))
    .style(|s| {
        s.width(180)
            .height_full()
            .background(Color::rgb8(28, 28, 32))
            .border_right(1)
            .border_color(Color::rgb8(60, 60, 60))
            .flex_col()
    });

    let settings_for_body = settings.clone();
    let body = dyn_container(
        move || active_tab.get(),
        move |which| match which {
            "general" => tab_general(&settings_for_body),
            "workspace" => tab_workspace(&settings_for_body),
            "search" => tab_search(&settings_for_body),
            "terminal" => tab_terminal(&settings_for_body),
            "hotkeys" => tab_hotkeys(&settings_for_body),
            "plugins" => tab_plugins(&settings_for_body),
            _ => label(|| String::new()).into_any(),
        },
    )
    .style(|s| s.size_full().flex_grow(1.0));

    let settings_for_save = settings.clone();
    let close_btn = button("× Close").action(move || {
        if let Err(e) = settings_for_save.save() {
            eprintln!("[settings] save error: {}", e);
        }
        open.set(false);
    });

    let header = h_stack((
        label(|| String::from("⚙ Settings"))
            .style(|s| s.padding(8).font_bold().font_size(15.0).color(Color::rgb8(220, 220, 220)).flex_grow(1.0)),
        close_btn,
    ))
    .style(|s| {
        s.height(40)
            .items_center()
            .padding_horiz(8)
            .background(Color::rgb8(34, 34, 38))
            .border_bottom(1)
            .border_color(Color::rgb8(60, 60, 60))
    });

    let content = h_stack((tabs_col, scroll(body).style(|s| s.size_full().flex_grow(1.0))))
        .style(|s| s.size_full().flex_grow(1.0));

    v_stack((header, content)).style(|s| {
        s.size_full()
            .flex_col()
            .background(Color::rgb8(24, 24, 28))
            .color(Color::rgb8(220, 220, 220))
    })
}
