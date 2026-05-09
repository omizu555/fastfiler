# FastFiler

縦タブ (列数指定可) + 任意分割ペイン を備えた **Windows 向け高速ファイラ**。
floem (Rust 純) ベース。Windows エクスプローラの遅さに耐えられず AI に作ってもらいました。

> 旧 Tauri 2 + Solid.js 実装は 2026-05 のリファクタで全て削除し、現在は floem 単独構成です。

## ドキュメント

| ドキュメント | 内容 |
|---|---|
| [doc/STATUS.md](./doc/STATUS.md) | 実装ステータス (実装済 / 未実装) |
| [doc/ARCHITECTURE.md](./doc/ARCHITECTURE.md) | クレート構成 / 状態モデル / 拡張ポイント |
| [doc/USAGE.md](./doc/USAGE.md) | 使い方ガイド (操作 / ホットキー) |
| [doc/BUILD.md](./doc/BUILD.md) | ビルド & インストール |

## ディレクトリ構成

```
fastfiler/
├ Cargo.toml             # workspace
├ crates/
│  ├ fastfiler-domain/   # OS/ファイル操作ライブラリ
│  └ fastfiler-native/   # floem GUI バイナリ
├ doc/                   # ドキュメント
│  └ plugins-sample/     # サンプル (旧プラグイン仕様)
└ experimental/          # POC (floem 検証用)
```

## ビルドと起動

```
cargo build -p fastfiler-native --release
.\target\release\fastfiler-native.exe
```

詳細は [doc/BUILD.md](./doc/BUILD.md) を参照。
