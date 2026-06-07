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

    // セキュリティ: ベア名 (`code` 等) は PATH (カレントディレクトリを除外) で
    // 絶対パスに解決してから起動する。閲覧中フォルダに置かれた悪意ある `code.exe`
    // 等が検索順序で実行される (バイナリプランティング) のを防ぐ。
    // 解決できなければ従来どおり exec をそのまま使う (ShellExecuteW の App Paths
    // 等に委ねる) — 退行はしない。
    let exec = resolve_in_path(&exec).unwrap_or(exec);

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
    //
    // セキュリティ: 各引数は cmd_quote で必ずダブルクオートする。標的が .cmd/.bat
    // だと ShellExecuteW 経由でも cmd が引数を再解釈するため、`a & calc.exe` の
    // ような攻撃者制御のファイル名でメタ文字が実行されるのを防ぐ。
    let params = if args.is_empty() {
        None
    } else {
        Some(
            args.iter()
                .map(|a| cmd_quote(a))
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
///
/// セキュリティ: ファイル名由来の値 ({path} 等) が cmd のメタ文字 (& | < > ^ ( ))
/// を含んでもコマンドとして解釈されないよう、各トークンを cmd_quote で必ず
/// ダブルクオートし (引用符内では cmd はメタ文字をリテラル扱いする)、`/c` の
/// 引用符剥がし規則に合わせて行全体をさらに 1 組の引用符で囲んで raw_arg で渡す。
fn build_shell_command(exec: &str, args: &[String], working_dir: &str) -> Command {
    let mut inner = cmd_quote(exec);
    for a in args {
        inner.push(' ');
        inner.push_str(&cmd_quote(a));
    }
    // `cmd /c "<...>"`: 先頭が " のとき cmd は最初と最後の " を剥がして残りを
    // 実行する。各トークンを引用済みなので、全体を 1 組の " で囲んでおく。
    let payload = format!("\"{inner}\"");

    let mut c = Command::new("cmd.exe");
    c.arg("/c");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // raw_arg: Rust による再クオート (cmd のメタ文字を考慮しない) を回避し、
        // 上で厳密に組み立てた行をそのまま cmd へ渡す。
        c.raw_arg(&payload);
        // CREATE_NO_WINDOW = 0x08000000 — cmd 自体のコンソールを一瞬も出さない
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        c.arg(&payload);
    }
    if !working_dir.is_empty() && Path::new(working_dir).is_dir() {
        c.current_dir(working_dir);
    }
    c
}

/// ベア名の実行ファイルを PATH (+ PATHEXT) で絶対パスに解決する。
/// **カレントディレクトリは検索対象に含めない** (PATH の空エントリ = cwd を除外)
/// ことでバイナリプランティングを防ぐ。パス区切りを含む / 絶対パスの exec、
/// および見つからない場合は None を返し、呼び出し側は元の exec をそのまま使う。
fn resolve_in_path(exec: &str) -> Option<String> {
    let p = Path::new(exec);
    if p.is_absolute() || exec.contains('\\') || exec.contains('/') {
        return None;
    }
    let path_var = std::env::var_os("PATH")?;
    let exts: Vec<String> = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
        .split(';')
        .map(|e| e.trim().to_string())
        .filter(|e| !e.is_empty())
        .collect();
    let has_ext = p.extension().is_some();
    for dir in std::env::split_paths(&path_var) {
        // 空エントリは Windows ではカレントディレクトリを意味する → 除外。
        if dir.as_os_str().is_empty() {
            continue;
        }
        if has_ext {
            let cand = dir.join(exec);
            if cand.is_file() {
                return Some(cand.to_string_lossy().into_owned());
            }
        } else {
            for ext in &exts {
                let cand = dir.join(format!("{exec}{ext}"));
                if cand.is_file() {
                    return Some(cand.to_string_lossy().into_owned());
                }
            }
        }
    }
    None
}

/// 1 トークンを cmd.exe 用に必ずダブルクオートする。引用符内では cmd は
/// `& | < > ^ ( )` をリテラルとして扱うため、ファイル名由来のメタ文字を無害化できる。
/// 内部の `"` は cmd 規約の `""` にエスケープ (NTFS のファイル名に `"` は使えないため
/// 実害ケースは設定値のみ)。`%VAR%` の環境変数展開だけは引用符内でも起こり得るが、
/// それで起きるのは情報露出程度でコマンド実行には至らない。
fn cmd_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_quote_wraps_and_neutralizes_metachars() {
        // 空白の有無にかかわらず常にダブルクオートする。
        assert_eq!(cmd_quote("plain"), "\"plain\"");
        assert_eq!(cmd_quote("has space"), "\"has space\"");
        // cmd のメタ文字を含むファイル名 — 引用符内に入りリテラル化される。
        assert_eq!(cmd_quote("x&calc.exe"), "\"x&calc.exe\"");
        assert_eq!(cmd_quote("a|b>c"), "\"a|b>c\"");
        // 内部の " は "" にエスケープ (cmd 規約)。
        assert_eq!(cmd_quote("a\"b"), "\"a\"\"b\"");
    }

    /// .bat を標的にした end-to-end テスト (BatBadBut クラスの最重要ケース)。
    /// バッチファイルを「ツール」に見立て、メタ文字入りのファイル名を引数に渡す。
    /// 注入が起きれば marker が作られ、起きなければバッチが**リテラルの引数**を
    /// out.txt に書く。両方 (注入なし & 引数が壊れず届く) を確認する。
    #[cfg(windows)]
    #[test]
    fn no_injection_and_arg_intact_for_batch_target() {
        let dir = std::env::temp_dir().join("ff_sec_batch_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let marker = dir.join("PWNED.txt");
        let out = dir.join("out.txt");
        // ツール: 第1引数を out.txt へ書くだけのバッチ。バッチ自身が %~1 を
        // 再注入しないよう引用して echo する (バッチ側の二次注入を排除)。
        let tool = dir.join("tool.bat");
        std::fs::write(&tool, "@echo \"%~1\">\"%~dp0out.txt\"\r\n").unwrap();
        // 攻撃ファイル名: & で marker を作ろうとする (スペース無し)。
        let malicious = format!("x&echo PWNED>{}", marker.display());
        let mut c = build_shell_command(
            tool.to_str().unwrap(),
            &[malicious.clone()],
            dir.to_str().unwrap(),
        );
        let _ = c.status();
        // (1) 注入が起きていない。
        assert!(!marker.exists(), "コマンド注入が成功: marker が作成された");
        // (2) バッチはリテラルの引数 (& を含む) を受け取れている。
        let got = std::fs::read_to_string(&out).unwrap_or_default();
        assert!(
            got.contains("x&echo PWNED>"),
            "引数がツールに正しく届いていない: out={got:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_shell_command_payload_is_outer_quoted() {
        // 攻撃シナリオ: `code` に `C:\d\x&calc.exe` を渡しても & が
        // コマンド区切りにならないこと。組み立てた cmd /c 行を検証する。
        let exec = "code";
        let args = vec!["C:\\d\\x&calc.exe".to_string()];
        // build_shell_command 内部と同じ手順で payload を再現。
        let mut inner = cmd_quote(exec);
        inner.push(' ');
        inner.push_str(&cmd_quote(&args[0]));
        let payload = format!("\"{inner}\"");
        // 先頭末尾が外側の引用符で、各トークンが個別に引用されている。
        assert_eq!(payload, "\"\"code\" \"C:\\d\\x&calc.exe\"\"");
        // cmd が先頭末尾の " を剥がすと、各トークンが引用された安全な行が残る。
        let stripped = &payload[1..payload.len() - 1];
        assert_eq!(stripped, "\"code\" \"C:\\d\\x&calc.exe\"");
    }
}
