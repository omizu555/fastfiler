//! コア状態 / 値型 / アクションを集約したモジュール。
//!
//! - [`state`]: AppState / Tab / PaneState / SplitNode 等の中核データ構造
//! - [`fs_model`]: FileRow / Stats / 書式整形等の値型と純粋関数
//! - [`actions`]: AppState への delete/paste/copy/rename 等の機能
//!
//! 以前は `crate::state` 等で直接アクセスしていた。後方互換のため `main.rs`
//! 側で `use core::{state, actions, fs_model};` を入れて、既存パスも引き続き使えるようにしている。

pub mod actions;
pub mod debug_mem;
pub mod fs_model;
pub mod jobs;
pub mod perf;
pub mod state;
pub mod tree_model;
