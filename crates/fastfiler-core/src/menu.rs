//! 右クリックメニューの木構築 (F-904/USAGE.md §2。計画書 §10-9: 二重実装の一本化)。
//!
//! 項目 (USAGE.md §2 と同一):
//! - 行: 開く / コピー / 切り取り / 貼り付け / 名前の変更 / 削除 /
//!   新しいフォルダ / 新しいファイル ▸ / ユーザーコマンド / プロパティ
//! - 背景: 貼り付け / 最新の情報に更新 / 新しいフォルダ / 新しいファイル ▸ /
//!   ユーザーコマンド / プロパティ (現在フォルダ)
//! - プロパティは最下部固定 (ADR 0007 追記 2026-06-07 — GPUI 版パリティ)。
//! - サブメニュー ▸ はクリック開閉 (hover では開かない)。常に 1 か所のみ表示。

/// メニュー構築に使う軽量なコマンド情報 (domain UserCommand の抜粋 —
/// core は domain に依存しないため GUI 層が変換して渡す)。
#[derive(Debug, Clone, PartialEq)]
pub struct CommandInfo {
    pub id: String,
    pub label: String,
    /// "file" | "folder" ("dir" 別名) | "selection" | "background" | "drop" | "any"
    pub when: String,
    /// 空 = 全拡張子。指定時はカーソル行の拡張子が一致するときだけ表示。
    pub extensions: Vec<String>,
    /// サブメニューに畳む。"/" 区切りで最大 3 階層 (COMMANDS.md §サブメニュー)。
    pub submenu: Option<String>,
    /// true でメニューに出さない (一時的な無効化)。
    pub hidden: bool,
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
    /// Windows のプロパティダイアログ (行 = その項目 / 背景 = 現在フォルダ)。
    Properties,
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
    // 最下部固定 (エクスプローラ準拠 — ADR 0007 追記)
    items.push(MenuItem::leaf("プロパティ", MenuAction::Properties));
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

/// メニュー階層の上限 (COMMANDS.md: "/" 区切りで最大 3 階層)。
const MENU_MAX_DEPTH: usize = 3;
/// 各メニューの項目数上限 (COMMANDS.md: 先頭から最大 50 件)。
const MENU_MAX_ITEMS: usize = 50;

/// ユーザーコマンドの表示フィルタ (COMMANDS.md の when 6 種) +
/// submenu によるグルーピング (同じ submenu が 1 つの ▸ にまとまる)。
fn user_command_items(
    commands: &[CommandInfo],
    row: Option<(bool, Option<&str>)>,
) -> Vec<MenuItem> {
    let mut root: Vec<MenuItem> = Vec::new();
    for c in commands.iter().filter(|c| {
        !c.hidden
            && match (c.when.as_str(), row) {
                ("any", _) => true,
                ("file", Some((false, ext))) => {
                    c.extensions.is_empty()
                        || ext
                            .is_some_and(|e| c.extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
                }
                ("folder" | "dir", Some((true, _))) => true,
                ("selection", Some(_)) => true,
                ("background", None) => true,
                // "drop" は右ボタン D&D メニュー専用
                _ => false,
            }
    }) {
        let path = parse_submenu(c.submenu.as_deref());
        insert_grouped(
            &mut root,
            &path,
            MenuItem::leaf(&c.label, MenuAction::UserCommand(c.id.clone())),
        );
    }
    root
}

/// `submenu` の "/" 区切りを階層パスへ (GPUI 版 parse_group と同一規則)。
fn parse_submenu(submenu: Option<&str>) -> Vec<String> {
    submenu
        .map(|s| {
            s.split('/')
                .map(|x| x.trim())
                .filter(|x| !x.is_empty())
                .take(MENU_MAX_DEPTH)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// 階層パスに沿ってグループ (▸) を作りながら項目を挿入する。
/// 記述順を保ち、各メニュー 50 件で打ち切る (COMMANDS.md)。
fn insert_grouped(items: &mut Vec<MenuItem>, path: &[String], leaf: MenuItem) {
    let Some((head, rest)) = path.split_first() else {
        if items.len() < MENU_MAX_ITEMS {
            items.push(leaf);
        }
        return;
    };
    if let Some(group) = items
        .iter_mut()
        .find(|i| i.action == MenuAction::Submenu && i.label == *head)
    {
        insert_grouped(&mut group.children, rest, leaf);
        return;
    }
    if items.len() >= MENU_MAX_ITEMS {
        return;
    }
    let mut group = MenuItem {
        label: head.clone(),
        action: MenuAction::Submenu,
        enabled: true,
        children: Vec::new(),
    };
    insert_grouped(&mut group.children, rest, leaf);
    items.push(group);
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
            submenu: None,
            hidden: false,
        }
    }

    #[test]
    fn submenu_groups_commands_up_to_three_levels() {
        let mut a = cmd("vscode-open", "any", &[]);
        a.submenu = Some("VSCode で開く".into());
        let mut b = cmd("vscode-diff", "any", &[]);
        b.submenu = Some("VSCode で開く".into());
        let mut c = cmd("deep", "any", &[]);
        c.submenu = Some("ツール/変換/画像/さらに深い".into()); // 4 階層目は切り捨て
        let mut h = cmd("hidden-one", "any", &[]);
        h.hidden = true;
        let top = cmd("top", "any", &[]);
        let items = user_command_items(&[a, b, c, h, top], None);
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["VSCode で開く", "ツール", "top"]); // hidden は出ない
        let vs = &items[0];
        assert_eq!(vs.action, MenuAction::Submenu);
        assert_eq!(vs.children.len(), 2); // 同じ submenu が 1 つにまとまる
                                          // 3 階層で打ち切り: ツール ▸ 変換 ▸ 画像 (葉は 3 階層目の中)
        let tool = &items[1];
        let conv = &tool.children[0];
        let img = &conv.children[0];
        assert_eq!(
            (tool.label.as_str(), conv.label.as_str(), img.label.as_str()),
            ("ツール", "変換", "画像")
        );
        assert_eq!(img.children[0].label, "deep");
        assert!(img.children[0].children.is_empty());
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
                "新しいファイル",
                "プロパティ"
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
                "新しいファイル",
                "プロパティ"
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
