use fastfiler_domain::file_ops;
use std::fs;
use tempfile::TempDir;

#[test]
fn create_dir_creates_directory() {
    let tmp = TempDir::new().unwrap();
    let target = tmp.path().join("new");
    file_ops::create_dir(&target).unwrap();
    assert!(target.is_dir());
}

#[test]
fn create_dir_recursive_for_nested() {
    let tmp = TempDir::new().unwrap();
    let nested = tmp.path().join("a").join("b").join("c");
    file_ops::create_dir(&nested).unwrap();
    assert!(nested.is_dir());
}

#[test]
fn rename_file_renames() {
    let tmp = TempDir::new().unwrap();
    let from = tmp.path().join("a.txt");
    let to = tmp.path().join("b.txt");
    fs::write(&from, b"hello").unwrap();
    file_ops::rename_path(&from, &to).unwrap();
    assert!(!from.exists());
    assert_eq!(fs::read(&to).unwrap(), b"hello");
}

#[test]
fn copy_file_copies_content() {
    let tmp = TempDir::new().unwrap();
    let from = tmp.path().join("src.txt");
    let to = tmp.path().join("dst.txt");
    fs::write(&from, b"copy me").unwrap();
    file_ops::copy_path(&from, &to).unwrap();
    assert_eq!(fs::read(&to).unwrap(), b"copy me");
    assert!(from.exists(), "copy must keep source");
}

#[test]
fn copy_dir_copies_recursively() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("src");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("a.txt"), b"a").unwrap();
    fs::create_dir(src.join("sub")).unwrap();
    fs::write(src.join("sub").join("b.txt"), b"b").unwrap();
    let dst = tmp.path().join("dst");
    file_ops::copy_path(&src, &dst).unwrap();
    assert_eq!(fs::read(dst.join("a.txt")).unwrap(), b"a");
    assert_eq!(fs::read(dst.join("sub").join("b.txt")).unwrap(), b"b");
}

#[test]
fn move_file_removes_source() {
    let tmp = TempDir::new().unwrap();
    let from = tmp.path().join("a.txt");
    let to = tmp.path().join("b.txt");
    fs::write(&from, b"x").unwrap();
    file_ops::move_path(&from, &to).unwrap();
    assert!(!from.exists());
    assert_eq!(fs::read(&to).unwrap(), b"x");
}

#[test]
fn delete_path_removes_file() {
    let tmp = TempDir::new().unwrap();
    let p = tmp.path().join("a.txt");
    fs::write(&p, b"x").unwrap();
    file_ops::delete_path(&p, false).unwrap();
    assert!(!p.exists());
}

#[test]
fn delete_path_recursive_removes_dir_tree() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("d");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("a.txt"), b"x").unwrap();
    file_ops::delete_path(&dir, true).unwrap();
    assert!(!dir.exists());
}
