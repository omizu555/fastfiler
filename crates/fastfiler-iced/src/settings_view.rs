//! 設定画面 (⚙ — F-1101〜F-1105)。独立 view (GPUI 版 render_settings 476 行の反省)。
//!
//! 標準 widget (pick_list / slider / text_input / checkbox) で組む。
//! 変更は即保存 (settings::update)。テーマ・行高は即時反映、
//! フォント (ファミリー/サイズの描画フォント) は再起動後に反映。

use iced::widget::{
    button, checkbox, column, container, pick_list, row, scrollable, slider, text, text_input,
};
use iced::{Element, Length};

use crate::settings::AppSettings;

#[derive(Debug, Clone)]
pub enum SettingsMsg {
    /// 設定画面を開く (⚙ — app.rs 側から送られる)。
    Open,
    Close,
    SetTheme(String),
    SetStyle(String),
    ReloadThemes,
    SetFontSize(f32),
    SetFontFamily(String),
    SetFontFilter(String),
    SetPort(String),
    SetTabColumns(u8),
    SetRenderer(String),
    SetShowTreeButton(bool),
    ReloadHotkeys,
    OpenHotkeysFile,
    OpenTemplatesDir,
    OpenCommandsDir,
}

/// 設定画面の view。テーマ名一覧は毎回 theme::theme_names() から取る。
/// `port_input` はポート欄の編集バッファ、`fonts` はインストール済みフォント
/// ファミリー一覧 (App が設定画面を開くときに一度だけ列挙してキャッシュ)。
pub fn view<'a>(
    settings: &'a AppSettings,
    port_input: &'a str,
    fonts: &[String],
    font_filter: &'a str,
) -> Element<'a, SettingsMsg> {
    let themes = crate::theme::theme_names();
    let current_theme = settings
        .theme
        .clone()
        .unwrap_or_else(|| themes.first().cloned().unwrap_or_default());

    let section = |label: &str| text(label.to_string()).size(15);

    let styles: Vec<String> = crate::theme::STYLES
        .iter()
        .map(|s| s.name.to_string())
        .collect();
    let current_style = settings
        .style
        .clone()
        .unwrap_or_else(|| "モダン".to_string());
    let theme_row = row![
        pick_list(themes, Some(current_theme), SettingsMsg::SetTheme).width(220),
        pick_list(styles, Some(current_style), SettingsMsg::SetStyle).width(120),
        button(text("テーマを再読込").size(12))
            .padding([4, 10])
            .on_press(SettingsMsg::ReloadThemes),
        text("themes\\*.json (GPUI 版と同形式)").size(11),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);

    let current_font = settings
        .font_family
        .clone()
        .unwrap_or_else(|| "Yu Gothic UI".to_string());
    // フォント選択: コンボボックス (pick_list) はドロップダウンのスクロールが
    // 省メモリレンダラでクリップされず文字が重なるため、スクロール不要の
    // 「絞り込み入力 + 候補ボタン」方式にする (数百件から打って絞る方が速い)
    let filter_lower = font_filter.to_lowercase();
    let matches: Vec<&String> = fonts
        .iter()
        .filter(|f| filter_lower.is_empty() || f.to_lowercase().contains(&filter_lower))
        .collect();
    const FONT_CANDIDATES: usize = 8;
    let mut font_col = column![
        row![
            text(format!("現在: {current_font}")).size(13),
            text(format!("サイズ {:.0}px", settings.font_size)).size(12),
            slider(10.0..=28.0, settings.font_size, SettingsMsg::SetFontSize)
                .step(1.0)
                .width(160),
            text("フォントは再起動後に反映").size(11),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center),
        text_input("フォント名で絞り込み (例: yu, gothic, 明朝)…", font_filter)
            .on_input(SettingsMsg::SetFontFilter)
            .width(280)
            .size(13),
    ]
    .spacing(6);
    let mut candidates = row![].spacing(6);
    for f in matches.iter().take(FONT_CANDIDATES) {
        let name = (*f).clone();
        let selected = **f == current_font;
        candidates = candidates.push(
            button(text((*f).clone()).size(12))
                .padding([3, 8])
                .style(if selected {
                    button::primary
                } else {
                    button::secondary
                })
                .on_press(SettingsMsg::SetFontFamily(name)),
        );
    }
    font_col = font_col.push(candidates.wrap());
    if matches.len() > FONT_CANDIDATES {
        font_col = font_col.push(
            text(format!(
                "…ほか {} 件 — 絞り込むと候補が表示されます",
                matches.len() - FONT_CANDIDATES
            ))
            .size(11),
        );
    }
    let font_row = font_col;

    let port_row = row![
        text_input("80", port_input)
            .on_input(SettingsMsg::SetPort)
            .width(90),
        text("Everything (HTTP) のポート。未起動なら内蔵検索に自動フォールバック").size(11),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);

    let tabs_row = row![
        text("タブバー列数").size(12),
        pick_list(
            vec![1u8, 2, 3, 4],
            Some(settings.tab_columns.clamp(1, 4)),
            SettingsMsg::SetTabColumns
        )
        .width(70),
        checkbox(settings.show_tree_button)
            .label("「ツリー」ボタンを表示")
            .on_toggle(SettingsMsg::SetShowTreeButton)
            .size(16)
            .text_size(12),
    ]
    .spacing(14)
    .align_y(iced::Alignment::Center);

    let renderer_labels = vec!["省メモリ (既定)".to_string(), "GPU (wgpu/DX12)".to_string()];
    let current_renderer = if settings.renderer.as_deref() == Some("gpu") {
        "GPU (wgpu/DX12)".to_string()
    } else {
        "省メモリ (既定)".to_string()
    };
    let renderer_row = row![
        pick_list(
            renderer_labels,
            Some(current_renderer),
            SettingsMsg::SetRenderer
        )
        .width(240),
        text(
            "省メモリはメモリ約 1/10・起動高速。既知: まれに描画の残像 / 大画面で重くなる場合あり"
        )
        .size(11),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);

    let hotkeys_row = row![
        button(text("ホットキー設定を開く").size(12))
            .padding([4, 10])
            .on_press(SettingsMsg::OpenHotkeysFile),
        button(text("再読み込み").size(12))
            .padding([4, 10])
            .on_press(SettingsMsg::ReloadHotkeys),
        text("hotkeys.json (旧ファイルから自動移行)").size(11),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);

    let folders_row = row![
        button(text("テンプレートフォルダを開く").size(12))
            .padding([4, 10])
            .on_press(SettingsMsg::OpenTemplatesDir),
        button(text("ユーザーコマンドのフォルダを開く").size(12))
            .padding([4, 10])
            .on_press(SettingsMsg::OpenCommandsDir),
    ]
    .spacing(10);

    let body = column![
        row![
            text("設定").size(20).width(Length::Fill),
            button(text("× 閉じる").size(13))
                .padding([4, 12])
                .on_press(SettingsMsg::Close),
        ]
        .align_y(iced::Alignment::Center),
        section("テーマ"),
        theme_row,
        section("フォント (一覧の行高が追従)"),
        font_row,
        section("検索"),
        port_row,
        section("タブバー"),
        tabs_row,
        section("レンダラ (再起動後に反映)"),
        renderer_row,
        section("ホットキー"),
        hotkeys_row,
        section("フォルダ"),
        folders_row,
    ]
    .spacing(12)
    .padding(24)
    .max_width(720);

    container(scrollable(body))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
