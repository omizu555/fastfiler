# ユーザーコマンドの使い方 (commands.json ガイド)

最終更新: 2026-06-07

右クリックメニュー (行 / 背景 / 右ボタン D&D) に**任意の外部コマンド**を追加できる。
FastFiler の拡張はこの仕組みに一本化されている ([ADR 0003](./adr/0003-remove-plugin-system.md))。

設定ファイル:

```
%APPDATA%\fastfiler\commands\commands.json
```

初回起動時に同じ場所へ `commands.json.sample` (コメント付きサンプル) が生成される。
これを `commands.json` という名前でコピーして編集する
(**実ファイルでは `//` コメント不可** — JSON 標準のため)。
設定画面 (⚙) の「フォルダを開く — ユーザーコマンド」でフォルダをすぐ開ける。
変更は次にメニューを開いたときに反映される (再起動不要)。

## 基本形

```json
[
  {
    "id": "vscode-open",
    "label": "VSCode で開く",
    "exec": "code",
    "args": ["{path}"],
    "when": "selection"
  }
]
```

## フィールド一覧 (各キーの説明)

| キー | 必須 | 説明 |
|---|---|---|
| `id` | ✅ | 一意な識別子 (英数字とハイフン推奨) |
| `label` | ✅ | メニューに表示する名前 |
| `exec` | ✅ | 起動するコマンド (フルパス or PATH 上の名前。`code` のような .cmd 実体も可 — 見つからなければ `cmd /c` 経由で再試行される) |
| `args` | — | 引数の配列。プレースホルダ使用可。**`"{paths}"` を単独の引数として書くと 1 パス = 1 引数で展開**される (空白を含むパスが壊れない)。展開して空になった引数は自動で除外 |
| `cwd` | — | 作業フォルダ。省略時は現在ペインのフォルダ |
| `when` | — | どのメニューに出すか (下表)。省略時 `"any"` |
| `extensions` | — | 拡張子フィルタの配列 (例 `["jpg", "png"]`)。行メニューで、選択ファイルの拡張子が一致するときだけ表示。大文字小文字無視・先頭の `.` 不要。空 = 全部に表示 |
| `shell` | — | `true` で `cmd /c` 経由で起動 (パイプやビルトインコマンドを使うとき)。コンソール窓は出ない |
| `hidden` | — | `true` でメニューに出さない (一時的に無効化したいとき) |
| `submenu` | — | サブメニューに畳む。`"/"` 区切りで**最大 3 階層** (下の「サブメニュー」参照)。省略時はトップレベル |
| `icon` | — | 予約フィールド (現バージョンでは未使用) |

### when の値

| 値 | 表示される場所 |
|---|---|
| `"file"` | ファイルの行を右クリックしたときのみ |
| `"folder"` (または `"dir"`) | フォルダの行のみ |
| `"selection"` | 行のみ (ファイル・フォルダ両方) |
| `"background"` | 何もないところ (背景) のみ |
| `"drop"` | **右ボタンドラッグ & ドロップのメニュー** (`{paths}` = ドラッグした項目 / `{cwd}` = ドロップ先フォルダ) |
| `"any"` (既定) | 行・背景の両方 |

メニューに並ぶのは各メニューにつき**先頭から最大 50 件** (commands.json の記述順)。

### サブメニュー (`submenu`)

コマンドが増えてメニューが長くなったら、`submenu` で**サブメニューに畳める**。
同じ `submenu` を持つコマンドが 1 つの「▸」メニューにまとまる。`"/"` 区切りで
**最大 3 階層**までネストできる。

```json
[
  { "id": "vscode-open", "label": "ファイルを開く", "exec": "code", "args": ["{path}"], "when": "selection", "submenu": "VSCode で開く" },
  { "id": "vscode-here", "label": "ここで開く",     "exec": "code", "args": ["{cwd}"],  "when": "background", "submenu": "VSCode で開く" }
]
```

これで右クリックメニューには `VSCode で開く ▸` の 1 行だけが出て、その中に
「ファイルを開く」「ここで開く」が並ぶ。`"submenu": "ツール/圧縮"` のように書けば
`ツール ▸ 圧縮 ▸` の 2 階層に入る。`submenu` を省いたコマンドは従来どおり
トップレベルに並ぶ (混在可)。`when`/`extensions` で絞られて中身が無くなった
サブメニューは自動的に出ない。

サブメニューは**右クリックメニューと、右ボタン D&D のチューザー (`when: "drop"`) の
両方**で効く。`drop` のコマンドに `submenu` を付ければチューザー内でもまとまる。

## プレースホルダ (各キーの説明)

`exec` / `args` / `cwd` の中で使える。実行時に選択状態で置換される。

| プレースホルダ | 展開結果 | 例 (`C:\work\report.xlsx` を選択時) |
|---|---|---|
| `{path}` | 選択 1 件目のフルパス | `C:\work\report.xlsx` |
| `{paths}` | 選択全件 (単独引数なら 1 パス = 1 引数) | `C:\work\a.txt` `C:\work\b.txt` … |
| `{name}` | ファイル名 (拡張子付き) | `report.xlsx` |
| `{stem}` | ファイル名 (拡張子なし) | `report` |
| `{ext}` | 拡張子 (`.` 付き) | `.xlsx` |
| `{parent}` | 親フォルダ | `C:\work` |
| `{cwd}` | 現在ペインのフォルダ (`when:"drop"` ではドロップ先) | `C:\work` |
| `{count}` | 選択件数 | `1` |

背景メニューでは選択がないため `{path}` 系は空文字に展開される
(空になった引数は除外されるので、背景用コマンドは `{cwd}` を使う)。

## レシピ集

> コピペで使える設定例をもっと見たいときは **[COMMANDS-RECIPES.md](./COMMANDS-RECIPES.md)**
> (エクスプローラ / Claude Code / 圧縮・展開 / ハッシュ表示などの全部入りサンプル)。

```json
[
  {
    "id": "7z-compress",
    "label": "7-Zip で圧縮 (.7z)",
    "exec": "C:\\Program Files\\7-Zip\\7z.exe",
    "args": ["a", "{parent}\\{stem}.7z", "{paths}"],
    "when": "selection"
  },
  {
    "id": "powershell-here",
    "label": "ここで PowerShell",
    "exec": "powershell.exe",
    "args": ["-NoExit"],
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
    "id": "img-only",
    "label": "Honeyview で開く",
    "exec": "C:\\Program Files\\Honeyview\\Honeyview.exe",
    "args": ["{path}"],
    "when": "file",
    "extensions": ["jpg", "png", "gif", "webp"]
  },
  {
    "id": "7z-compress-drop",
    "label": "ここに 7-Zip で圧縮 (.7z)",
    "exec": "C:\\Program Files\\7-Zip\\7z.exe",
    "args": ["a", "{cwd}\\{stem}.7z", "{paths}"],
    "when": "drop"
  }
]
```

## 起動の仕組み (トラブル時に)

- 通常はエクスプローラと同じ `ShellExecuteW` 経路で起動する。PowerShell 等の
  コンソールアプリも正しく新しい窓で開く
- `.cmd` / `.bat` (例: `code` の実体 `code.cmd`) は `cmd /c` + 不可視フラグで起動する。
  バッチ用のコンソール窓は出ず、そこから開く GUI (VS Code 等) だけが残る
- `exec` が見つからない場合は `cmd /c` で自動再試行する (`code` → `code.cmd` など)
- それでも動かないときは `"shell": true` を試す
- プロパティダイアログのような「呼び出しプロセスの生存が必要な UI」は
  ユーザーコマンドでは出せない → 行の **Shift+右クリック** (Windows シェルメニュー) を使う

## 関連

- 操作全般: [USAGE.md](./USAGE.md) / キー割り当て: [HOTKEYS.md](./HOTKEYS.md)
- 拡張方針の背景: [ADR 0003](./adr/0003-remove-plugin-system.md) / [ADR 0007](./adr/0007-shell-context-menu-shift-only.md)
