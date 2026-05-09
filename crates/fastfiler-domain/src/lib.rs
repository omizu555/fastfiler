//! FastFiler のドメインロジック (Tauri 非依存)。
//!
//! Phase 2B (段階移行中):
//!  - `error`: アプリ共通エラー型
//!  - `events`: 任意の sink にイベントを emit するための抽象
//!  - `fs`: ファイルシステム列挙・stat・ドライブ列挙
//!
//! 残りのモジュール (search / file_jobs / term / ... の純粋部分) は
//! 後続コミットで順次移動する。

pub mod error;
pub mod events;
pub mod fs;
