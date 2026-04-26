// ユーザー定義コマンド (v1.13)
//
// %APPDATA%\fastfiler\commands\commands.json から読み込み、右クリックメニューに
// 任意の外部コマンド項目を追加する。
//
// プレースホルダ:
//   {path}    選択 1 件目のフルパス
//   {paths}   選択全件 (空白区切り、自動クオート)
//   {name}    basename (拡張子付)
//   {stem}    basename (拡張子なし)
//   {ext}     拡張子 (.xxx)
//   {parent}  親フォルダ
//   {cwd}     現在ペインのパス
//   {count}   選択件数

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserCommand {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon: Option<String>,
    pub exec: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default = "default_when")]
    pub when: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub submenu: Option<String>,
    #[serde(default)]
    pub shell: bool,
    #[serde(default)]
    pub hidden: bool,
}

fn default_when() -> String {
    "any".to_string()
}

fn commands_dir_inner() -> AppResult<PathBuf> {
    let appdata =
        std::env::var("APPDATA").map_err(|_| AppError::Other("APPDATA not set".into()))?;
    let dir = PathBuf::from(appdata).join("fastfiler").join("commands");
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
        let sample = dir.join("commands.json.sample");
        if !sample.exists() {
            let _ = fs::write(&sample, SAMPLE_JSON);
        }
    }
    Ok(dir)
}

#[tauri::command]
pub fn user_commands_dir() -> AppResult<String> {
    let p = commands_dir_inner()?;
    Ok(p.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn list_user_commands() -> AppResult<Vec<UserCommand>> {
    let dir = commands_dir_inner()?;
    let file = dir.join("commands.json");
    if !file.is_file() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&file)?;
    let cmds: Vec<UserCommand> = serde_json::from_str(&text)
        .map_err(|e| AppError::Other(format!("commands.json parse error: {}", e)))?;
    Ok(cmds.into_iter().filter(|c| !c.hidden).collect())
}

#[derive(Deserialize)]
pub struct RunCtx {
    pub paths: Vec<String>,
    pub cwd: String,
}

#[tauri::command]
pub fn run_user_command(id: String, ctx: RunCtx) -> AppResult<()> {
    let cmds = list_user_commands()?;
    let cmd = cmds
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| AppError::NotFound(format!("user command not found: {}", id)))?;

    let exec = expand_placeholders(&cmd.exec, &ctx, false);
    let args: Vec<String> = cmd
        .args
        .iter()
        .map(|a| expand_placeholders(a, &ctx, false))
        .collect();
    let working_dir = match &cmd.cwd {
        Some(s) => expand_placeholders(s, &ctx, false),
        None => ctx.cwd.clone(),
    };

    let mut command = if cmd.shell {
        // cmd /c "<exec> <args...>"
        let mut full = quote_if_needed(&exec);
        for a in &args {
            full.push(' ');
            full.push_str(&quote_if_needed(a));
        }
        let mut c = Command::new("cmd.exe");
        c.arg("/c").arg(full);
        c
    } else {
        let mut c = Command::new(&exec);
        c.args(&args);
        c
    };

    if !working_dir.is_empty() && Path::new(&working_dir).is_dir() {
        command.current_dir(&working_dir);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW = 0x08000000 — コンソール ウィンドウを開かない
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // shell=false で GUI/CLI 双方の場合は通常起動。shell=true のときは
        // cmd 自体のウィンドウが一瞬出ないよう CREATE_NO_WINDOW を付ける。
        if cmd.shell {
            command.creation_flags(CREATE_NO_WINDOW);
        }
    }

    command
        .spawn()
        .map(|_| ())
        .map_err(|e| AppError::Other(format!("spawn failed ({}): {}", cmd.id, e)))
}

fn expand_placeholders(input: &str, ctx: &RunCtx, _quote_paths: bool) -> String {
    let first = ctx.paths.first().cloned().unwrap_or_default();
    let p = Path::new(&first);
    let name = p
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = p
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    let parent = p
        .parent()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let paths_joined: String = ctx
        .paths
        .iter()
        .map(|s| quote_if_needed(s))
        .collect::<Vec<_>>()
        .join(" ");
    let count = ctx.paths.len().to_string();

    input
        .replace("{paths}", &paths_joined)
        .replace("{path}", &first)
        .replace("{name}", &name)
        .replace("{stem}", &stem)
        .replace("{ext}", &ext)
        .replace("{parent}", &parent)
        .replace("{cwd}", &ctx.cwd)
        .replace("{count}", &count)
}

fn quote_if_needed(s: &str) -> String {
    if s.is_empty() {
        return "\"\"".to_string();
    }
    if s.contains(' ') || s.contains('\t') {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

const SAMPLE_JSON: &str = r#"// FastFiler ユーザー定義コマンド サンプル
//
// このファイルを `commands.json` にリネーム (またはコピー) して編集すると、
// 右クリックメニューに項目が追加されます。
//
// プレースホルダ:
//   {path}   選択 1 件目のフルパス
//   {paths}  選択全件 (空白区切り)
//   {name}   ファイル名 (拡張子付)
//   {stem}   拡張子なしファイル名
//   {ext}    .xxx 形式の拡張子
//   {parent} 親フォルダ
//   {cwd}    現在ペインのパス
//   {count}  選択数
//
// when:
//   "file" / "folder" / "any" (既定) / "background" (空白右クリック)
//
// 注意: コメント (//) は **commands.json では使えません**。このサンプルは
// 参考用なので、実ファイルではコメントを削除してください。
[
  {
    "id": "vscode",
    "label": "VSCode で開く",
    "icon": "🆚",
    "exec": "code",
    "args": ["{path}"],
    "when": "any"
  },
  {
    "id": "powershell-here",
    "label": "ここで PowerShell",
    "icon": "💻",
    "exec": "powershell.exe",
    "args": ["-NoExit", "-Command", "Set-Location -LiteralPath '{cwd}'"],
    "when": "background"
  },
  {
    "id": "7z-compress",
    "label": "7-Zip で圧縮 (.7z)",
    "icon": "📦",
    "exec": "C:\\Program Files\\7-Zip\\7z.exe",
    "args": ["a", "{parent}\\{stem}.7z", "{paths}"],
    "when": "any",
    "submenu": "圧縮"
  },
  {
    "id": "7z-extract",
    "label": "7-Zip で展開 (ここに)",
    "icon": "📂",
    "exec": "C:\\Program Files\\7-Zip\\7z.exe",
    "args": ["x", "{path}", "-o{parent}", "-y"],
    "when": "file",
    "extensions": [".zip", ".7z", ".rar", ".tar", ".gz"],
    "submenu": "圧縮"
  }
]
"#;
