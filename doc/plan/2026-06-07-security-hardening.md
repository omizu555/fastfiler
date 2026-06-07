# 2026-06-07 セキュリティ調査と修正

ユーザー要望によるセキュリティ/脆弱性調査 (3 領域: プロセス起動 / 入力パース・
ネットワーク / unsafe・Win32) と、その修正記録。

## 調査の総評

設計は概ね堅実。**ユーザーコマンド (commands.json) 自体は自己責任モデルで妥当**だが、
**ファイル名 (攻撃者が制御可能) が cmd シェル文字列へ流れ込む** 1 点が本物の穴だった。
SSRF・JSON パース・session 復元・shell_assoc・クリップボード読取などは安全と確認。

## 修正済み

### 🔴 コマンドインジェクション (user_commands.rs)
- **問題**: `{path}` 等で展開された生のファイル名が `cmd /c "code <path>"` に
  不十分なクオートで渡り、`x&calc.exe` のような名前で `&` がコマンド区切りとして
  解釈され任意コード実行。攻撃者が握るのは**ファイル名のみ**で、被害者は同梱サンプルの
  「VSCode で開く」を実行するだけで踏む。
- **修正**:
  - `cmd_quote()` — 各トークンを**常時**ダブルクオート (引用符内では cmd は
    `& | < > ^ ( )` をリテラル扱い)。
  - `build_shell_command` — `/c` の引用符剥がし規則に合わせ行全体を 1 組の `"` で
    囲み、`raw_arg` で Rust の再クオート (cmd 非対応) を回避。
  - ShellExecuteW の params も `cmd_quote` 化 (標的が .cmd/.bat だと ShellExecuteW
    経由でも cmd が再解釈するため = BatBadBut クラス対策)。
- **テスト**: `cmd_quote` 単体 / payload 構造 / **.bat を標的にした end-to-end**
  (注入が起きない & リテラル引数がツールへ正しく届く) の 3 本。

### 🟠 cwd バイナリプランティング (user_commands.rs)
- **問題**: `exec` がベア名 (`code` 等) + cwd=閲覧中フォルダ → 検索順序 (cwd 含む)
  で悪意ある `code.exe` が実行されうる。
- **修正**: `resolve_in_path()` — ベア名を PATH (**空エントリ=cwd は除外**) + PATHEXT で
  絶対パスに解決してから起動。見つからなければ従来どおり (退行なし)。
  - 残課題: 正規 exe が cwd から DLL を読み込む DLL プランティングまでは未対策
    (`SetDllDirectory` 等が必要)。主要ベクタ (任意 exe 実行) は解消。

### 🟡 小さめの堅牢化
- `everything.rs`: 応答件数を `max_results` で `truncate` (信頼できないローカル
  HTTP サーバの大量ヒットによる UI/メモリフラッディング DoS を防止)。
- `ole_dnd.rs::read_hglobal_dword`: `GlobalSize >= 4` ガード追加 (規約違反の
  ドロップ先による 4 バイト範囲外読み取りを防止)。
- `theme.rs::load_user_themes`: テーマファイル数 (256) と 1 ファイルサイズ (256KiB)
  に上限 (大量/巨大 JSON + `Box::leak` によるメモリ DoS を防止)。

## 未対応 (低優先 / 設計上許容)

- `ole_dnd.rs` の IDropTarget 受信側 (`extract_hdrop_paths` 他) に範囲外読み取りの
  穴があるが、**GPUI 版では完全に未使用のデッドコード** (gpui の `ExternalPaths` で
  受信)。将来有効化するなら `GlobalSize` ベースの境界チェックが必要、または削除推奨。
- `win_clipboard.rs` 書き込みエラー経路の HGLOBAL リーク (コメントで許容済み・UBではない)。
- `file_ops.rs` / `shell.rs` の `CoInitializeEx` HRESULT を見ずに `CoUninitialize`
  する箇所 (専用フレッシュスレッドで実行のため今日は発火しないが、イディオムとしては要改善)。
- `quote_if_needed` の末尾バックスラッシュによる引数分割 (ShellExecuteW 経路) は
  `cmd_quote` 化で実質緩和済み (常時クオートのため)。

## 安全と確認済み (変更不要)

Everything SSRF (ホスト固定) / 検索クエリの URL エンコード / 設定・セッション・
ホットキー JSON のパース耐性 / session 復元時のパス検証 / theme の AtomicPtr+Box::leak
の健全性 / shell_assoc (HKCU 限定・固定 ProgID) / IContextMenu / icons / win_clipboard 読取。

> 注記 (2026-06-07): この修正・文書はローカルコミットが GitHub 再クローンで失われた後、
> チャット履歴から再構築したもの。以後 push 漏れに注意。
