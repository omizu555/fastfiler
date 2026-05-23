//! FastFiler のドメインロジック (GUI 非依存)。
//!
//! 公開モジュール:
//!  - `error` / `events`: 共通エラー型と event sink 抽象
//!  - `fs` / `file_ops` / `file_jobs` / `watcher`: ファイルシステム操作
//!  - `search` / `everything`: 検索 (内蔵 + Everything HTTP)
//!  - `shell` / `shell_assoc` / `win_clipboard` / `icons`: Windows シェル統合
//!  - `templates` / `user_commands`: 新規ファイルテンプレートとユーザーコマンド
//!  - `undo`: in-memory Undo スタック (ADR 0006/0008)
//!  - `ascii_tree`: 選択フォルダの構造を ASCII (ボックス罫線) 文字列化
//!
//! 不採用モジュール (削除済):
//!  - プラグイン機構: ADR 0003
//!  - 内蔵ターミナル: ADR 0004
//!  - サムネイル / プレビュー: ADR 0005

pub mod ascii_tree;
pub mod error;
pub mod events;
pub mod everything;
pub mod file_jobs;
pub mod file_ops;
pub mod fs;
pub mod icons;
pub mod path_util;
pub mod search;
pub mod shell;
pub mod shell_assoc;
pub mod templates;
pub mod undo;
pub mod user_commands;
pub mod watcher;
pub mod win_clipboard;
