# ユーザーコマンド レシピ集 (commands.json サンプル)

最終更新: 2026-06-11

右クリックメニュー / 背景メニュー / 右ボタン D&D に追加できる**コピペで使える設定例**を集めたもの。
各キーやプレースホルダの意味は [COMMANDS.md](./COMMANDS.md) を参照。

## 使い方

設定ファイルはここ:

```
%APPDATA%\fastfiler\commands\commands.json
```

設定画面 (⚙) の「フォルダを開く — ユーザーコマンド」、または背景の右クリック →
「設定 ▸ ユーザーコマンドの設定...」で**新しいタブ**にこのフォルダが開く。

- ファイルが無ければ、同じ場所の `commands.json.sample` を `commands.json` にコピーして編集する。
- **実ファイルでは `//` コメントは使えない** (JSON 標準のため)。下のサンプルをそのまま貼るときはコメント行を消すこと。
- 配列 `[ ... ]` の中にオブジェクトを `,` 区切りで並べる。各メニューに出るのは記述順で**先頭 10 件まで**。
- 変更は次にメニューを開いたとき反映される (再起動不要)。

> パスはすべて自動でクオートされる。`C:\Program Files\...` のように空白を含む `exec` も
> そのまま書いてよい (JSON なので `\` は `\\` と書く)。

---

## 同梱のデフォルト

初回生成される `commands.json.sample` に入っているもの。挙動の確認用にここへ再掲する。

| ラベル | when | submenu | 説明 |
|---|---|---|---|
| ファイルを開く | `selection` | VSCode で開く | 選択を VS Code で開く |
| ここで開く | `background` | VSCode で開く | 今のフォルダを VS Code で開く |
| 7-Zip で圧縮 (.7z) | `selection` | — | 選択をまとめて親フォルダに `.7z` 圧縮 |
| ここで PowerShell | `background` | — | 今のフォルダで PowerShell を開く |
| ここで CMD | `background` | — | 今のフォルダで CMD を開く |
| ここでターミナル | `background` | — | 今のフォルダで Windows Terminal を開く |
| ここに 7-Zip で圧縮 (.7z) | `drop` | — | 右ボタン D&D した項目をドロップ先に `.7z` 圧縮 |

VSCode 系は `submenu` で `VSCode で開く ▸` の 1 つにまとめてある (サブメニューの例)。

```json
[
  { "id": "vscode-open",      "label": "ファイルを開く",           "exec": "code",                              "args": ["{path}"],                          "when": "selection",  "submenu": "VSCode で開く" },
  { "id": "vscode-here",      "label": "ここで開く",                "exec": "code",                              "args": ["{cwd}"],                           "when": "background",  "submenu": "VSCode で開く" },
  { "id": "powershell-here",  "label": "ここで PowerShell",         "exec": "powershell.exe",                    "args": ["-NoExit"],                         "when": "background" },
  { "id": "cmd-here",         "label": "ここで CMD",                "exec": "cmd.exe",                           "args": ["/k"],                              "when": "background" },
  { "id": "terminal-here",    "label": "ここでターミナル",          "exec": "wt.exe",                            "args": ["-d", "{cwd}"],                     "when": "background" },
  { "id": "7z-compress",      "label": "7-Zip で圧縮 (.7z)",        "exec": "C:\\Program Files\\7-Zip\\7z.exe",  "args": ["a", "{parent}\\{stem}.7z", "{paths}"], "when": "selection" },
  { "id": "7z-compress-drop", "label": "ここに 7-Zip で圧縮 (.7z)", "exec": "C:\\Program Files\\7-Zip\\7z.exe",  "args": ["a", "{cwd}\\{stem}.7z", "{paths}"],    "when": "drop" }
]
```

---

## サブメニューでまとめる

メニューが長くなったら `submenu` でサブメニューに畳める。同じ `submenu` 名の
コマンドが 1 つの `▸` にまとまる。`"/"` 区切りで**最大 3 階層**までネスト可。

```json
[
  { "id": "vscode-file",  "label": "ファイルを開く", "exec": "code",        "args": ["{path}"], "when": "selection",  "submenu": "VSCode で開く" },
  { "id": "vscode-here2", "label": "ここで開く",      "exec": "code",        "args": ["{cwd}"],  "when": "background",  "submenu": "VSCode で開く" },

  { "id": "zip-here",     "label": "ZIP で圧縮",     "exec": "C:\\Program Files\\7-Zip\\7z.exe", "args": ["a", "-tzip", "{parent}\\{stem}.zip", "{paths}"], "when": "selection", "submenu": "ツール/圧縮" },
  { "id": "sevenz-here",  "label": "7z で圧縮",      "exec": "C:\\Program Files\\7-Zip\\7z.exe", "args": ["a", "{parent}\\{stem}.7z", "{paths}"],          "when": "selection", "submenu": "ツール/圧縮" }
]
```

- 上は `VSCode で開く ▸ {ファイルを開く, ここで開く}` の 2 択サブメニュー。
- 下は `ツール ▸ 圧縮 ▸ {ZIP で圧縮, 7z で圧縮}` の 2 階層ネスト。
- `submenu` を書かなければ従来どおりトップレベルに並ぶ (混在 OK)。

サブメニューは**右ボタン D&D のチューザー (`when: "drop"`) でも効く**。圧縮系を
畳む例:

```json
[
  { "id": "7z-drop",  "label": "ここに 7-Zip で圧縮 (.7z)", "exec": "C:\\Program Files\\7-Zip\\7z.exe", "args": ["a", "{cwd}\\{stem}.7z", "{paths}"],          "when": "drop", "submenu": "ツール/圧縮" },
  { "id": "zip-drop", "label": "ここに ZIP で圧縮 (.zip)",   "exec": "C:\\Program Files\\7-Zip\\7z.exe", "args": ["a", "-tzip", "{cwd}\\{stem}.zip", "{paths}"], "when": "drop", "submenu": "ツール/圧縮" }
]
```

ドロップして右ボタンを離すと「ここにコピー / ここに移動」の下に `ツール ▸ 圧縮 ▸`
が出て、その中に 2 つの圧縮コマンドがまとまる。

---

## エクスプローラ / シェル連携

```json
[
  {
    "id": "explorer-here",
    "label": "エクスプローラで開く",
    "exec": "explorer.exe",
    "args": ["{cwd}"],
    "when": "background"
  },
  {
    "id": "explorer-folder",
    "label": "エクスプローラで開く",
    "exec": "explorer.exe",
    "args": ["{path}"],
    "when": "folder"
  },
  {
    "id": "explorer-reveal",
    "label": "エクスプローラで場所を表示",
    "exec": "explorer.exe",
    "args": ["/select,{path}"],
    "when": "file"
  },
  {
    "id": "copy-fullpath",
    "label": "フルパスをコピー",
    "exec": "powershell.exe",
    "args": ["-NoProfile", "-Command", "Set-Clipboard -Value '{path}'"],
    "when": "selection",
    "shell": true
  }
]
```

- `explorer-reveal` は選択ファイルを Windows エクスプローラで**選択した状態**で開く (`/select,`)。
- `copy-fullpath` は `shell: true` なので窓を出さずにクリップボードへコピーする。

---

## エディタ / ターミナル

```json
[
  {
    "id": "notepad-open",
    "label": "メモ帳で開く",
    "exec": "notepad.exe",
    "args": ["{path}"],
    "when": "file"
  },
  {
    "id": "wsl-here",
    "label": "ここで WSL",
    "exec": "wt.exe",
    "args": ["-d", "{cwd}", "wsl.exe"],
    "when": "background"
  },
  {
    "id": "gitbash-here",
    "label": "ここで Git Bash",
    "exec": "C:\\Program Files\\Git\\git-bash.exe",
    "args": ["--cd={cwd}"],
    "when": "background"
  }
]
```

---

## Claude Code を開く

`claude` コマンド (Claude Code CLI) が PATH にあること。今のフォルダで Claude Code を起動する。

```json
[
  {
    "id": "claude-here",
    "label": "ここで Claude Code",
    "exec": "wt.exe",
    "args": ["-d", "{cwd}", "cmd", "/k", "claude"],
    "when": "background"
  },
  {
    "id": "claude-here-ps",
    "label": "ここで Claude Code (PowerShell)",
    "exec": "powershell.exe",
    "args": ["-NoExit", "-Command", "claude"],
    "when": "background"
  }
]
```

- `claude-here` は Windows Terminal を今のフォルダで開いて `claude` を実行する (推奨)。
- Windows Terminal が無い環境では `claude-here-ps` の PowerShell 版を使う
  (こちらは今のフォルダで PowerShell が開き、その中で `claude` が走る)。

---

## 圧縮 / 展開 (7-Zip)

`C:\Program Files\7-Zip\7z.exe` を想定 (インストール先が違う場合はパスを直す)。

```json
[
  {
    "id": "7z-extract-here",
    "label": "ここに展開 (7-Zip)",
    "exec": "C:\\Program Files\\7-Zip\\7z.exe",
    "args": ["x", "{path}", "-o{parent}\\{stem}", "-y"],
    "when": "file",
    "extensions": ["zip", "7z", "rar", "tar", "gz", "lzh"]
  },
  {
    "id": "7z-compress-zip",
    "label": "ZIP で圧縮",
    "exec": "C:\\Program Files\\7-Zip\\7z.exe",
    "args": ["a", "-tzip", "{parent}\\{stem}.zip", "{paths}"],
    "when": "selection"
  }
]
```

- `7z-extract-here` は書庫を `<書庫名>` フォルダへ展開する。`extensions` で書庫拡張子のときだけメニューに出す。

---

## 画像 / メディア (拡張子で絞り込み)

対応アプリのインストール先に合わせて `exec` を直すこと。

```json
[
  {
    "id": "honeyview",
    "label": "Honeyview で開く",
    "exec": "C:\\Program Files\\Honeyview\\Honeyview.exe",
    "args": ["{path}"],
    "when": "file",
    "extensions": ["jpg", "jpeg", "png", "gif", "webp", "bmp"]
  },
  {
    "id": "vlc",
    "label": "VLC で再生",
    "exec": "C:\\Program Files\\VideoLAN\\VLC\\vlc.exe",
    "args": ["{path}"],
    "when": "file",
    "extensions": ["mp4", "mkv", "avi", "mov", "mp3", "flac", "wav"]
  }
]
```

---

## 検証 / 情報

```json
[
  {
    "id": "sha256",
    "label": "SHA-256 を表示",
    "exec": "powershell.exe",
    "args": ["-NoExit", "-Command", "Get-FileHash -Algorithm SHA256 -LiteralPath '{path}'"],
    "when": "file"
  }
]
```

- `-NoExit` で結果を表示したまま PowerShell 窓が残る。`-LiteralPath` なので空白や記号を含むファイル名でも安全。

> ファイルの**プロパティダイアログ** (詳細・セキュリティタブ等) はユーザーコマンドでは出せない。
> 行の **Shift+右クリック** (Windows 標準のシェルメニュー) を使う。

---

## おすすめ全部入り (コピペ用)

よく使うものをまとめたサンプル。インストールしていないツールの項目は消すか `exec` のパスを直す。
**この JSON をそのまま `commands.json` に貼って使える** (コメントは含めていない)。

```json
[
  { "id": "vscode-open",     "label": "VSCode で開く",        "exec": "code",          "args": ["{path}"],                          "when": "selection" },
  { "id": "notepad-open",    "label": "メモ帳で開く",          "exec": "notepad.exe",   "args": ["{path}"],                          "when": "file" },
  { "id": "explorer-folder", "label": "エクスプローラで開く",   "exec": "explorer.exe",  "args": ["{path}"],                          "when": "folder" },
  { "id": "explorer-reveal", "label": "場所を表示",            "exec": "explorer.exe",  "args": ["/select,{path}"],                  "when": "file" },
  { "id": "copy-fullpath",   "label": "フルパスをコピー",       "exec": "powershell.exe","args": ["-NoProfile", "-Command", "Set-Clipboard -Value '{path}'"], "when": "selection", "shell": true },
  { "id": "sha256",          "label": "SHA-256 を表示",        "exec": "powershell.exe","args": ["-NoExit", "-Command", "Get-FileHash -Algorithm SHA256 -LiteralPath '{path}'"], "when": "file" },

  { "id": "vscode-here",     "label": "ここを VSCode で開く",   "exec": "code",          "args": ["{cwd}"],                           "when": "background" },
  { "id": "claude-here",     "label": "ここで Claude Code",     "exec": "wt.exe",        "args": ["-d", "{cwd}", "cmd", "/k", "claude"], "when": "background" },
  { "id": "explorer-here",   "label": "エクスプローラで開く",   "exec": "explorer.exe",  "args": ["{cwd}"],                           "when": "background" },
  { "id": "powershell-here", "label": "ここで PowerShell",      "exec": "powershell.exe","args": ["-NoExit"],                         "when": "background" },
  { "id": "terminal-here",   "label": "ここでターミナル",       "exec": "wt.exe",        "args": ["-d", "{cwd}"],                     "when": "background" },

  { "id": "7z-compress",     "label": "7-Zip で圧縮 (.7z)",     "exec": "C:\\Program Files\\7-Zip\\7z.exe", "args": ["a", "{parent}\\{stem}.7z", "{paths}"], "when": "selection" },
  { "id": "7z-extract-here", "label": "ここに展開 (7-Zip)",     "exec": "C:\\Program Files\\7-Zip\\7z.exe", "args": ["x", "{path}", "-o{parent}\\{stem}", "-y"], "when": "file", "extensions": ["zip", "7z", "rar", "tar", "gz", "lzh"] },
  { "id": "7z-compress-drop","label": "ここに 7-Zip で圧縮",    "exec": "C:\\Program Files\\7-Zip\\7z.exe", "args": ["a", "{cwd}\\{stem}.7z", "{paths}"],    "when": "drop" }
]
```

---

## つまずいたら

- 起動の仕組み・トラブルシュート・全フィールドの説明は [COMMANDS.md](./COMMANDS.md) にまとめてある。
- `code` のような `.cmd` 実体のコマンドは、コンソール窓を出さずに起動する (内部で `cmd /c` + 不可視実行)。
- `exec` が見つからないときは自動で `cmd /c` 再試行 → それでもダメなら `"shell": true` を試す。
- パイプやリダイレクト (`|` `>`) を使いたいときは `"shell": true` にして、コマンド全体を 1 つの `exec` 文字列に書く。

## 関連

- リファレンス: [COMMANDS.md](./COMMANDS.md)
- 操作全般: [USAGE.md](./USAGE.md) / キー割り当て: [HOTKEYS.md](./HOTKEYS.md)
