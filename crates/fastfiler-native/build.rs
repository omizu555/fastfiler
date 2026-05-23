fn main() {
    // Windows ビルド時のみ exe にアイコンを埋め込む。
    // 他プラットフォームではこの build.rs は何もしない (cfg ガード)。
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rerun-if-changed=assets/icon.rc");
        println!("cargo:rerun-if-changed=assets/icon.ico");
        embed_resource::compile("assets/icon.rc", embed_resource::NONE);
    }
}
