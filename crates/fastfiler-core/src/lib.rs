//! FastFiler のフレームワーク非依存コア。
//!
//! 設計原則 (doc/plan/2026-07-02-iced-rewrite.md §5):
//! - 状態 (`PaneState` / 将来の `AppModel`) と純ロジック (`update`) はここに置き、
//!   GUI フレームワークの型を一切参照しない。domain (GPL) にも依存しない。
//! - `update` は I/O をしない。副作用は `Effect` として返し、GUI 層が実行する。
//! - ここに置いたものには単体テストを書く。

pub mod domain_event;
pub mod effect;
pub mod format;
pub mod model;
pub mod msg;
pub mod selection;
pub mod transfer;
pub mod update;

pub use domain_event::DomainEvent;
pub use effect::Effect;
pub use model::{Column, Entry, Overlay, PaneId, PaneState, SortState};
pub use msg::PaneMsg;
pub use selection::NavKey;
