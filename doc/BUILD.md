# FastFiler ビルド & 開発ガイド

floem ベースの純 Rust 版。Node / Tauri / WebView2 は不要。

---

## 1. 必要環境

| 項目 | バージョン | 備考 |
|---|---|---|
| OS | Windows 10 / 11 (x64) | Linux / macOS は WIP (フォント取得・OLE D&D が未対応) |
| Rust | 1.77 以上 | [rustup](https://rustup.rs/) 経由を推奨 |
| MSVC Build Tools | Visual Studio 2022 Build Tools | "C++ build tools" + Windows 10/11 SDK |

`rustup default stable-x86_64-pc-windows-msvc` でツールチェイン設定。

---

## 2. クイックビルド

### 開発ビルド

```powershell
cargo build -p fastfiler-native
.\target\debug\fastfiler-native.exe
```

初回は依存解決で 5〜10 分。2 回目以降はインクリメンタルで数秒。

### リリースビルド

```powershell
cargo build -p fastfiler-native --release
.\target\release\fastfiler-native.exe
```

`Cargo.toml` の `[profile.release]` で LTO / codegen-units=1 / panic=abort / opt-level="s" / strip を有効化済み。
インストーラは付属しない。実行ファイル単体配布。

### ライブラリ単体ビルド (UI なし)

```powershell
cargo build -p fastfiler-domain
```

ファイル操作・検索・watcher 等のドメインロジックだけを Tauri/floem 抜きで使いたい場合用。

---

## 3. 開発サイクル

### 整形 / Lint / テスト

```powershell
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

ワークスペース全体に適用される。CI では `clippy -D warnings` を通過させる前提。

### 実行と検証

```powershell
cargo run -p fastfiler-native
```

ログは `%APPDATA%\FastFiler\fastfiler.log` (前回分は `.1`) にローテートされる。
ターミナルにも進行ログが出る。

### ドメインクレートのスモーク

```powershell
cargo run -p fastfiler-domain --example trash_test -- "C:\path\to\file"
```

`SHFileOperationW` 経由のごみ箱送りを単発検証する。

### メモリ調査ビルド (mem-debug)

メモリ使用量の増加源を計測したいときは `mem-debug` feature を有効化する。
通常ビルドには一切影響しない (OFF 時はゼロコスト)。

```powershell
cargo run -p fastfiler-native --release --features mem-debug
```

有効化すると以下が動く。

- **counting global allocator**: ヒープ実確保量 (`heap_cur` / `heap_peak`) を計測。
- **live インスタンスカウンタ**: `PaneState` / `Tab` の論理 live 数を計数。
- **2 秒周期のスナップショットログ** を `fastfiler.log` に出力 (タブ close 時にも `after-close-*`)。

ログ例:

```
[mem] tick panes=3 tabs=2 heap_cur=42.1MB heap_peak=55.0MB ws=120.3MB priv=140.2MB
```

読み方:

| 観測 | 解釈 |
|---|---|
| タブ/ペインを閉じると `panes` / `tabs` がベースラインへ戻る、かつ `heap_cur` が平坦で `ws` だけ高い | ロジックリークではない。アロケータ / wgpu が解放済みメモリを OS に返さないだけ (GPU GUI では普通) |
| 開閉を繰り返すと `panes` / `tabs` が増え続ける | 真のリーク。scope dispose か view teardown が不発 → 要修正 |
| `heap_cur` が増え続ける | ヒープリーク確定 |

`ws` (WorkingSet) はタスクマネージャの表示値に対応。`heap_cur` との差が
「OS に返らない保持分」の目安になる。

### ヒーププロファイル調査ビルド (dhat-heap)

`heap_cur` が増え続ける真のヒープリークの**発生箇所 (コールスタック)** を特定したい
ときは `dhat-heap` feature を使う。アロケーションのコールスタックを記録し、
終了時に `dhat-heap.json` を出力する。`mem-debug` とは排他。

シンボルを残すため `strip` / `debug` を上書きしてビルドする (リポジトリ直下で実行)。

```powershell
cargo build -p fastfiler-native --release --features dhat-heap `
  --config "profile.release.strip=false" --config "profile.release.debug=1"

# dhat-heap.json をリポジトリ直下に出すため、カレントを揃えて実行する
.\target\release\fastfiler-native.exe
```

計測手順:

1. 起動後、**タブの作成と削除を何度か繰り返す** (リークを再現させる)。
2. **ウィンドウを通常どおり閉じる** (×ボタン)。event loop が正常終了し、
   `dhat::Profiler` の Drop で `dhat-heap.json` が書き出される。
   - プロセス強制終了 (タスクマネージャkill) では出力されないので注意。
3. 生成された `dhat-heap.json` を解析する。

解析方法:

- ブラウザで [dhat viewer](https://nnethercote.github.io/dh_view/dh_view.html) を開き
  `dhat-heap.json` を読み込む。**"At end" (t-end) のバイト数が大きいフレーム**が
  終了時点でも解放されていない＝リーク源。
- "Sort metric" を *Bytes at t-gmax* / *Bytes at t-end* に切り替えて、
  リーク量の多いコールスタックを上から確認する。

---

## 4. 永続化と設定

| 種類 | 保存先 |
|---|---|
| アプリ設定 (テーマ・フォント・ホットキー等) | `%APPDATA%\FastFiler\settings.ron` |
| 動作ログ | `%APPDATA%\FastFiler\fastfiler.log` |

`settings.ron` は人間可読 RON 形式。手動編集後の起動でロードされる (パース失敗時は既定値にフォールバック)。
完全リセットしたい場合は `%APPDATA%\FastFiler\` フォルダごと削除。

---

## 5. トラブルシューティング

| 症状 | 対処 |
|---|---|
| `linker 'link.exe' not found` | MSVC Build Tools が未導入。Visual Studio Installer から「C++ build tools」を入れる |
| 起動直後に終了する | `%APPDATA%\FastFiler\fastfiler.log` を確認。`settings.ron` のパース失敗が原因なら同フォルダを削除 |
| ウインドウが画面外に出る | `settings.ron` の `window_x` / `window_y` を編集 or 削除 |
| 0xc0000005 (STATUS_ACCESS_VIOLATION) | 過去事例: クリック直後の selection クランプ漏れ。再現したらログ + 操作手順を Issue へ |
| Everything 検索結果が出ない | Everything 1.5 alpha 以降の HTTP Server が起動 + ポート (既定 80 → 設定で変更可) と一致しているか |
| フォント一覧が空 | Windows のレジストリ `HKLM/HKCU\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts` を読めるか確認 |

---

## 6. 配布

`target\release\fastfiler-native.exe` を単体コピーで動作する。
WebView2 / VC++ ランタイムへの依存は無し (MSVC は静的リンクされている)。
ZIP 化して配布する場合は `LICENSE` / `README.md` も同梱推奨。
