fn main() {
    // 診断用ビルドスタンプ (どのビルドの exe が動いているかログで確定するため)
    println!(
        "cargo:rustc-env=FF_BUILD_STAMP={}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );
    // Windows ビルド時のみ exe にアイコンを埋め込む。
    // 他プラットフォームではこの build.rs は何もしない (cfg ガード)。
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rerun-if-changed=assets/icon.rc");
        println!("cargo:rerun-if-changed=assets/icon.ico");
        embed_resource::compile("assets/icon.rc", embed_resource::NONE);
    }
}
