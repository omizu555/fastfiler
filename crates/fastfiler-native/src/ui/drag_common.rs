//! D&D の Move/Copy 判定など、内部 D&D / 外部 D&D 受信の両方で共有するロジック。
//!
//! grilling Q6 (Phase 2-recv) で「Phase 1 内部 D&D と完全統一」と決定したため、
//! 元実装 (pane.rs:1366-1385) をここに移し、両側から呼ぶ。

use std::path::{Path, PathBuf};

use fastfiler_domain::path_util::volume_key;

/// Phase 1 内部 D&D / Phase 2 外部 D&D 受信で共通の Move/Copy 判定。
///
/// 戻り値: `(is_move, reason)`。`reason` はログ・perf metric 用の短い識別子。
///
/// 判定ロジック (grilling Q4/Q5 で確定):
/// - Ctrl  → Copy ("ctrl")
/// - Shift → Move ("shift")  (Ctrl+Shift は Ctrl 優先)
/// - 無修飾 + 全 source が dest と同ボリューム → Move ("same-volume")
/// - 無修飾 + dest または source の volume_key が不明 → Copy ("unknown-volume")
/// - 無修飾 + 全 source が dest と別ボリューム → Copy ("cross-volume")
/// - 無修飾 + 一部が同・一部が別 → Copy ("mixed-volume") (安全側)
pub fn compute_effect(
    sources: &[PathBuf],
    dest_dir: &Path,
    ctrl: bool,
    shift: bool,
) -> (bool, &'static str) {
    let dest_vk = volume_key(dest_dir);
    let all_same_volume = dest_vk.is_some() && sources.iter().all(|s| volume_key(s) == dest_vk);
    if ctrl {
        (false, "ctrl")
    } else if shift {
        (true, "shift")
    } else if all_same_volume {
        (true, "same-volume")
    } else if dest_vk.is_none() || sources.iter().any(|s| volume_key(s).is_none()) {
        (false, "unknown-volume")
    } else if sources.iter().all(|s| volume_key(s) != dest_vk) {
        (false, "cross-volume")
    } else {
        (false, "mixed-volume")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn ctrl_forces_copy() {
        let src = vec![p(r"C:\a.txt")];
        let (is_move, reason) = compute_effect(&src, &p(r"C:\dest"), true, false);
        assert!(!is_move);
        assert_eq!(reason, "ctrl");
    }

    #[test]
    fn shift_forces_move() {
        let src = vec![p(r"C:\a.txt")];
        let (is_move, reason) = compute_effect(&src, &p(r"D:\dest"), false, true);
        assert!(is_move);
        assert_eq!(reason, "shift");
    }

    #[test]
    fn ctrl_beats_shift() {
        let src = vec![p(r"C:\a.txt")];
        let (is_move, reason) = compute_effect(&src, &p(r"C:\dest"), true, true);
        assert!(!is_move);
        assert_eq!(reason, "ctrl");
    }

    #[test]
    fn same_volume_moves() {
        let src = vec![p(r"C:\a.txt"), p(r"C:\b.txt")];
        let (is_move, reason) = compute_effect(&src, &p(r"C:\dest"), false, false);
        assert!(is_move);
        assert_eq!(reason, "same-volume");
    }

    #[test]
    fn cross_volume_copies() {
        let src = vec![p(r"D:\a.txt")];
        let (is_move, reason) = compute_effect(&src, &p(r"C:\dest"), false, false);
        assert!(!is_move);
        assert_eq!(reason, "cross-volume");
    }

    #[test]
    fn mixed_volume_copies_safely() {
        let src = vec![p(r"C:\a.txt"), p(r"D:\b.txt")];
        let (is_move, reason) = compute_effect(&src, &p(r"C:\dest"), false, false);
        assert!(!is_move);
        assert_eq!(reason, "mixed-volume");
    }
}
