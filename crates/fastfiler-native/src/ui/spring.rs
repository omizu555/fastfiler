//! Spring-loaded folder: D&D 中のホバー自動 navigate / expand。
//!
//! D&D 中に folder row や ツリーノード上で `SPRING_DELAY_MS` 静止すると、
//! 対象ペインを cd する、または対象ツリーノードを展開する。
//!
//! 実装メモ:
//! - 別スレッドで `sleep` → `floem::ext_event::create_ext_action` で UI スレッドに戻る。
//! - `AppState.spring_epoch` を毎 arm 毎にインクリメントし、タイマー発火時に
//!   epoch 一致 + target/kind 一致を確認してから trigger (stale 判別)。
//! - hover が外れたり drop が完了したら `disarm` で `spring_hover` を `None` にする
//!   (epoch チェックで stale なタイマーは黙って捨てられる)。

use std::path::PathBuf;
use std::time::Duration;

use floem::ext_event::create_ext_action;
use floem::reactive::{Scope, SignalGet, SignalUpdate};

use crate::core::state::{AppState, SpringHover, SpringKind};

/// hover 静止判定の閾値。
pub const SPRING_DELAY_MS: u64 = 700;

/// ペイン内 folder row hover を arm する。
///
/// D&D 中 (`app.dragging` が `Some`) でなければ何もしない。
/// 同じ pane + target で arm 済みなら何もしない (再トリガー防止)。
pub fn arm_pane(app: &AppState, pane_id: u64, target: PathBuf) {
    if app.dragging.get_untracked().is_none() {
        return;
    }
    if let Some(h) = app.spring_hover.get_untracked() {
        if h.target == target && matches!(h.kind, SpringKind::Pane(p) if p == pane_id) {
            return;
        }
    }
    arm_inner(app, target, SpringKind::Pane(pane_id));
}

/// ツリーノード hover を arm する。
pub fn arm_tree(app: &AppState, target: PathBuf) {
    if app.dragging.get_untracked().is_none() {
        return;
    }
    if let Some(h) = app.spring_hover.get_untracked() {
        if h.target == target && matches!(h.kind, SpringKind::Tree) {
            return;
        }
    }
    arm_inner(app, target, SpringKind::Tree);
}

/// hover を解除する (drop 完了 / drag cancel)。
pub fn disarm(app: &AppState) {
    if app.spring_hover.get_untracked().is_some() {
        app.spring_hover.set(None);
    }
}

/// 「自分の hover が今 active なら消す」レースに強い disarm。
/// PointerLeave 時に呼ぶ。直後の別 row PointerEnter が新 epoch で上書きしても問題ない順序にする。
pub fn disarm_if_pane(app: &AppState, pane_id: u64, target: &std::path::Path) {
    if let Some(h) = app.spring_hover.get_untracked() {
        if h.target == target && matches!(h.kind, SpringKind::Pane(p) if p == pane_id) {
            app.spring_hover.set(None);
        }
    }
}

/// ツリー用の同種ヘルパ。
pub fn disarm_if_tree(app: &AppState, target: &std::path::Path) {
    if let Some(h) = app.spring_hover.get_untracked() {
        if h.target == target && matches!(h.kind, SpringKind::Tree) {
            app.spring_hover.set(None);
        }
    }
}

fn arm_inner(app: &AppState, target: PathBuf, kind: SpringKind) {
    let epoch = app.spring_epoch.get_untracked().wrapping_add(1);
    app.spring_epoch.set(epoch);
    app.spring_hover.set(Some(SpringHover {
        epoch,
        target: target.clone(),
        kind: kind.clone(),
    }));
    crate::flog!(
        "[spring] arm epoch={} kind={:?} target={}",
        epoch,
        kind,
        target.display()
    );

    let app_cb = app.clone();
    let target_cb = target.clone();
    let kind_cb = kind.clone();
    let cb = create_ext_action(Scope::current(), move |()| {
        let Some(h) = app_cb.spring_hover.get_untracked() else {
            return;
        };
        if h.epoch != epoch || h.target != target_cb || h.kind != kind_cb {
            return;
        }
        if app_cb.dragging.get_untracked().is_none() {
            app_cb.spring_hover.set(None);
            return;
        }
        match &kind_cb {
            SpringKind::Pane(pane_id) => {
                if let Some(pane) = app_cb.find_pane(*pane_id) {
                    crate::flog!(
                        "[spring] fire pane={} navigate={}",
                        pane_id,
                        target_cb.display()
                    );
                    pane.navigate(target_cb.clone(), true);
                }
            }
            SpringKind::Tree => {
                crate::flog!("[spring] fire tree expand={}", target_cb.display());
                // ツリー側は expand 制御を tree モジュールに委譲。
                crate::ui::tree::spring_expand(&app_cb, &target_cb);
            }
        }
        app_cb.spring_hover.set(None);
    });

    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(SPRING_DELAY_MS));
        cb(());
    });
}
