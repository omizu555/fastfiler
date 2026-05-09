# FastFiler ビルド & インストール手順

floem (純 Rust) 版。Node / Tauri は不要になりました。

## 必要環境

- Windows 10/11
- Rust 1.77+ (rustup 経由を推奨)
- Visual Studio 2022 Build Tools (MSVC) — windows-rs / floem の依存

## 開発ビルド (ホットリロードはなし)

```powershell
cargo build -p fastfiler-native
.\target\debug\fastfiler-native.exe
```

初回は依存解決で 5〜10 分。2 回目以降はインクリメンタル。

## リリースビルド

```powershell
cargo build -p fastfiler-native --release
.\target\release\fastfiler-native.exe
```

Cargo.toml の [profile.release] で LTO / codegen-units=1 / panic=abort / opt-level="s" / strip を有効化済。
インストーラは付属しません。バイナリ単体配布です。

## domain クレート単体検証

```powershell
cargo run -p fastfiler-domain --example trash_test -- "C:\path\to\file"
```

ごみ箱送り (SHFileOperationW) のスモークテスト用。

## 設定ファイル

%APPDATA%\fastfiler\settings.ron に永続化。手動編集も可。
