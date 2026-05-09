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
