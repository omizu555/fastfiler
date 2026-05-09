//! 設定ダイアログ本体 (`settings_view`) と、各タブ (general/workspace/search/terminal/hotkeys/plugins) のビュー。

use floem::prelude::*;
use floem::style::CursorStyle;
use floem::views::{button, container, dyn_container, h_stack, label, scroll, text_input, v_stack, Decorators};

use crate::theme;

use super::model::AppSettings;
use super::widgets::{row_check, row_font, row_input, row_select, section_label};

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
        row_font("UI フォント (uiFont)", s.ui_font),
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
                .style(|s| s.width(220).padding(6).color(theme::text_label())),
            text_input(sig).style(|s| {
                s.flex_grow(1.0)
                    .padding(4)
                    .border(1)
                    .border_color(theme::border_strong())
                    .background(theme::bg_modal())
                    .color(theme::text_normal())
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
                .style(|s| s.padding(12).color(theme::text_dim())),
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
                .style(|s| s.padding(6).cursor(CursorStyle::Pointer).color(theme::text_normal()))
                .on_click_stop(move |_| sig.set(!sig.get()))
                .into_any(),
            );
        }
        floem::views::stack_from_iter(rows).style(|s| s.flex_col()).into_any()
    };
    container(body).style(|s| s.padding(8)).into_any()
}

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
                let bg = if on { theme::accent_select() } else { theme::bg_chrome() };
                s.height(32)
                    .width_full()
                    .items_center()
                    .padding_horiz(12)
                    .background(bg)
                    .border_bottom(1)
                    .border_color(theme::border_default())
                    .cursor(CursorStyle::Pointer)
                    .color(theme::text_normal())
            })
            .on_click_stop(move |_| active_tab.set(id))
    };

    let tabs_col = v_stack((
        label(|| String::from("Settings"))
            .style(|s| s.padding(12).font_bold().color(theme::text_normal()).font_size(15.0)),
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
            .background(theme::bg_panel())
            .border_right(1)
            .border_color(theme::border_default())
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
            .style(|s| s.padding(8).font_bold().font_size(15.0).color(theme::text_normal()).flex_grow(1.0)),
        close_btn,
    ))
    .style(|s| {
        s.height(40)
            .items_center()
            .padding_horiz(8)
            .background(theme::bg_chrome())
            .border_bottom(1)
            .border_color(theme::border_default())
    });

    let content = h_stack((tabs_col, scroll(body).style(|s| s.size_full().flex_grow(1.0))))
        .style(|s| s.size_full().flex_grow(1.0));

    v_stack((header, content)).style(|s| {
        s.size_full()
            .flex_col()
            .background(theme::bg_root())
            .color(theme::text_normal())
    })
}
