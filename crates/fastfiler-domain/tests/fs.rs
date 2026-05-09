// Phase 2C-1: fs (list_dir / stat_path / list_dirs) のユニットテスト。
use fastfiler_domain::fs as ff;
use std::fs;
use tempfile::TempDir;

fn s(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

#[test]
fn list_dir_returns_files_and_dirs() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), b"hello").unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();

    let entries = ff::list_dir(s(tmp.path())).unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"a.txt"));
    assert!(names.contains(&"sub"));

    let file = entries.iter().find(|e| e.name == "a.txt").unwrap();
    assert_eq!(file.kind, "file");
    assert_eq!(file.size, 5);

    let dir = entries.iter().find(|e| e.name == "sub").unwrap();
    assert_eq!(dir.kind, "dir");
}

#[test]
fn stat_path_returns_file_metadata() {
    let tmp = TempDir::new().unwrap();
    let p = tmp.path().join("x.bin");
    fs::write(&p, vec![0u8; 42]).unwrap();
    let st = ff::stat_path(s(&p)).unwrap();
    assert_eq!(st.name, "x.bin");
    assert_eq!(st.kind, "file");
    assert_eq!(st.size, 42);
}

#[test]
fn list_dirs_only_returns_directories() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), b"").unwrap();
    fs::create_dir(tmp.path().join("d1")).unwrap();
    fs::create_dir(tmp.path().join("d2")).unwrap();

    let dirs = ff::list_dirs(s(tmp.path()), Some(true)).unwrap();
    assert!(dirs.iter().all(|e| e.kind == "dir"));
    let names: Vec<&str> = dirs.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"d1"));
    assert!(names.contains(&"d2"));
    assert!(!names.contains(&"a.txt"));
}

#[test]
fn list_dir_errors_on_missing_path() {
    let tmp = TempDir::new().unwrap();
    let missing = tmp.path().join("does_not_exist");
    let r = ff::list_dir(s(&missing));
    assert!(r.is_err());
}
