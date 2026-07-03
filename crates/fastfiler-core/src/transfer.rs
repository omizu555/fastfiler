//! 貼り付け / 転送の計画と同名衝突の解決 (F-501/F-503)。純ロジック — テスト対象。
//!
//! 規則 (USAGE.md §2 / GPUI 版パリティ):
//! - 衝突ダイアログは [上書き] [別名で保存] [キャンセル] の 3 択、複数件は一括適用
//! - 別名は `名前 (2).拡張子` の連番 (空きが出るまで 2,3,… と進める)

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// 転送の種別。clipboard の op ("copy"/"cut") から決まる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferOp {
    Copy,
    Move,
}

/// 1 転送項目。`to_name` は宛先フォルダ直下のファイル名 (別名解決で変わり得る)。
#[derive(Debug, Clone, PartialEq)]
pub struct TransferItem {
    pub from: PathBuf,
    pub to_name: String,
}

/// 衝突解決前の転送計画。conflicts は items のうち宛先に同名が存在する index。
#[derive(Debug, Clone, PartialEq)]
pub struct TransferPlan {
    pub op: TransferOp,
    pub dest: PathBuf,
    pub items: Vec<TransferItem>,
    pub conflicts: Vec<usize>,
}

/// 衝突ダイアログの選択肢。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictChoice {
    Overwrite,
    RenameBoth,
    Cancel,
}

/// 貼り付け元パスと宛先の既存名から転送計画を作る。
/// - 同一フォルダへの移動は no-op として除外。
/// - 同一フォルダへのコピー (自己複製) は**ダイアログを出さず**その場で
///   `名前 (2).拡張子` を振る (GPUI 版 build_job_items の挙動と同じ)。
pub fn plan_transfer(
    op: TransferOp,
    sources: &[PathBuf],
    dest: &Path,
    existing_names: &BTreeSet<String>,
) -> TransferPlan {
    let mut items = Vec::new();
    let mut conflicts = Vec::new();
    // 自己複製の連番は「既存名 + 本バッチで確定した名前」と衝突しないように振る
    let mut taken: BTreeSet<String> = existing_names.clone();
    for src in sources {
        let Some(name) = src.file_name().map(|s| s.to_string_lossy().to_string()) else {
            continue;
        };
        if src.parent() == Some(dest) {
            if op == TransferOp::Move {
                continue; // 同一フォルダへの移動は no-op
            }
            // 同一フォルダへのコピー = 即複製 (衝突ダイアログなし)
            let unique = unique_name(&name, &taken);
            taken.insert(unique.clone());
            items.push(TransferItem {
                from: src.clone(),
                to_name: unique,
            });
            continue;
        }
        if existing_names.contains(&name) {
            conflicts.push(items.len());
        }
        items.push(TransferItem {
            from: src.clone(),
            to_name: name,
        });
    }
    TransferPlan {
        op,
        dest: dest.to_path_buf(),
        items,
        conflicts,
    }
}

/// 衝突の一括解決。確定した (from, to) の組を返す。Cancel は空。
pub fn resolve_conflicts(
    plan: &TransferPlan,
    choice: ConflictChoice,
    existing_names: &BTreeSet<String>,
) -> Vec<(PathBuf, PathBuf)> {
    match choice {
        ConflictChoice::Cancel => vec![],
        ConflictChoice::Overwrite => plan
            .items
            .iter()
            .filter(|it| it.from != plan.dest.join(&it.to_name)) // 自分自身への上書きは除外
            .map(|it| (it.from.clone(), plan.dest.join(&it.to_name)))
            .collect(),
        ConflictChoice::RenameBoth => {
            // 既存名 + 本バッチで確定した名前の両方と衝突しない連番を振る
            let mut taken: BTreeSet<String> = existing_names.clone();
            let conflicted: BTreeSet<usize> = plan.conflicts.iter().copied().collect();
            plan.items
                .iter()
                .enumerate()
                .map(|(i, it)| {
                    let name = if conflicted.contains(&i) {
                        unique_name(&it.to_name, &taken)
                    } else {
                        it.to_name.clone()
                    };
                    taken.insert(name.clone());
                    (it.from.clone(), plan.dest.join(name))
                })
                .collect()
        }
    }
}

/// 同一ボリューム判定 (F-604: 修飾キーなしの既定 = 同一ドライブなら移動)。
/// ドライブレター (大文字小文字無視) または UNC の `\\server\share` を比較する。
/// GPUI 版は domain path_util::volume_key — core は domain 非依存のため同等実装。
pub fn same_volume(a: &Path, b: &Path) -> bool {
    match (volume_key(a), volume_key(b)) {
        (Some(x), Some(y)) => x.eq_ignore_ascii_case(&y),
        _ => false,
    }
}

fn volume_key(p: &Path) -> Option<String> {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix("\\\\?\\") {
        return volume_key(Path::new(rest));
    }
    if let Some(rest) = s.strip_prefix("\\\\") {
        let mut it = rest.split('\\');
        let (server, share) = (it.next()?, it.next()?);
        if server.is_empty() || share.is_empty() {
            return None;
        }
        return Some(format!("\\\\{server}\\{share}"));
    }
    let mut chars = s.chars();
    let drive = chars.next()?;
    if drive.is_ascii_alphabetic() && chars.next() == Some(':') {
        return Some(format!("{drive}:"));
    }
    None
}

/// `名前 (2).拡張子` 形式の空き連番名 (USAGE.md §2)。
pub fn unique_name(name: &str, taken: &BTreeSet<String>) -> String {
    if !taken.contains(name) {
        return name.to_string();
    }
    let (stem, ext) = split_name(name);
    for n in 2.. {
        let candidate = if ext.is_empty() {
            format!("{stem} ({n})")
        } else {
            format!("{stem} ({n}).{ext}")
        };
        if !taken.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!()
}

/// "a.tar.gz" → ("a.tar", "gz") / "dir" → ("dir", "") / ".gitignore" → (".gitignore", "")。
pub fn split_name(name: &str) -> (&str, &str) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, ext),
        _ => (name, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taken(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn unique_name_numbers_like_explorer() {
        let t = taken(&["a.txt", "a (2).txt", "dir"]);
        assert_eq!(unique_name("a.txt", &t), "a (3).txt");
        assert_eq!(unique_name("b.txt", &t), "b.txt");
        assert_eq!(unique_name("dir", &t), "dir (2)");
        assert_eq!(
            unique_name(".gitignore", &taken(&[".gitignore"])),
            ".gitignore (2)"
        );
    }

    #[test]
    fn plan_flags_conflicts_and_skips_same_folder_move() {
        let dest = PathBuf::from("C:\\dest");
        let existing = taken(&["a.txt"]);
        let plan = plan_transfer(
            TransferOp::Move,
            &[
                PathBuf::from("C:\\src\\a.txt"),  // 衝突
                PathBuf::from("C:\\src\\b.txt"),  // 衝突なし
                PathBuf::from("C:\\dest\\c.txt"), // 同一フォルダ move → 除外
            ],
            &dest,
            &existing,
        );
        assert_eq!(plan.items.len(), 2);
        assert_eq!(plan.conflicts, vec![0]);
    }

    #[test]
    fn copy_into_same_folder_duplicates_immediately_without_dialog() {
        // GPUI 版パリティ: 同一フォルダの Ctrl+C→Ctrl+V はダイアログなしで即 (2)
        let dest = PathBuf::from("C:\\dest");
        let existing = taken(&["a.txt"]);
        let plan = plan_transfer(
            TransferOp::Copy,
            &[PathBuf::from("C:\\dest\\a.txt")],
            &dest,
            &existing,
        );
        assert!(plan.conflicts.is_empty()); // ダイアログ不要
        assert_eq!(plan.items[0].to_name, "a (2).txt");
        // 2 個目の自己複製は (3) に散る
        let plan2 = plan_transfer(
            TransferOp::Copy,
            &[
                PathBuf::from("C:\\dest\\a.txt"),
                PathBuf::from("C:\\dest\\a.txt"),
            ],
            &dest,
            &existing,
        );
        assert_eq!(plan2.items[1].to_name, "a (3).txt");
    }

    #[test]
    fn rename_both_avoids_batch_internal_collisions() {
        let dest = PathBuf::from("C:\\dest");
        let existing = taken(&["x.txt"]);
        // 異なるフォルダから同名 2 件 → (2) と (3) に散る
        let plan = plan_transfer(
            TransferOp::Copy,
            &[
                PathBuf::from("C:\\s1\\x.txt"),
                PathBuf::from("C:\\s2\\x.txt"),
            ],
            &dest,
            &existing,
        );
        assert_eq!(plan.conflicts, vec![0, 1]);
        let resolved = resolve_conflicts(&plan, ConflictChoice::RenameBoth, &existing);
        assert_eq!(resolved[0].1, PathBuf::from("C:\\dest\\x (2).txt"));
        assert_eq!(resolved[1].1, PathBuf::from("C:\\dest\\x (3).txt"));
    }

    #[test]
    fn same_volume_rules() {
        assert!(same_volume(Path::new("C:\\a\\b"), Path::new("c:\\x")));
        assert!(!same_volume(Path::new("C:\\a"), Path::new("D:\\a")));
        assert!(same_volume(
            Path::new("\\\\nas\\media\\x"),
            Path::new("\\\\NAS\\media\\y")
        ));
        assert!(!same_volume(
            Path::new("\\\\nas\\media"),
            Path::new("\\\\nas\\docs")
        ));
        assert!(!same_volume(Path::new("C:\\a"), Path::new("\\\\nas\\m")));
    }

    #[test]
    fn overwrite_and_cancel() {
        let dest = PathBuf::from("C:\\dest");
        let existing = taken(&["a.txt"]);
        let plan = plan_transfer(
            TransferOp::Copy,
            &[PathBuf::from("C:\\src\\a.txt")],
            &dest,
            &existing,
        );
        let ow = resolve_conflicts(&plan, ConflictChoice::Overwrite, &existing);
        assert_eq!(
            ow,
            vec![(
                PathBuf::from("C:\\src\\a.txt"),
                PathBuf::from("C:\\dest\\a.txt")
            )]
        );
        assert!(resolve_conflicts(&plan, ConflictChoice::Cancel, &existing).is_empty());
    }
}
