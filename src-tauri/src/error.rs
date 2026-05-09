// Phase 2B: 実体は fastfiler-domain crate に移動済。
// 既存コードの `use crate::error::{AppError, AppResult};` を維持するため re-export だけ残す。
pub use fastfiler_domain::error::{AppError, AppResult};
