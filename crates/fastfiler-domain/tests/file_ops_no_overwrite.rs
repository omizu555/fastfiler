//! `file_ops::rename_path_no_overwrite` / `move_path_no_overwrite` の動作確認。
//!
//! ADR 0008 S1: Undo 経路は宛先存在時に絶対に上書きしない。

use std::fs;
use std::path::PathBuf;

use fastfiler_domain::file_ops as fops;

fn td_path(td: &tempfile::TempDir, name: &str) -> PathBuf {
    td.path().join(name)
}

#[test]
fn rename_no_overwrite_succeeds_when_destination_missing() {
    let td = tempfile::tempdir().unwrap();
    let a = td_path(&td, "a.txt");
    let b = td_path(&td, "b.txt");
    fs::write(&a, b"hi").unwrap();
    fops::rename_path_no_overwrite(&a, &b).unwrap();
    assert!(!a.exists());
    assert!(b.exists());
}

#[test]
fn rename_no_overwrite_fails_when_destination_exists() {
    let td = tempfile::tempdir().unwrap();
    let a = td_path(&td, "a.txt");
    let b = td_path(&td, "b.txt");
    fs::write(&a, b"src").unwrap();
    fs::write(&b, b"dst").unwrap();
    let res = fops::rename_path_no_overwrite(&a, &b);
    assert!(res.is_err(), "should refuse to overwrite");
    // 上書きされていないこと
    assert_eq!(fs::read(&b).unwrap(), b"dst");
    assert!(a.exists());
}

#[test]
fn move_no_overwrite_creates_parent_and_moves() {
    let td = tempfile::tempdir().unwrap();
    let src = td_path(&td, "src.txt");
    fs::write(&src, b"x").unwrap();
    let dst = td.path().join("sub").join("dst.txt");
    fops::move_path_no_overwrite(&src, &dst).unwrap();
    assert!(!src.exists());
    assert!(dst.exists());
}

#[test]
fn move_no_overwrite_fails_when_destination_exists() {
    let td = tempfile::tempdir().unwrap();
    let src = td_path(&td, "s.txt");
    let dst = td_path(&td, "d.txt");
    fs::write(&src, b"src").unwrap();
    fs::write(&dst, b"dst").unwrap();
    let res = fops::move_path_no_overwrite(&src, &dst);
    assert!(res.is_err());
    assert_eq!(fs::read(&dst).unwrap(), b"dst");
    assert!(src.exists());
}
