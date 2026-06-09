# FastFiler ビルド & 開発 & リリースガイド (GPUI 版)

GPUI (Zed のフレームワークを `vendor/` に完全移植) ベースの純 Rust 構成。
Node / Tauri / WebView2 は不要。zed リポジトリのチェックアウトも**不要** (自己完結)。

---

## 1. 必要環境

| 項目 | バージョン | 備考 |
|---|---|---|
| OS | Windows 10 / 11 (x64) | GPUI は他 OS も対応するが本リポジトリの vendor は Windows 専用に整理済み |
| Rust | **1.95.0** (固定) | `rust-toolchain.toml` で固定。rustup が自動取得 |
| MSVC Build Tools | Visual Studio 2022 Build Tools | "C++ build tools" + Windows 10/11 SDK |

補足:
- `.cargo/config.toml` で `check-revoke = false` を設定済み
  (証明書失効サーバへ到達できないネットワークでの取得失敗
  `CRYPT_E_NO_REVOCATION_CHECK` 回避。不要な環境では削除可)。
- 唯一の git 依存は `smol-rs/async-task` (workspace の `[patch.crates-io]`)。
  初回ビルドのみネットワークが必要。

---

## 2. クイックビルド

### 開発ビルド

```powershell
cargo run -p fastfiler-gpui
```

初回はフルビルドで数分。以降のインクリメンタルは数秒。

### リリースビルド

```powershell
cargo build -p fastfiler-gpui --release
.\target\release\fastfiler.exe
```

約 2 分 (LTO + codegen-units=1)。生成物は約 6MB・単体動作・コンソール非表示。

### クリーンリビルド

依存やビルドキャッシュ起因の問題を疑うとき:

```powershell
cargo clean
cargo build -p fastfiler-gpui --release
```

vendor (GPUI) を含むフルビルドになるため数分かかる。

---

## 3. ワークスペース構成

```
crates/fastfiler-domain   OS/ファイル操作ライブラリ (GUI 非依存)
crates/fastfiler-gpui     GUI バイナリ (これをビルドする)
vendor/                   GPUI と依存クレート (独立サブワークスペース・触る必要なし)
```

- `vendor/` の更新 (GPUI のバージョンアップ) 手順は [`../vendor/README.md`](../vendor/README.md)。
- アーキテクチャの詳細は [`ARCHITECTURE.md`](./ARCHITECTURE.md)。

---

## 4. 開発メモ

- デバッグ起動でコンソールにパニックが出る (`RUST_BACKTRACE=1` 推奨)。
- ユーザーデータは `%APPDATA%\FastFiler\` 配下:
  - `gpui_session.json` — セッション (タブ / 分割 / 列幅 / ロック / ウィンドウ位置)
  - `gpui_settings.json` — 設定 (テーマ / スタイル / フォント / タブ列数 / Everything ポート 等)
  - `gpui_hotkeys.json` — ホットキー割り当て
  - `themes/*.json` — カスタムテーマ (初回起動でサンプル生成)
  - 壊れた場合は該当ファイルを削除すれば既定値で再生成される。
- メモリ健全性の計装 `PANES_ALIVE` (pane.rs) は残しているが、常時表示の
  `live panes: N` は通常利用の邪魔になるため UI からは撤去した (2026-06-09)。
  リーク調査時はタブバー下部に一時的に表示を足すか `PANES_ALIVE` をログ出力する。
- GPUI の API を調べるときは `vendor/crates/gpui/examples/` が最短
  (hello_world / uniform_list / input など)。

---

## 5. リリース手順 (ZIP 配布)

| 項目 | 値 |
|---|---|
| バージョン | `crates/fastfiler-gpui/Cargo.toml` の `version` |
| プラットフォーム | Windows 10 / 11 (x64) のみ |
| 配布形式 | 単一実行ファイル `fastfiler.exe` + ドキュメント (ZIP) |
| ランタイム依存 | なし (WebView2 不要) |

### リリース前チェックリスト

コード品質:

- [ ] `cargo fmt --all` (差分なし)
- [ ] `cargo clippy -p fastfiler-gpui -p fastfiler-domain -- -D warnings`
- [ ] `cargo build -p fastfiler-gpui --release` (warnings 0)

動作確認 (手動):

- [ ] 起動 → 前回セッション (タブ / 分割 / ウィンドウ位置) が復元される
- [ ] フォルダ展開速度 (`C:\Windows\System32` を開いて即時表示)
- [ ] タブ追加/閉じ・ペイン分割/閉じ → メモリがベースラインへ戻る
      (調査時は `PANES_ALIVE` を一時表示してベースライン復帰を確認)
- [ ] D&D (ペイン間 / エクスプローラ相互 / 右ボタンドラッグのメニュー)
- [ ] コピー / 切り取り / 貼り付け (エクスプローラ相互) + 進捗表示
- [ ] リネーム / 新規フォルダ / 新規ファイル (日本語 IME 入力)
- [ ] ごみ箱削除 (複数選択) と Ctrl+Z での取り消し
- [ ] ワークスペースツリー (展開 / フォーカスペインに開く / 幅変更)
- [ ] 右クリックメニュー (行 / 背景 / Shift+右クリックのシェルメニュー)
- [ ] 検索 (Ctrl+F、Everything 起動中なら連携)
- [ ] 設定画面 (テーマ / スタイル / フォントの切替が即反映)

### ビルドと梱包

```powershell
cargo build -p fastfiler-gpui --release

# ZIP 構成
fastfiler-<version>-win-x64/
├ fastfiler.exe
├ README.md
└ doc/USAGE.md
```

```powershell
$v = "x.y.z"
$dir = "fastfiler-$v-win-x64"
mkdir $dir; mkdir $dir\doc
copy target\release\fastfiler.exe $dir\
copy README.md $dir\
copy doc\USAGE.md $dir\doc\
Compress-Archive -Path $dir -DestinationPath "$dir.zip"
```

### タグ付け

```powershell
git tag -a gpui-vX.Y.Z -m "FastFiler GPUI vX.Y.Z"
git push origin gpui-vX.Y.Z   # 任意
```

### 既知の制限 (リリースノートに記載)

[`README.md`](./README.md) (doc 案内) の「未実装 / 残タスク」を転記する。
