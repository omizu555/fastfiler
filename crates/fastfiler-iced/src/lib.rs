//! FastFiler iced 版のライブラリ部。
//!
//! bin (main.rs) と examples/ (スパイク・検証アプリ) が共有する部品を置く。
//! - `widgets/` — 自前ウィジェット (FileList ほか)
//! - `app` — アプリの boot/update/view/subscription (main.rs は起動だけ)
//! - `effects` — core の Effect → iced Task / domain 呼び出し
//! - `dev` — 開発・検証用ユーティリティ

pub mod app;
pub mod dev;
pub mod effects;
pub mod hotkeys;
pub mod settings;
pub mod settings_view;
pub mod theme;
pub mod widgets;
