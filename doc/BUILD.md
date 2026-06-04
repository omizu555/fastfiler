# FastFiler ビルド & 開発ガイド (GPUI 版)

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
.\target\release\fastfiler-gpui.exe
```

約 2 分 (LTO + codegen-units=1)。生成物は約 6MB・単体動作・コンソール非表示。

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
- セッションは `%APPDATA%\FastFiler\gpui_session.json`。壊れた場合は削除すれば初期化。
- メモリ健全性はタブバー下部の `live panes: N` で確認できる
  (タブ/ペインを開閉してベースラインへ戻れば OK)。
- GPUI の API を調べるときは `vendor/crates/gpui/examples/` が最短
  (hello_world / uniform_list / input など)。
