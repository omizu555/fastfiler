# FastFiler

縦タブ (列数指定可) + 任意分割ペイン を備えた **Windows 向け高速ファイラ**。
floem (Rust 純) ベース。Windows エクスプローラの遅さに耐えられず AI に作ってもらいました。

> 旧 Tauri 2 + Solid.js 実装は 2026-05 のリファクタで全て削除し、現在は floem 単独構成です。

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
| [doc/STATUS.md](./doc/STATUS.md) | 実装ステータス (実装済 / 採用予定 / 不採用) |
| [doc/ARCHITECTURE.md](./doc/ARCHITECTURE.md) | クレート構成 / 状態モデル / 拡張ポイント |
| [doc/USAGE.md](./doc/USAGE.md) | 使い方ガイド (操作 / ホットキー) |
| [doc/BUILD.md](./doc/BUILD.md) | ビルド & インストール |
| [doc/RELEASE.md](./doc/RELEASE.md) | リリース手順とチェックリスト |
| [doc/IDEAS.md](./doc/IDEAS.md) | 機能アイデアの採否台帳 |

## ディレクトリ構成

```
fastfiler/
├ Cargo.toml             # workspace
├ CONTEXT.md             # 中核アイデンティティ + 用語
├ crates/
│  ├ fastfiler-domain/   # OS/ファイル操作ライブラリ
│  └ fastfiler-native/   # floem GUI バイナリ
├ doc/                   # ドキュメント
│  └ adr/                # ADR (意思決定記録)
└ experimental/          # POC (floem 検証用)
```

## ビルドと起動

```powershell
cargo build -p fastfiler-native --release
.\target\release\fastfiler-native.exe
```

詳細は [doc/BUILD.md](./doc/BUILD.md) を参照。
配布用のリリースビルド手順は [doc/RELEASE.md](./doc/RELEASE.md) を参照。

## バージョン

現在 **v0.1.0** (リリース候補)。
旧 TARUI (Tauri 2 + Solid.js) 版の中核機能を floem 単独構成で再現したマイルストーン。
実装範囲は [doc/STATUS.md](./doc/STATUS.md) を、配布手順は
[doc/RELEASE.md](./doc/RELEASE.md) を参照。
