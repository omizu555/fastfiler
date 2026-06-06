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
    let appdata = std::env::var("APPDATA").map_err(|_| AppError::EnvMissing("APPDATA"))?;
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

pub fn user_commands_dir() -> AppResult<String> {
    let p = commands_dir_inner()?;
    Ok(p.to_string_lossy().into_owned())
}

pub fn list_user_commands() -> AppResult<Vec<UserCommand>> {
    let dir = commands_dir_inner()?;
    let file = dir.join("commands.json");
    if !file.is_file() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&file)?;
    let cmds: Vec<UserCommand> = serde_json::from_str(&text)
        .map_err(|e| AppError::Parse(format!("commands.json parse error: {}", e)))?;
    Ok(cmds.into_iter().filter(|c| !c.hidden).collect())
}

#[derive(Deserialize)]
pub struct RunCtx {
    pub paths: Vec<String>,
    pub cwd: String,
}

pub fn run_user_command(id: String, ctx: RunCtx) -> AppResult<()> {
    let cmds = list_user_commands()?;
    let cmd = cmds
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| AppError::NotFound(format!("user command not found: {}", id)))?;

    let exec = expand_placeholders(&cmd.exec, &ctx, false);
    let mut args: Vec<String> = Vec::new();
    for a in &cmd.args {
        // "{paths}" 単独の引数は 1 パス = 1 引数として展開する。空白区切りの
        // 1 引数に詰めるとクオートが入れ子になり、空白を含むパスが壊れる
        // (7z に渡すとパスが分割され 0 files になる実害があった)。
        if a == "{paths}" {
            args.extend(ctx.paths.iter().cloned());
            continue;
        }
        let e = expand_placeholders(a, &ctx, false);
        // プレースホルダ展開で空になった引数 (例: 背景メニューでの {path}) は
        // 除外する。空文字をそのまま渡すと `code ""` のような壊れた起動になる。
        if e.is_empty() && !a.is_empty() {
            continue;
        }
        args.push(e);
    }
    let working_dir = match &cmd.cwd {
        Some(s) => expand_placeholders(s, &ctx, false),
        None => ctx.cwd.clone(),
    };

    if cmd.shell {
        return build_shell_command(&exec, &args, &working_dir)
            .spawn()
            .map(|_| ())
            .map_err(|e| AppError::Other(format!("spawn failed ({}): {}", cmd.id, e)));
    }

    // 直接起動はエクスプローラと同じ ShellExecuteW 経路で行う。
    // CreateProcess 直接起動だと親の標準ハンドルが継承され、powershell 等の
    // コンソールアプリが「新しいウィンドウは開くが入出力は親側」になり
    // 開いた直後に閉じてしまう (shell::launch_with_shell のコメント参照)。
    let params = if args.is_empty() {
        None
    } else {
        Some(
            args.iter()
                .map(|a| quote_if_needed(a))
                .collect::<Vec<_>>()
                .join(" "),
        )
    };
    let cwd = (!working_dir.is_empty() && Path::new(&working_dir).is_dir())
        .then(|| working_dir.clone());
    match crate::shell::launch_with_shell(exec.clone(), params, cwd) {
        Ok(()) => Ok(()),
        // "code" (実体は code.cmd) などで見つからない場合は cmd /c 経由で再試行する。
        Err(e) => build_shell_command(&exec, &args, &working_dir)
            .spawn()
            .map(|_| ())
            .map_err(|e2| {
                AppError::Other(format!(
                    "起動失敗 ({}): {} (cmd /c 再試行も失敗: {})",
                    cmd.id, e, e2
                ))
            }),
    }
}

/// `cmd.exe /c "<exec> <args...>"` を組み立てる
/// (shell=true 指定と、.cmd/.bat の NotFound フォールバックで共用)。
fn build_shell_command(exec: &str, args: &[String], working_dir: &str) -> Command {
    let mut full = quote_if_needed(exec);
    for a in args {
        full.push(' ');
        full.push_str(&quote_if_needed(a));
    }
    let mut c = Command::new("cmd.exe");
    c.arg("/c").arg(full);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW = 0x08000000 — cmd 自体のコンソールを一瞬も出さない
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    if !working_dir.is_empty() && Path::new(working_dir).is_dir() {
        c.current_dir(working_dir);
    }
    c
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
//   {paths}  選択全件 (単独の引数に書くと 1 パス = 1 引数で展開)
//   {name}   ファイル名 (拡張子付)
//   {stem}   拡張子なしファイル名
//   {ext}    .xxx 形式の拡張子
//   {parent} 親フォルダ
//   {cwd}    現在ペインのパス
//   {count}  選択数
//
// when (どこのメニューに出すか):
//   "file"       … ファイルの行のみ
//   "folder"     … フォルダの行のみ
//   "selection"  … 行のみ (ファイル・フォルダ両方)
//   "background" … 何もないところ (背景) のみ
//   "drop"       … 右ボタンドラッグ&ドロップのメニュー
//                  ({paths}=ドラッグした項目 / {cwd}=ドロップ先フォルダ)
//   "any" (既定) … 行・背景の両方
//
// 補足: 作業フォルダは現在ペインのパス。ターミナル系は起動するだけで
// そのフォルダで開きます。
//
// 注意: コメント (//) は **commands.json では使えません**。このサンプルは
// 参考用なので、実ファイルではコメントを削除してください。
[
  {
    "id": "vscode-open",
    "label": "VSCode で開く",
    "exec": "code",
    "args": ["{path}"],
    "when": "selection"
  },
  {
    "id": "7z-compress",
    "label": "7-Zip で圧縮 (.7z)",
    "exec": "C:\\Program Files\\7-Zip\\7z.exe",
    "args": ["a", "{parent}\\{stem}.7z", "{paths}"],
    "when": "selection"
  },
  {
    "id": "vscode-here",
    "label": "ここを VSCode で開く",
    "exec": "code",
    "args": ["{cwd}"],
    "when": "background"
  },
  {
    "id": "powershell-here",
    "label": "ここで PowerShell",
    "exec": "powershell.exe",
    "args": ["-NoExit"],
    "when": "background"
  },
  {
    "id": "cmd-here",
    "label": "ここで CMD",
    "exec": "cmd.exe",
    "args": ["/k"],
    "when": "background"
  },
  {
    "id": "terminal-here",
    "label": "ここでターミナル",
    "exec": "wt.exe",
    "args": ["-d", "{cwd}"],
    "when": "background"
  },
  {
    "id": "7z-compress-drop",
    "label": "ここに 7-Zip で圧縮 (.7z)",
    "exec": "C:\\Program Files\\7-Zip\\7z.exe",
    "args": ["a", "{cwd}\\{stem}.7z", "{paths}"],
    "when": "drop"
  }
]
"#;
