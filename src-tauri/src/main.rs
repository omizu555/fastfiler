// FastFiler 起動エントリ
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    fastfiler_lib::run();
}
