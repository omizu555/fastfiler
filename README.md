# FastFiler

縦タブ + 任意分割ペイン を備えた **Windows 向け高速ファイラ**。
GUI は **GPUI** (Zed のフレームワークを `vendor/` に完全移植・自己完結) ベース。
Windows エクスプローラの遅さに耐えられず AI に作ってもらいました。

> - 旧 Tauri 2 + Solid.js 実装は 2026-05 のリファクタで削除。
> - 旧 floem 実装はタブ/ペイン開閉でメモリが増殖する構造問題があり、
>   2026-06 に GPUI へ全面移植した ([ADR 0012](./doc/adr/0012-migrate-floem-to-gpui.md))。
>   floem 版は削除済み (git 履歴 `wip(floem): メモリ増殖調査の計装を保全` 以前を参照)。

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
| [doc/README.md](./doc/README.md) | **doc フォルダの案内** + 実装状況サマリ (まずここ) |
| [doc/USAGE.md](./doc/USAGE.md) | 使い方ガイド (操作リファレンス) |
| [doc/THEMES.md](./doc/THEMES.md) | テーマのカスタマイズ (JSON / 全色キーの説明) |
| [doc/COMMANDS.md](./doc/COMMANDS.md) | ユーザーコマンド (commands.json の書き方) |
| [doc/HOTKEYS.md](./doc/HOTKEYS.md) | ホットキーのカスタマイズ (全アクションの説明) |
| [doc/ARCHITECTURE.md](./doc/ARCHITECTURE.md) | クレート構成 / 状態モデル / 拡張ポイント |
| [doc/BUILD.md](./doc/BUILD.md) | ビルド & 開発 & リリース手順 |
| [doc/IDEAS.md](./doc/IDEAS.md) | 機能アイデアの採否台帳 |
| [doc/adr/](./doc/adr/) | アーキテクチャ意思決定記録 (ADR) — 何を捨てたか・なぜか |
| [doc/plan/](./doc/plan/) | 実装計画 (日付付き、作業ログ含む) |
| [vendor/README.md](./vendor/README.md) | GPUI vendor の構成・改変点・更新手順 |

## ディレクトリ構成

```
fastfiler/
├ Cargo.toml             # workspace
├ CONTEXT.md             # 中核アイデンティティ + 用語
├ rust-toolchain.toml    # 1.95.0 (GPUI 要求)
├ crates/
│  ├ fastfiler-domain/   # OS/ファイル操作ライブラリ (GUI 非依存)
│  └ fastfiler-gpui/     # GPUI GUI バイナリ
├ vendor/                # GPUI とその依存 18 クレート (zed から完全移植・自己完結)
└ doc/                   # ドキュメント (案内は doc/README.md)
   ├ adr/                # ADR (意思決定記録)
   └ plan/               # 実装計画 (日付付き md)
```

## ビルドと起動

```powershell
cargo build -p fastfiler-gpui --release
.\target\release\fastfiler.exe
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
| D&D | ペイン間 / エクスプローラと相互 (右ボタンドラッグのメニュー対応) |
| 検索 / 元に戻す | Ctrl+F (Everything 連携) / Ctrl+Z (リネーム・ごみ箱送り) |
| ワークスペースツリー | 「ツリー」ボタンで表示切替、クリックでフォーカスペインに開く |

タブ / 分割構成 / ウィンドウ位置はセッション保存され、次回起動時に復元されます
(`%APPDATA%\FastFiler\gpui_session.json`)。

## バージョン

- **GPUI 版**: floem 版 v0.1.0 の中核機能パリティ + メモリ問題の構造的解決
  (タブ/ペイン開閉で `live panes` がベースラインへ戻ることを実機確認可能)。
- 旧 floem 版: v0.1.0 で凍結 → 削除済み (git 履歴参照)。
