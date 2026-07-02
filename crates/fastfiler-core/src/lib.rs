//! FastFiler のフレームワーク非依存コア。
//!
//! 設計原則 (doc/plan/2026-07-02-iced-rewrite.md §5):
//! - 状態 (`AppModel`) と純ロジック (`update`) はここに置き、GUI フレームワークの型を
//!   一切参照しない。
//! - `update` は I/O をしない。副作用は `Effect` として返し、GUI 層が実行する。
//! - ここに置いたものには単体テストを書く。
//!
//! Phase 1 で model / msg / update / effect / selection / history を実装する。

pub mod model;

pub use model::PaneId;
