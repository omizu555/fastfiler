---
applyTo: "crates/fastfiler-native/**/*.rs"
---

# fastfiler-native ルール

floem ベースの GUI。リアクティブ更新まわりに既知のクラッシュパターンが多いので、新規コードでも下記を厳守する。

## floem / RwSignal の鉄則

- `dyn_container` の **中で** `create_effect` を作らない（スコープ寿命が壊れる）
- リスト系 UI は `dyn_stack` + key 関数を使う。`dyn_container` で代用しない
- effect 内で `set` するときは `set_untracked` か「変化時のみ set」で再入を防ぐ
- `tabs.set + active.set` のような連続書込は untracked 比較を挟む
- effect / click ハンドラの中で長寿命 `RwSignal` を生成するときは `Scope::new()` で untether する

## 状態モデル

- `PaneState` は全フィールド `RwSignal` / `Arc`。値渡しできる構造を崩さない
- 起動時の重い初期化は `lib.rs::run_app()` に集約する
- 範囲外インデックスは必ずクランプする（sort / reload 直後の click で過去にクラッシュ）

## ロギング

- 進行ログは `flog!` マクロを使う
- ログは `%APPDATA%\FastFiler\fastfiler.log` に出る前提で書く

## 検証

- `cargo build -p fastfiler-native` まで通す
- ヘッドレスでの実行確認は前提にしない
