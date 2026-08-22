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

// =================================================================
// 仮想ファイル貼り付け (RDP/Outlook — FileGroupDescriptorW) の計画
// =================================================================

/// 仮想ファイル 1 件。`index` はクリップボード内の FILECONTENTS lindex、
/// `rel_path` は宛先相対の `dir\file.txt` 形式 (GUI 層でサニタイズ済み)。
/// core は domain 非依存のため domain::virtual_files と同形の別定義。
#[derive(Debug, Clone, PartialEq)]
pub struct VirtualEntry {
    pub index: u32,
    pub rel_path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
}

/// 仮想貼り付けの計画。conflicts は宛先に同名が存在する**トップレベル名**
/// (rel_path の先頭成分) — ダイアログ表示と解決の単位。
#[derive(Debug, Clone, PartialEq)]
pub struct VirtualPlan {
    pub dest: PathBuf,
    pub entries: Vec<VirtualEntry>,
    pub conflicts: Vec<String>,
}

/// rel_path の先頭成分 (衝突判定・リネームの単位)。
fn top_level_name(rel_path: &str) -> &str {
    rel_path.split('\\').next().unwrap_or(rel_path)
}

/// 仮想ファイル貼り付けの計画を作る。ソースパスが無いので同一フォルダ規則
/// (no-op move / 即複製) は無く、衝突検出のみ (常にコピー動作)。
pub fn plan_virtual_paste(
    entries: Vec<VirtualEntry>,
    dest: &Path,
    existing_names: &BTreeSet<String>,
) -> VirtualPlan {
    let mut conflicts: Vec<String> = Vec::new();
    for e in &entries {
        let top = top_level_name(&e.rel_path);
        if existing_names.contains(top) && !conflicts.iter().any(|c| c == top) {
            conflicts.push(top.to_string());
        }
    }
    VirtualPlan {
        dest: dest.to_path_buf(),
        entries,
        conflicts,
    }
}

/// 仮想貼り付けの衝突解決。rel_path のトップレベル成分を書き換えた
/// entries を返す (Overwrite はそのまま / Cancel は空)。
pub fn resolve_virtual_conflicts(
    plan: &VirtualPlan,
    choice: ConflictChoice,
    existing_names: &BTreeSet<String>,
) -> Vec<VirtualEntry> {
    match choice {
        ConflictChoice::Cancel => vec![],
        ConflictChoice::Overwrite => plan.entries.clone(),
        ConflictChoice::RenameBoth => {
            // 既存名 + 今回持ち込む非衝突トップレベル名の両方を避けて連番を振る
            let mut taken: BTreeSet<String> = existing_names.clone();
            for e in &plan.entries {
                let top = top_level_name(&e.rel_path);
                if !plan.conflicts.iter().any(|c| c == top) {
                    taken.insert(top.to_string());
                }
            }
            // 同一トップレベル配下の全 entry が同じ新名を共有する (フォルダの中身)
            let mut renames: std::collections::BTreeMap<String, String> =
                std::collections::BTreeMap::new();
            for c in &plan.conflicts {
                let unique = unique_name(c, &taken);
                taken.insert(unique.clone());
                renames.insert(c.clone(), unique);
            }
            plan.entries
                .iter()
                .map(|e| {
                    let top = top_level_name(&e.rel_path);
                    match renames.get(top) {
                        Some(new_top) => {
                            let rest = &e.rel_path[top.len()..]; // "\..." または ""
                            VirtualEntry {
                                rel_path: format!("{new_top}{rest}"),
                                ..e.clone()
                            }
                        }
                        None => e.clone(),
                    }
                })
                .collect()
        }
    }
}

/// F-604 の修飾キー規則 (内部 D&D と外部 OLE D&D で共通の唯一の定義):
/// Ctrl = コピー / Shift or 同一ボリューム = 移動 / それ以外 = コピー。
pub fn decide_op(ctrl: bool, shift: bool, same_vol: bool) -> TransferOp {
    if ctrl {
        TransferOp::Copy
    } else if shift || same_vol {
        // Shift 明示 or 同一ドライブの既定 = 移動 (F-604)
        TransferOp::Move
    } else {
        TransferOp::Copy
    }
}

/// 外部 D&D (OLE) の希望 DROPEFFECT を決める (F-604。spike_ole の TODO 回収)。
/// keys は MK_* フラグ、allowed は許可マスク。マスク外を返すと NONE に丸められ
/// ドロップ拒否になるため、必ず allowed 内から選ぶ。
/// 値は Win32 定義: MK_SHIFT=0x04 / MK_CONTROL=0x08、COPY=1 / MOVE=2。
pub fn decide_drop_effect(keys: u32, allowed: u32, src: Option<&Path>, dest: &Path) -> u32 {
    const MK_SHIFT: u32 = 0x04;
    const MK_CONTROL: u32 = 0x08;
    const COPY: u32 = 1;
    const MOVE: u32 = 2;
    let same_vol = src.is_some_and(|s| same_volume(s, dest));
    let desired = match decide_op(keys & MK_CONTROL != 0, keys & MK_SHIFT != 0, same_vol) {
        TransferOp::Copy => COPY,
        TransferOp::Move => MOVE,
    };
    if desired & allowed != 0 {
        desired
    } else if allowed & COPY != 0 {
        COPY
    } else if allowed & MOVE != 0 {
        MOVE
    } else {
        0
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
    fn drop_effect_rules() {
        const COPY: u32 = 1;
        const MOVE: u32 = 2;
        let c = Path::new("C:\\src\\a.txt");
        let dest_same = Path::new("C:\\dst");
        let dest_other = Path::new("D:\\dst");
        // 修飾キーなし: 同一ドライブ=MOVE / 別ドライブ=COPY
        assert_eq!(decide_drop_effect(0, COPY | MOVE, Some(c), dest_same), MOVE);
        assert_eq!(
            decide_drop_effect(0, COPY | MOVE, Some(c), dest_other),
            COPY
        );
        // Ctrl=COPY / Shift=MOVE が優先
        assert_eq!(
            decide_drop_effect(0x08, COPY | MOVE, Some(c), dest_same),
            COPY
        );
        assert_eq!(
            decide_drop_effect(0x04, COPY | MOVE, Some(c), dest_other),
            MOVE
        );
        // allowed マスク外は許可側へフォールバック
        assert_eq!(decide_drop_effect(0x04, COPY, Some(c), dest_other), COPY);
        assert_eq!(decide_drop_effect(0, 0, Some(c), dest_same), 0);
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

    fn ventry(index: u32, rel: &str, is_dir: bool) -> VirtualEntry {
        VirtualEntry {
            index,
            rel_path: rel.to_string(),
            is_dir,
            size: None,
        }
    }

    #[test]
    fn virtual_plan_flags_top_level_conflicts_once() {
        let dest = PathBuf::from("C:\\dest");
        let existing = taken(&["a.txt", "dir"]);
        let plan = plan_virtual_paste(
            vec![
                ventry(0, "a.txt", false),      // 衝突
                ventry(1, "b.txt", false),      // 衝突なし
                ventry(2, "dir", true),         // 衝突 (フォルダ)
                ventry(3, "dir\\c.txt", false), // 同じトップレベル → 重複計上しない
                ventry(4, "dir\\sub\\d.txt", false),
            ],
            &dest,
            &existing,
        );
        assert_eq!(plan.conflicts, vec!["a.txt".to_string(), "dir".to_string()]);
        assert_eq!(plan.entries.len(), 5);
    }

    #[test]
    fn virtual_rename_both_renames_folder_and_children_consistently() {
        let dest = PathBuf::from("C:\\dest");
        let existing = taken(&["dir", "b.txt"]);
        let plan = plan_virtual_paste(
            vec![
                ventry(0, "dir", true),
                ventry(1, "dir\\c.txt", false),
                ventry(2, "b.txt", false),
            ],
            &dest,
            &existing,
        );
        let resolved = resolve_virtual_conflicts(&plan, ConflictChoice::RenameBoth, &existing);
        assert_eq!(resolved[0].rel_path, "dir (2)");
        assert_eq!(resolved[1].rel_path, "dir (2)\\c.txt"); // 中身も同じ新名の下へ
        assert_eq!(resolved[2].rel_path, "b (2).txt");
    }

    #[test]
    fn virtual_rename_avoids_incoming_non_conflicted_names() {
        // 既存 "x.txt" と衝突する "x.txt" のリネームが、同時に持ち込む
        // "x (2).txt" (非衝突) を踏まないこと
        let dest = PathBuf::from("C:\\dest");
        let existing = taken(&["x.txt"]);
        let plan = plan_virtual_paste(
            vec![ventry(0, "x.txt", false), ventry(1, "x (2).txt", false)],
            &dest,
            &existing,
        );
        let resolved = resolve_virtual_conflicts(&plan, ConflictChoice::RenameBoth, &existing);
        assert_eq!(resolved[0].rel_path, "x (3).txt");
        assert_eq!(resolved[1].rel_path, "x (2).txt");
    }

    #[test]
    fn virtual_overwrite_and_cancel() {
        let dest = PathBuf::from("C:\\dest");
        let existing = taken(&["a.txt"]);
        let plan = plan_virtual_paste(vec![ventry(0, "a.txt", false)], &dest, &existing);
        let ow = resolve_virtual_conflicts(&plan, ConflictChoice::Overwrite, &existing);
        assert_eq!(ow, plan.entries);
        assert!(resolve_virtual_conflicts(&plan, ConflictChoice::Cancel, &existing).is_empty());
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
