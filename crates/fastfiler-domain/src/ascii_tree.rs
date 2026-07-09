//! 選択フォルダの構造を `tree` 風に ASCII (ボックス罫線) で文字列化する。
//!
//! - フォルダのみを出力 (ファイルは含まない)
//! - 深さ無制限
//! - 「隠し」判定はクロージャ (`is_hidden`) として受け取り、呼び出し側
//!   (`fastfiler-native` 側の `show_hidden` 設定など) と一致させる
//! - 名前順 (大文字小文字を区別しない) でソート
//! - 読み取りに失敗したフォルダは `[アクセス不可]` を子として 1 行付ける

use std::path::Path;

const BRANCH: &str = "├── ";
const LAST: &str = "└── ";
const VERT: &str = "│   ";
const SPACE: &str = "    ";

/// Windows 属性 (Hidden) ベースの「隠し」判定。
/// 非 Windows ではドット先頭ファイル/フォルダを隠し扱いとする。
pub fn is_hidden_default(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        std::fs::symlink_metadata(path)
            .map(|m| (m.file_attributes() & FILE_ATTRIBUTE_HIDDEN) != 0)
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.starts_with('.'))
            .unwrap_or(false)
    }
}

/// `root` を頂点として ASCII ツリー文字列を返す。
///
/// 出力例:
/// ```text
/// my-folder
/// ├── sub-a
/// │   └── inner
/// └── sub-b
/// ```
///
/// `is_hidden` は「隠し扱い」と判定するべきパスに対して `true` を返す述語。
/// `show_hidden` 設定が有効なら常に `false` を返すクロージャを渡す。
pub fn render_ascii_tree(root: &Path, is_hidden: &dyn Fn(&Path) -> bool) -> String {
    let mut out = String::new();
    let head = root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned());
    out.push_str(&head);
    out.push('\n');
    write_children(&mut out, root, "", is_hidden);
    out
}

fn write_children(out: &mut String, dir: &Path, prefix: &str, is_hidden: &dyn Fn(&Path) -> bool) {
    let children = match collect_visible_dirs(dir, is_hidden) {
        Ok(v) => v,
        Err(()) => {
            out.push_str(prefix);
            out.push_str(LAST);
            out.push_str("[アクセス不可]\n");
            return;
        }
    };
    let last_idx = children.len().saturating_sub(1);
    for (i, name) in children.iter().enumerate() {
        let is_last = i == last_idx;
        out.push_str(prefix);
        out.push_str(if is_last { LAST } else { BRANCH });
        out.push_str(name);
        out.push('\n');
        let child_path = dir.join(name);
        let next_prefix = format!("{}{}", prefix, if is_last { SPACE } else { VERT });
        write_children(out, &child_path, &next_prefix, is_hidden);
    }
}

fn collect_visible_dirs(dir: &Path, is_hidden: &dyn Fn(&Path) -> bool) -> Result<Vec<String>, ()> {
    let rd = std::fs::read_dir(dir).map_err(|_| ())?;
    let mut names: Vec<String> = Vec::new();
    for entry in rd.flatten() {
        let ft = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if !ft.is_dir() {
            continue;
        }
        let path = entry.path();
        if is_hidden(&path) {
            continue;
        }
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort_by_cached_key(|a| a.to_lowercase());
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn never_hidden(_p: &Path) -> bool {
        false
    }

    #[test]
    fn empty_folder_outputs_only_root_name() {
        let tmp = tempfile::tempdir().unwrap();
        let out = render_ascii_tree(tmp.path(), &never_hidden);
        let head = tmp.path().file_name().unwrap().to_string_lossy();
        assert_eq!(out, format!("{head}\n"));
    }

    #[test]
    fn folders_only_excludes_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("dir-a")).unwrap();
        std::fs::write(tmp.path().join("file.txt"), b"x").unwrap();
        let out = render_ascii_tree(tmp.path(), &never_hidden);
        assert!(out.contains("dir-a"));
        assert!(!out.contains("file.txt"));
    }

    #[test]
    fn nested_folders_use_box_drawing() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("a/x")).unwrap();
        std::fs::create_dir(tmp.path().join("b")).unwrap();
        let out = render_ascii_tree(tmp.path(), &never_hidden);
        // a が先、b が最後
        assert!(out.contains("├── a"));
        assert!(out.contains("│   └── x"));
        assert!(out.contains("└── b"));
    }

    #[test]
    fn hidden_predicate_filters_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("visible")).unwrap();
        std::fs::create_dir(tmp.path().join(".hidden")).unwrap();
        let hide_dot = |p: &Path| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.starts_with('.'))
                .unwrap_or(false)
        };
        let out = render_ascii_tree(tmp.path(), &hide_dot);
        assert!(out.contains("visible"));
        assert!(!out.contains(".hidden"));
    }
}
