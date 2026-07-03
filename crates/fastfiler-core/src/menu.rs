//! 右クリックメニューの木構築 (F-904/USAGE.md §2。計画書 §10-9: 二重実装の一本化)。
//!
//! 項目 (USAGE.md §2 と同一):
//! - 行: 開く / コピー / 切り取り / 貼り付け / 名前の変更 / 削除 /
//!   新しいフォルダ / 新しいファイル ▸ / ユーザーコマンド
//! - 背景: 貼り付け / 最新の情報に更新 / 新しいフォルダ / 新しいファイル ▸ /
//!   ユーザーコマンド
//! - サブメニュー ▸ はクリック開閉 (hover では開かない)。常に 1 か所のみ表示。

/// メニュー構築に使う軽量なコマンド情報 (domain UserCommand の抜粋 —
/// core は domain に依存しないため GUI 層が変換して渡す)。
#[derive(Debug, Clone, PartialEq)]
pub struct CommandInfo {
    pub id: String,
    pub label: String,
    /// "file" | "folder" | "selection" | "background" | "drop" | "any"
    pub when: String,
    /// 空 = 全拡張子。指定時はカーソル行の拡張子が一致するときだけ表示。
    pub extensions: Vec<String>,
}

/// テンプレート情報の抜粋。
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateInfo {
    pub name: String,
    pub path: String,
}

/// メニュー項目の実行内容。
#[derive(Debug, Clone, PartialEq)]
pub enum MenuAction {
    Open,
    Copy,
    Cut,
    Paste,
    Rename,
    Delete,
    Refresh,
    NewFolder,
    NewFileEmpty,
    NewFileTemplate(String),
    /// テンプレートフォルダをフォーカスペインで開く。
    OpenTemplatesDir,
    UserCommand(String),
    /// サブメニュー (クリックで開閉)。
    Submenu,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    pub label: String,
    pub action: MenuAction,
    pub enabled: bool,
    pub children: Vec<MenuItem>,
}

impl MenuItem {
    fn leaf(label: &str, action: MenuAction) -> Self {
        Self {
            label: label.to_string(),
            action,
            enabled: true,
            children: Vec::new(),
        }
    }

    fn disabled(mut self, off: bool) -> Self {
        self.enabled = !off;
        self
    }
}

/// メニューの文脈。
pub struct MenuContext<'a> {
    /// 行の上か背景か。Some = 行 (カーソル行の is_dir と拡張子)。
    pub row: Option<(bool, Option<&'a str>)>,
    /// 貼り付け可能か (クリップボードにファイルがあるか)。
    pub can_paste: bool,
    pub templates: &'a [TemplateInfo],
    pub commands: &'a [CommandInfo],
}

/// メニュー木を構築する (F-904)。
pub fn build_menu(ctx: &MenuContext) -> Vec<MenuItem> {
    let mut items = Vec::new();
    let new_file = new_file_submenu(ctx.templates);
    match ctx.row {
        Some((is_dir, ext)) => {
            items.push(MenuItem::leaf("開く", MenuAction::Open));
            items.push(MenuItem::leaf("コピー", MenuAction::Copy));
            items.push(MenuItem::leaf("切り取り", MenuAction::Cut));
            items.push(MenuItem::leaf("貼り付け", MenuAction::Paste).disabled(!ctx.can_paste));
            items.push(MenuItem::leaf("名前の変更", MenuAction::Rename));
            items.push(MenuItem::leaf("削除", MenuAction::Delete));
            items.push(MenuItem::leaf("新しいフォルダ", MenuAction::NewFolder));
            items.push(new_file);
            items.extend(user_command_items(ctx.commands, Some((is_dir, ext))));
        }
        None => {
            items.push(MenuItem::leaf("貼り付け", MenuAction::Paste).disabled(!ctx.can_paste));
            items.push(MenuItem::leaf("最新の情報に更新", MenuAction::Refresh));
            items.push(MenuItem::leaf("新しいフォルダ", MenuAction::NewFolder));
            items.push(new_file);
            items.extend(user_command_items(ctx.commands, None));
        }
    }
    items
}

fn new_file_submenu(templates: &[TemplateInfo]) -> MenuItem {
    let mut children: Vec<MenuItem> = templates
        .iter()
        .map(|t| MenuItem::leaf(&t.name, MenuAction::NewFileTemplate(t.path.clone())))
        .collect();
    children.push(MenuItem::leaf(
        "テンプレートフォルダを開く",
        MenuAction::OpenTemplatesDir,
    ));
    MenuItem {
        label: "新しいファイル".into(),
        action: MenuAction::Submenu,
        enabled: true,
        children,
    }
}

/// ユーザーコマンドの表示フィルタ (COMMANDS.md の when 6 種)。
/// row = Some((is_dir, ext)) は行の上、None は背景。
fn user_command_items(
    commands: &[CommandInfo],
    row: Option<(bool, Option<&str>)>,
) -> Vec<MenuItem> {
    commands
        .iter()
        .filter(|c| match (c.when.as_str(), row) {
            ("any", _) => true,
            ("file", Some((false, ext))) => {
                c.extensions.is_empty()
                    || ext.is_some_and(|e| c.extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
            }
            ("folder", Some((true, _))) => true,
            ("selection", Some(_)) => true,
            ("background", None) => true,
            // "drop" は右ボタン D&D メニュー専用 (5c で使用)
            _ => false,
        })
        .map(|c| MenuItem::leaf(&c.label, MenuAction::UserCommand(c.id.clone())))
        .collect()
}

/// 右ボタン D&D のドロップメニュー (F-605。5c で使用)。
pub fn build_drop_menu(commands: &[CommandInfo]) -> Vec<MenuItem> {
    let mut items = vec![
        MenuItem::leaf("ここにコピー", MenuAction::Copy),
        MenuItem::leaf("ここに移動", MenuAction::Cut),
    ];
    items.extend(
        commands
            .iter()
            .filter(|c| c.when == "drop")
            .map(|c| MenuItem::leaf(&c.label, MenuAction::UserCommand(c.id.clone()))),
    );
    items.push(MenuItem::leaf("キャンセル", MenuAction::Submenu));
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(id: &str, when: &str, exts: &[&str]) -> CommandInfo {
        CommandInfo {
            id: id.into(),
            label: id.into(),
            when: when.into(),
            extensions: exts.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn row_and_background_menus_match_usage_spec() {
        let ctx = MenuContext {
            row: Some((false, Some("txt"))),
            can_paste: false,
            templates: &[],
            commands: &[],
        };
        let items = build_menu(&ctx);
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "開く",
                "コピー",
                "切り取り",
                "貼り付け",
                "名前の変更",
                "削除",
                "新しいフォルダ",
                "新しいファイル"
            ]
        );
        assert!(!items[3].enabled); // 貼り付け淡色 (can_paste=false)
        let bg = build_menu(&MenuContext {
            row: None,
            can_paste: true,
            templates: &[],
            commands: &[],
        });
        let labels: Vec<_> = bg.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "貼り付け",
                "最新の情報に更新",
                "新しいフォルダ",
                "新しいファイル"
            ]
        );
        assert!(bg[0].enabled);
    }

    #[test]
    fn when_filter_follows_commands_md() {
        let cmds = vec![
            cmd("any", "any", &[]),
            cmd("file", "file", &[]),
            cmd("txt-only", "file", &["txt"]),
            cmd("folder", "folder", &[]),
            cmd("sel", "selection", &[]),
            cmd("bg", "background", &[]),
            cmd("drop", "drop", &[]),
        ];
        let on_txt = user_command_items(&cmds, Some((false, Some("txt"))));
        let ids: Vec<_> = on_txt.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(ids, ["any", "file", "txt-only", "sel"]);
        let on_exe = user_command_items(&cmds, Some((false, Some("exe"))));
        let ids: Vec<_> = on_exe.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(ids, ["any", "file", "sel"]); // txt-only は拡張子不一致
        let on_dir = user_command_items(&cmds, Some((true, None)));
        let ids: Vec<_> = on_dir.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(ids, ["any", "folder", "sel"]);
        let on_bg = user_command_items(&cmds, None);
        let ids: Vec<_> = on_bg.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(ids, ["any", "bg"]);
        // drop はドロップメニューのみ
        let dm = build_drop_menu(&cmds);
        let ids: Vec<_> = dm.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(ids, ["ここにコピー", "ここに移動", "drop", "キャンセル"]);
    }

    #[test]
    fn template_submenu_lists_templates_plus_open_dir() {
        let templates = vec![TemplateInfo {
            name: "メモ.md".into(),
            path: "C:\\t\\メモ.md".into(),
        }];
        let ctx = MenuContext {
            row: None,
            can_paste: false,
            templates: &templates,
            commands: &[],
        };
        let items = build_menu(&ctx);
        let sub = items.iter().find(|i| i.label == "新しいファイル").unwrap();
        assert_eq!(sub.children.len(), 2);
        assert_eq!(sub.children[0].label, "メモ.md");
        assert!(matches!(
            sub.children[0].action,
            MenuAction::NewFileTemplate(_)
        ));
        assert_eq!(sub.children[1].label, "テンプレートフォルダを開く");
    }
}
