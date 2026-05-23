# FastFiler リリース手順

最終更新: 2026-05-23 / **v0.1.0** リリース用

旧 TARUI 版で持っていた中核機能を floem 単独構成で再現できた段階で、
当面の開発を止め、配布用バイナリをリリースする手順を定めるドキュメント。

実装範囲は [`STATUS.md`](./STATUS.md)、捨てた機能の根拠は
[`adr/`](./adr/) を参照。

---

## 1. リリース対象

| 項目 | 値 |
|---|---|
| バージョン | **0.1.0** (`crates/fastfiler-native/Cargo.toml` の `version`) |
| プラットフォーム | Windows 10 / 11 (x64) のみ |
| 配布形式 | 単一実行ファイル `fastfiler-native.exe` + ドキュメント |
| ランタイム依存 | なし (MSVC ランタイムは静的リンク、WebView2 不要) |

---

## 2. リリース前チェックリスト

### コード品質
- [ ] `cargo fmt --all` (差分なし)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` (0 エラー)
- [ ] `cargo test --workspace` (全 pass)
- [ ] `cargo build -p fastfiler-native --release` (warnings 0 が望ましい)

### 動作確認 (手動)
- [ ] 起動 → ウインドウ表示 / 直前タブ / 直前パスが復元される
- [ ] フォルダ展開速度 (`C:\Windows\System32` を開いて 1 秒以内に表示)
- [ ] タブ分割 / BSP ペイン (横→縦と再帰分割可能)
- [ ] ファイル D&D (内部 / エクスプローラ ⇔ FastFiler 双方向 / 右ボタン D&D メニュー)
- [ ] Undo (`Ctrl+Z` でリネーム / 移動 / ゴミ箱送り を逆実行)
- [ ] 検索 (`Ctrl+F` 内蔵 / Everything 連携 — Everything 1.5 alpha 起動時のみ)
- [ ] 新規ファイル (右クリック「新規ファイル ▸」テンプレ一覧 + 空ファイル + テンプレフォルダを開く)
- [ ] ホットキー変更 + 永続化 (設定 → ホットキー)
- [ ] テーマ即時切替 + アクセントカラー
- [ ] プログレスダイアログ (100 ファイル以上のコピー で右下表示)

### ドキュメント
- [ ] `doc/STATUS.md` の §1 実装済が最新コミットと一致
- [ ] `doc/USAGE.md` の操作説明が現実装と一致
- [ ] `README.md` のバージョン記載 (`v0.1.0`)
- [ ] `LICENSE` が同梱されている

---

## 3. リリースビルド

```powershell
# 念のためクリーン
cargo clean -p fastfiler-native

# リリースビルド (LTO + opt-level="s" + strip 適用)
cargo build -p fastfiler-native --release
```

生成物: `target\release\fastfiler-native.exe`

`Cargo.toml` の `[profile.release]`:
- `lto = true`
- `codegen-units = 1`
- `panic = "abort"`
- `opt-level = "s"` (サイズ優先)
- `strip = true` (シンボル除去)

---

## 4. 配布物の組み立て

リリース用フォルダを作って以下を同梱する。

```
fastfiler-v0.1.0-win-x64/
├── fastfiler-native.exe        ← target\release\fastfiler-native.exe (アイコン埋込済)
├── README.md
├── LICENSE                     ← (リポジトリに追加されたら)
└── doc/
    ├── USAGE.md
    ├── STATUS.md
    └── adr/                    ← ADR 群 (任意)
```

### exe アイコンの差し替え

exe アイコンは `crates\fastfiler-native\assets\icon.ico` を `build.rs` から
`embed-resource` クレート経由で埋め込んでいる。差し替えたい場合:

1. 元画像 (正方形 PNG / 512px 以上推奨) を `crates\fastfiler-native\assets\icon.png` に置く
2. `pwsh scripts\make_icon.ps1` を実行 — マルチサイズ ICO (16/32/48/64/128/256) を再生成
3. `cargo build -p fastfiler-native --release` で埋め込み確認

build.rs は Windows ターゲットでのみ `embed_resource::compile` を呼ぶので、
他プラットフォームのビルドには影響しない。

ZIP 化例:

```powershell
$ver = "0.1.0"
$out = "fastfiler-v$ver-win-x64"
New-Item -ItemType Directory $out | Out-Null
Copy-Item target\release\fastfiler-native.exe $out\
Copy-Item README.md, LICENSE $out\ -ErrorAction SilentlyContinue
Copy-Item doc $out\doc -Recurse
Compress-Archive -Path $out -DestinationPath "$out.zip"
```

---

## 5. GitHub Release への公開 (任意)

```powershell
gh release create v0.1.0 `
  --title "FastFiler v0.1.0" `
  --notes-file doc\RELEASE_NOTES_v0.1.0.md `
  fastfiler-v0.1.0-win-x64.zip
```

リリースノートは `STATUS.md` §1 (実装済) を要約してまとめる。
旧 TARUI 版との対応・移行ガイドが必要なら本ファイルに節を増やす。

---

## 6. リリース後

### バージョン番号の更新

次の開発フェーズに入る前に `Cargo.toml` を `0.1.1-dev` などに上げる。

```toml
# crates/fastfiler-native/Cargo.toml
version = "0.1.1-dev"
```

### 不具合の受付窓口

- ログ: `%APPDATA%\FastFiler\fastfiler.log` (+ `.1` 旧分)
- 設定: `%APPDATA%\FastFiler\settings.ron`
- 報告時はこの 2 ファイル + 操作手順を Issue に添付してもらう

### 既知の制限事項 (v0.1.0)

リリース時点で残っている既知の課題。`STATUS.md` §2 (採用予定) と §4 (細かい課題) も併せて参照。

- ペイン内ツリービュー (`📋 / 🌲` 切替) は未実装
- ドック式パネル配置は `left` / `right` / `hidden` の 3 ヶ所のみ (`top` / `bottom` 未対応)
- パネルプリセット未実装 (ドック 5 ヶ所化が前提)
- Shift+右クリックの `IContextMenu` シェル拡張メニュー未実装 (ADR 0007)
- 仮想スクロールのスクロール量チューニング未完
- Linux / macOS は未サポート (フォント取得・OLE D&D が Windows 専用 API)
