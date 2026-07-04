//! BSP ペインツリー (計画書 §5.2 / §10-6: GPUI 版のフリー関数群を impl + テストに)。
//!
//! - タブは常に 1 つ以上のペインを持つ (最後の 1 枚は閉じられない — 呼び出し側が保証)。
//! - 同方向の分割は n-ary に展開する (GPUI 版の `ratios: Vec<f32>` と同じ表現。
//!   セッション形式 NodeData::Split { dir, ratios, children } とも一致)。

use crate::model::PaneId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDir {
    /// 左右に並ぶ (縦の仕切り)。
    Row,
    /// 上下に並ぶ (横の仕切り)。
    Column,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PaneNode {
    Leaf(PaneId),
    Split {
        /// リサイズハンドルの識別用 (タブ内で一意)。
        id: u64,
        dir: SplitDir,
        /// 各 child の比率 (合計 1.0)。
        ratios: Vec<f32>,
        children: Vec<PaneNode>,
    },
}

/// ペイン境界ドラッグの最小サイズ px (USAGE.md §2)。
pub const PANE_MIN_PX: f32 = 80.0;

impl PaneNode {
    /// 表示順 (左→右、上→下) の葉一覧。
    pub fn leaves(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect(&self, out: &mut Vec<PaneId>) {
        match self {
            PaneNode::Leaf(id) => out.push(*id),
            PaneNode::Split { children, .. } => {
                for c in children {
                    c.collect(out);
                }
            }
        }
    }

    pub fn first_leaf(&self) -> PaneId {
        match self {
            PaneNode::Leaf(id) => *id,
            PaneNode::Split { children, .. } => children[0].first_leaf(),
        }
    }

    pub fn contains(&self, id: PaneId) -> bool {
        self.leaves().contains(&id)
    }

    /// `target` 葉を `dir` 方向に分割し、直後に `new` を挿入する。
    /// 親 Split が同方向なら children へ挿入 (n-ary 拡張)、
    /// そうでなければ葉を 2 児の Split に置き換える。
    /// 戻り値: 分割できたか (target が見つからなければ false)。
    pub fn split(&mut self, target: PaneId, dir: SplitDir, new: PaneId, next_id: &mut u64) -> bool {
        match self {
            PaneNode::Leaf(id) if *id == target => {
                *next_id += 1;
                *self = PaneNode::Split {
                    id: *next_id,
                    dir,
                    ratios: vec![0.5, 0.5],
                    children: vec![PaneNode::Leaf(target), PaneNode::Leaf(new)],
                };
                true
            }
            PaneNode::Leaf(_) => false,
            PaneNode::Split {
                dir: d,
                ratios,
                children,
                ..
            } => {
                // 同方向の直下に target 葉があれば n-ary 挿入 (比率は等分割し直す)
                if *d == dir {
                    if let Some(ix) = children
                        .iter()
                        .position(|c| matches!(c, PaneNode::Leaf(id) if *id == target))
                    {
                        children.insert(ix + 1, PaneNode::Leaf(new));
                        let n = children.len() as f32;
                        *ratios = vec![1.0 / n; children.len()];
                        return true;
                    }
                }
                for c in children.iter_mut() {
                    if c.split(target, dir, new, next_id) {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// `target` 葉を取り除く。児が 1 つになった Split は昇格して畳む。
    /// 戻り値: 取り除けたか。root が対象の葉そのものの場合は false
    /// (タブ内最後の 1 枚 — 呼び出し側が閉じない)。
    pub fn remove(&mut self, target: PaneId) -> bool {
        let PaneNode::Split {
            ratios, children, ..
        } = self
        else {
            return false;
        };
        if let Some(ix) = children
            .iter()
            .position(|c| matches!(c, PaneNode::Leaf(id) if *id == target))
        {
            children.remove(ix);
            ratios.remove(ix);
            // 残り比率を正規化
            let sum: f32 = ratios.iter().sum();
            if sum > 0.0 {
                for r in ratios.iter_mut() {
                    *r /= sum;
                }
            }
            if children.len() == 1 {
                *self = children.remove(0); // 1 児 Split は昇格
            }
            return true;
        }
        for c in children.iter_mut() {
            if c.remove(target) {
                return true;
            }
        }
        false
    }

    /// リサイズ: split `id` の `ix` 番目の仕切り (child ix と ix+1 の間) の比率を更新。
    /// `delta` は比率差分 (px→比率変換は view 側)。隣接 2 児の間でだけ融通する。
    pub fn resize(&mut self, split_id: u64, handle_ix: usize, delta: f32, min_ratio: f32) -> bool {
        match self {
            PaneNode::Leaf(_) => false,
            PaneNode::Split {
                id,
                ratios,
                children,
                ..
            } => {
                if *id == split_id {
                    if handle_ix + 1 >= ratios.len() {
                        return false;
                    }
                    let pair = ratios[handle_ix] + ratios[handle_ix + 1];
                    let a = (ratios[handle_ix] + delta)
                        .clamp(min_ratio, (pair - min_ratio).max(min_ratio));
                    ratios[handle_ix] = a;
                    ratios[handle_ix + 1] = pair - a;
                    return true;
                }
                children
                    .iter_mut()
                    .any(|c| c.resize(split_id, handle_ix, delta, min_ratio))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotmap::SlotMap;

    fn ids(n: usize) -> (SlotMap<PaneId, ()>, Vec<PaneId>) {
        let mut sm: SlotMap<PaneId, ()> = SlotMap::with_key();
        let v = (0..n).map(|_| sm.insert(())).collect();
        (sm, v)
    }

    #[test]
    fn split_leaf_creates_binary_then_extends_nary() {
        let (_sm, p) = ids(3);
        let mut root = PaneNode::Leaf(p[0]);
        let mut next = 0;
        assert!(root.split(p[0], SplitDir::Row, p[1], &mut next));
        assert_eq!(root.leaves(), vec![p[0], p[1]]);
        // 同方向の再分割は n-ary に伸びる (children 3, 等比率)
        assert!(root.split(p[0], SplitDir::Row, p[2], &mut next));
        match &root {
            PaneNode::Split {
                ratios, children, ..
            } => {
                assert_eq!(children.len(), 3);
                assert_eq!(ratios, &vec![1.0 / 3.0; 3]);
            }
            _ => panic!(),
        }
        assert_eq!(root.leaves(), vec![p[0], p[2], p[1]]);
    }

    #[test]
    fn split_other_direction_nests() {
        let (_sm, p) = ids(3);
        let mut root = PaneNode::Leaf(p[0]);
        let mut next = 0;
        root.split(p[0], SplitDir::Row, p[1], &mut next);
        root.split(p[1], SplitDir::Column, p[2], &mut next);
        // p1 の位置に Column Split がネストする
        match &root {
            PaneNode::Split { dir, children, .. } => {
                assert_eq!(*dir, SplitDir::Row);
                assert!(matches!(
                    &children[1],
                    PaneNode::Split {
                        dir: SplitDir::Column,
                        ..
                    }
                ));
            }
            _ => panic!(),
        }
        assert_eq!(root.leaves(), vec![p[0], p[1], p[2]]);
    }

    #[test]
    fn remove_collapses_single_child_split() {
        let (_sm, p) = ids(3);
        let mut root = PaneNode::Leaf(p[0]);
        let mut next = 0;
        root.split(p[0], SplitDir::Row, p[1], &mut next);
        root.split(p[1], SplitDir::Column, p[2], &mut next);
        // p2 を閉じる → Column Split が畳まれ p1 の葉へ昇格
        assert!(root.remove(p[2]));
        assert_eq!(root.leaves(), vec![p[0], p[1]]);
        // p1 も閉じる → root は 1 児 Split → 昇格して Leaf(p0)
        assert!(root.remove(p[1]));
        assert_eq!(root, PaneNode::Leaf(p[0]));
        // 最後の 1 枚は remove できない
        assert!(!root.remove(p[0]));
    }

    #[test]
    fn nary_remove_renormalizes_ratios() {
        let (_sm, p) = ids(3);
        let mut root = PaneNode::Leaf(p[0]);
        let mut next = 0;
        root.split(p[0], SplitDir::Row, p[1], &mut next);
        root.split(p[1], SplitDir::Row, p[2], &mut next);
        assert!(root.remove(p[1]));
        match &root {
            PaneNode::Split { ratios, .. } => {
                assert_eq!(ratios.len(), 2);
                assert!((ratios.iter().sum::<f32>() - 1.0).abs() < 1e-6);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn resize_moves_ratio_between_neighbors_with_clamp() {
        let (_sm, p) = ids(2);
        let mut root = PaneNode::Leaf(p[0]);
        let mut next = 0;
        root.split(p[0], SplitDir::Row, p[1], &mut next);
        let split_id = match &root {
            PaneNode::Split { id, .. } => *id,
            _ => panic!(),
        };
        assert!(root.resize(split_id, 0, 0.2, 0.1));
        match &root {
            PaneNode::Split { ratios, .. } => assert_eq!(ratios, &vec![0.7, 0.3]),
            _ => panic!(),
        }
        // 最小比率でクランプ
        assert!(root.resize(split_id, 0, 1.0, 0.1));
        match &root {
            PaneNode::Split { ratios, .. } => {
                assert!((ratios[0] - 0.9).abs() < 1e-6);
                assert!((ratios[1] - 0.1).abs() < 1e-6);
            }
            _ => panic!(),
        }
    }
}
