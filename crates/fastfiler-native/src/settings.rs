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
}

impl AppSettings {
    pub fn new() -> Self {
        let hotkeys: im::Vector<(String, RwSignal<String>)> = default_hotkeys()
            .into_iter()
            .map(|(k, v)| (k.to_string(), RwSignal::new(v.to_string())))
            .collect();
        Self {
            initial_path: RwSignal::new(String::from("C:\\")),
            show_hidden: RwSignal::new(false),
            show_thumbnails: RwSignal::new(true),
            show_preview: RwSignal::new(false),
            show_plugin_panel: RwSignal::new(false),
            hide_pane_toolbar: RwSignal::new(false),
            theme: RwSignal::new(String::from("system")),
            theme_preset: RwSignal::new(String::from("default")),
            accent_color: RwSignal::new(String::new()),
            icon_set: RwSignal::new(String::from("emoji")),
            icon_pack: RwSignal::new(String::from("default")),
            ui_font: RwSignal::new(String::new()),
            ui_font_size: RwSignal::new(String::from("13")),

            tab_columns: RwSignal::new(String::from("1")),
            tabs_width: RwSignal::new(String::from("220")),
            tree_width: RwSignal::new(String::from("240")),
            same_panel_stack: RwSignal::new(false),
            workspace_layout: RwSignal::new(String::from("tabsLeft")),
            panel_dock_tabs: RwSignal::new(String::from("left")),
            panel_dock_tree: RwSignal::new(String::from("left")),

            search_backend: RwSignal::new(String::from("builtin")),
            everything_port: RwSignal::new(String::from("80")),
            everything_scope: RwSignal::new(true),

            show_terminal: RwSignal::new(false),
            terminal_height: RwSignal::new(String::from("240")),
            terminal_shell: RwSignal::new(String::new()),
            terminal_font: RwSignal::new(String::new()),
            terminal_font_size: RwSignal::new(String::from("13")),

            hotkeys: RwSignal::new(hotkeys),
            plugins_enabled: RwSignal::new(im::Vector::new()),
        }
    }
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

    let close_btn = button("× Close").action(move || open.set(false));

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
