# FastFiler Copilot 指示

このリポジトリは Windows 向けの Rust workspace です。`doc/BUILD.md` と `doc/ARCHITECTURE.md` を正として作業してください。

## 作業方針

- **1 セッション 1 論点**。複数の要件を同時に実装しない。混在しそうなら分割を提案して確認する
- 非自明な変更は **Plan モード**（Shift+Tab）で計画を提示し、承認を得てから実装する
- 計画は「調査 → 変更箇所の列挙 → 検証手順」の順で出す
- 仕様が曖昧なら、実装前に確認する
- 既存のモジュール構成と命名を優先し、新しい抽象化は最小限にする
- `experimental/` は workspace 外なので、通常の実装対象から外す
- crate 別の細則は `.github/instructions/` を参照する

## 検証

次の順で確認する。

```powershell
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p fastfiler-native
```

GUI の実行確認が必要な場合でも、まずは build で止める。ヘッドレス環境で `cargo run -p fastfiler-native` を前提にしない。

## floem の注意点

- `dyn_container` の中で `create_effect` を作らない
- リスト系 UI は `dyn_stack` と key 関数を優先する
- effect 内で signal を更新するときは `set_untracked` か、変化時のみ更新する
- `tabs.set` と `active.set` のような連続更新は、再入しない形にする

## 参照

- `doc/BUILD.md`
- `doc/ARCHITECTURE.md`
