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
    let font_row = row![
        pick_list(
            fonts.to_vec(),
            Some(current_font),
            SettingsMsg::SetFontFamily
        )
        .width(240),
        text(format!("サイズ {:.0}px", settings.font_size)).size(12),
        slider(10.0..=28.0, settings.font_size, SettingsMsg::SetFontSize)
            .step(1.0)
            .width(160),
        text("フォントは再起動後に反映").size(11),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);

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

    let renderer_labels = vec![
        "標準 (GPU)".to_string(),
        "省メモリ (ソフトウェア描画)".to_string(),
    ];
    let current_renderer = if settings.renderer.as_deref() == Some("lowmem") {
        "省メモリ (ソフトウェア描画)".to_string()
    } else {
        "標準 (GPU)".to_string()
    };
    let renderer_row = row![
        pick_list(
            renderer_labels,
            Some(current_renderer),
            SettingsMsg::SetRenderer
        )
        .width(240),
        text("省メモリはメモリ約 1/10・起動高速。大画面では描画が重くなる場合あり").size(11),
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
        text("iced_hotkeys.json (gpui_hotkeys.json から自動移行)").size(11),
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
