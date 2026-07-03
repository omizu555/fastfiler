//! HWND が必要な Win32 統合 (計画書 §5.1 / §8)。
//!
//! - 純ロジックは fastfiler-core へ、OS 機能そのものは fastfiler-domain へ。
//!   ここには「GUI ウィンドウと Win32 を繋ぐ配線」だけを置く。
//! - GUI 層に windows crate の型を漏らさない (HWND は `isize` で受け渡す流儀)。
//! - HWND の取得は `window::run` + `window_interop::hwnd_from_raw` が正規経路
//!   (`window::raw_id` の u64=HWND は winit 内部表現依存なので恒久コードでは使わない)。

#[cfg(windows)]
pub mod drop_target;
pub mod single_instance;
pub mod window_interop;
