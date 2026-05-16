---
applyTo: "crates/fastfiler-domain/**/*.rs"
---

# fastfiler-domain ルール

OS / GUI 非依存のロジック層。`fastfiler-native` から再利用される前提なので、ここで GUI 依存を持ち込まない。

## 設計

- エラーは `crate::error::AppError` に集約する。`anyhow` を新規導入しない
- 公開 API は `fastfiler-native` から呼ばれる。安易にシグネチャを変えない
- イベント通知は `EventSink` trait を介す。テストでは `NullSink` を使う

## Windows API

- 削除は `SHFileOperationW` を使う。`IFileOperation` (COM) は SEH 例外で落ちた実績があるので使わない
- COM 呼び出しは `CoInitializeEx` の有無を毎回確認する

## テスト

- 新規・変更ロジックには `crates/fastfiler-domain/tests/` にユニットテストを足す
- 検証は最低限 `cargo test -p fastfiler-domain` を通す
