use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    tauri_build::build();
    generate_icons_bundle();
}

/// icons-bundle/material/ 以下の SVG/JSON を MATERIAL_BUNDLE 配列として生成する。
/// 同梱ファイルが無くても空配列で動作する (custom パックはユーザー任意配置)。
fn generate_icons_bundle() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let bundle_root = manifest_dir.join("icons-bundle").join("material");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let out_file = out_dir.join("icons_bundle.rs");

    // フォルダ全体を再 cargo:rerun-if-changed しておく
    println!("cargo:rerun-if-changed={}", bundle_root.display());

    let mut files: Vec<(String, PathBuf)> = Vec::new();
    if bundle_root.exists() {
        collect_files(&bundle_root, &bundle_root, &mut files);
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut s = String::new();
    s.push_str("/// build.rs により自動生成 (icons-bundle/material/)\n");
    s.push_str("pub static MATERIAL_BUNDLE: &[(&str, &[u8])] = &[\n");
    for (rel, abs) in &files {
        // include_bytes! はパス内 \\ を許容するが Windows で raw 文字列にする
        let abs_str = abs.to_string_lossy().replace('\\', "/");
        s.push_str(&format!(
            "    ({:?}, include_bytes!(\"{}\")),\n",
            rel, abs_str
        ));
        println!("cargo:rerun-if-changed={}", abs.display());
    }
    s.push_str("];\n");

    fs::write(&out_file, s).expect("write icons_bundle.rs");
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let rd = match fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_dir() {
            collect_files(root, &p, out);
        } else if p.is_file() {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            out.push((rel, p));
        }
    }
}
