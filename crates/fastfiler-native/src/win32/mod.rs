//! Win32 専用機能 (ADR 0011 など)。
//!
//! 現状は右ボタン D&D の `WM_RBUTTONUP` フックのみ。

#![cfg(windows)]

pub mod right_drag_hook;
pub mod single_instance;
