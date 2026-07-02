//! FastFiler iced 版のライブラリ部。
//!
//! bin (main.rs) と examples/ (スパイク・検証アプリ) が共有する部品を置く。
//! Phase 1 以降、widgets/ (FileList ほか) と views/ はここに実装し、
//! examples やベンチから直接 import できるようにする (計画書 §5.1)。

pub mod dev;
