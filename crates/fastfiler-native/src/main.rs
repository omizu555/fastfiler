// FastFiler — バイナリエントリポイント
//
// 実装は `lib.rs` の [`fastfiler_native::run_app`] にある。
// このファイルは新規ウインドウ起動を呼ぶだけ。

// リリースビルドではコンソールウィンドウを表示しない (GUI subsystem)。
// debug ビルドでは println / panic を確認できるようコンソールを残す。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    fastfiler_native::run_app();
}
