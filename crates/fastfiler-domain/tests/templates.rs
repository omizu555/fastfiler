// Phase 2C-1: templates の新規ファイル生成と重複名処理。
use fastfiler_domain::templates;
use std::fs;
use tempfile::TempDir;

fn s(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

#[test]
fn create_empty_file_creates_file_with_body() {
    let tmp = TempDir::new().unwrap();
    let path =
        templates::create_empty_file(s(tmp.path()), "memo.txt".into(), Some("hi".into())).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"hi");
}

#[test]
fn create_empty_file_avoids_overwrite_with_suffix() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("memo.txt"), b"existing").unwrap();
    let path = templates::create_empty_file(s(tmp.path()), "memo.txt".into(), None).unwrap();
    let p = std::path::Path::new(&path);
    assert_ne!(p.file_name().unwrap(), "memo.txt");
    assert!(p.file_name().unwrap().to_string_lossy().starts_with("memo"));
    assert!(p.exists());
    // 既存ファイルは保持されている
    assert_eq!(fs::read(tmp.path().join("memo.txt")).unwrap(), b"existing");
}

#[test]
fn create_file_from_template_copies_content() {
    let tmp = TempDir::new().unwrap();
    let tpl = tmp.path().join("tpl.txt");
    fs::write(&tpl, b"TEMPLATE BODY").unwrap();
    let dest_dir = tmp.path().join("out");
    fs::create_dir(&dest_dir).unwrap();

    let path = templates::create_file_from_template(s(&tpl), s(&dest_dir), Some("new.txt".into()))
        .unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"TEMPLATE BODY");
}
