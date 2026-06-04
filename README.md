# FastFiler

縦タブ + 任意分割ペイン を備えた **Windows 向け高速ファイラ**。
GUI は **GPUI** (Zed のフレームワークを `vendor/` に完全移植・自己完結) ベース。
Windows エクスプローラの遅さに耐えられず AI に作ってもらいました。

> - 旧 Tauri 2 + Solid.js 実装は 2026-05 のリファクタで削除。
> - 旧 floem 実装はタブ/ペイン開閉でメモリが増殖する構造問題があり、
>   2026-06 に GPUI へ全面移植した ([ADR 0012](./doc/adr/0012-migrate-floem-to-gpui.md))。
>   floem 版 (`crates/fastfiler-native`) はパリティ最終確認まで残置。

## 中核アイデンティティ

FastFiler が何を目指し、何を**捨てている**かは [`CONTEXT.md`](./CONTEXT.md) に
まとめてあります。要点だけ:

1. **縦タブ + 任意分割ペイン** (最重要 — 大量フォルダの同時操作)
2. **速度** (System32 のような数万件フォルダでも瞬時)
3. **Windows との深い統合** (シェル / OLE D&D / 既定ハンドラ)
4. **拡張性** (ユーザーコマンド + ホットキー + テーマの 3 軸に限定)

写真整理用ファイラやプラグイン基盤ではありません。
何を持たないかの根拠は [`doc/adr/`](./doc/adr/) を参照してください。

## ドキュメント

| ドキュメント | 内容 |
|---|---|
| [CONTEXT.md](./CONTEXT.md) | 中核アイデンティティと用語の定義 |
| [doc/adr/](./doc/adr/) | アーキテクチャ意思決定記録 (ADR) — 何を捨てたか・なぜか |
| [doc/plan-2026-06-03-gpui-migration.md](./doc/plan-2026-06-03-gpui-migration.md) | GPUI 移植計画と進捗ログ (§11) |
| [vendor/README.md](./vendor/README.md) | GPUI vendor の構成・改変点・更新手順 |
| [doc/STATUS.md](./doc/STATUS.md) | (floem 版時点の) 実装ステータス |
| [doc/ARCHITECTURE.md](./doc/ARCHITECTURE.md) | (floem 版時点の) クレート構成 |
| [doc/USAGE.md](./doc/USAGE.md) | (floem 版時点の) 使い方ガイド |

## ディレクトリ構成

```
fastfiler/
├ Cargo.toml             # workspace
├ CONTEXT.md             # 中核アイデンティティ + 用語
├ rust-toolchain.toml    # 1.95.0 (GPUI 要求)
├ crates/
│  ├ fastfiler-domain/   # OS/ファイル操作ライブラリ (GUI 非依存・GPUI 版でも全面再利用)
│  ├ fastfiler-gpui/     # GPUI GUI バイナリ (現行)
│  └ fastfiler-native/   # 旧 floem GUI バイナリ (残置・削除予定)
├ vendor/                # GPUI とその依存 18 クレート (zed から完全移植・自己完結)
├ doc/                   # ドキュメント
│  └ adr/                # ADR (意思決定記録)
└ experimental/          # POC
```

## ビルドと起動

```powershell
cargo build -p fastfiler-gpui --release
.\target\release\fastfiler-gpui.exe
```

開発時は `cargo run -p fastfiler-gpui`。

## GPUI 版の主な操作

| 操作 | キー / マウス |
|---|---|
| タブ追加 / 切替 / 閉じる | ＋ボタン / クリック・Ctrl(+Shift)+Tab / × |
| ペイン分割 / 閉じる | ペイン右上 ↔ ↕ × |
| ペイン境界 / ツリー幅 | ドラッグでリサイズ |
| フォーカスペイン巡回 | F6 (青枠が宛先) |
| 選択 | クリック / Ctrl+クリック / Shift+クリック / Shift+矢印 / Ctrl+A |
| 開く / 親へ / 更新 | Enter・ダブルクリック / Backspace / F5 |
| リネーム / 新フォルダ / 新ファイル | F2 / F7 / F8 (IME 対応入力) |
| コピー / 切り取り / 貼り付け | Ctrl+C / X / V (エクスプローラ相互運用) |
| 削除 (ごみ箱) | Delete |
| 右クリック | コンテキストメニュー (行 / 背景) |
| D&D | ペイン間 / エクスプローラ→FastFiler (同一ボリューム=移動) |
| ワークスペースツリー | 「ツリー」ボタンで表示切替、クリックでフォーカスペインに開く |

タブ / 分割構成 / ウィンドウ位置はセッション保存され、次回起動時に復元されます
(`%APPDATA%\FastFiler\gpui_session.json`)。

## バージョン

- **GPUI 版**: floem 版 v0.1.0 の中核機能パリティ + メモリ問題の構造的解決
  (タブ/ペイン開閉で `live panes` がベースラインへ戻ることを実機確認可能)。
- 旧 floem 版: v0.1.0 (リリース候補のまま凍結)。
