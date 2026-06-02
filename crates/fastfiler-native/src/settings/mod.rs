//! 設定モジュール: ランタイム値 (`AppSettings`) と永続化 (`PersistedSettings`)、
//! および設定ダイアログ UI を機能別サブモジュールに分割している。
//!
//! - [`model`]    : `AppSettings` (各フィールドは `RwSignal`)、`from_persisted`、`save`
//! - [`persisted`]: `PersistedSettings` (serde mirror)、ロード/保存パス
//! - [`widgets`]  : 設定ダイアログ用の共通フォーム部品 (row_input/check/select/font)
//! - [`dialog`]   : 設定ダイアログ本体 (`settings_view`) と各タブ
//!
//! 公開 API はこのモジュールから re-export する。

mod dialog;
pub mod model;
pub mod persisted;
mod widgets;

pub use dialog::settings_view;
pub use model::{AppSettings, IconSet};
pub use persisted::PersistedSettings;
