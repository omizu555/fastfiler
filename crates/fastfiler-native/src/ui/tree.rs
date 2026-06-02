// Tree pane — フォルダツリー (遅延展開)

use std::path::PathBuf;

use floem::event::{Event, EventListener, EventPropagation};
use floem::keyboard::{Key, KeyEvent, NamedKey};
use floem::menu::{Menu, MenuItem};
use floem::prelude::*;
use floem::reactive::{SignalGet, SignalUpdate};
use floem::style::CursorStyle;
use floem::views::{container, dyn_container, h_stack, label, scroll, v_stack, Decorators};

use crate::core::tree_model::{
    expand_to_path, find_node, parent_in_tree, reload_expanded_recursive, visible_paths,
    visual_index, TreeNode, TreeNodeKind,
};
use crate::state::AppState;
use crate::theme;

/// Spring-loaded folder からの呼出: 対象パスのツリーノードを展開する (cd はしない)。
/// 既に展開済みなら何もしない。
pub fn spring_expand(app: &AppState, target: &std::path::Path) {
    let roots = app.tree_roots.get_untracked();
    if let Some(node) = find_node(&roots, target) {
        if !node.expanded.get_untracked() {
            node.load_children();
            node.expanded.set(true);
            // 関連 effect を起動するため tree_tick も bump (任意)。
            app.tree_tick.update(|t| *t = t.wrapping_add(1));
        }
    } else {
        // ルートからの経路が未展開なら expand_to_path で道中を全部開く。
        expand_to_path(&roots, target);
        app.tree_tick.update(|t| *t = t.wrapping_add(1));
    }
}

pub fn render_tree_node(app: AppState, node: TreeNode, depth: usize) -> floem::AnyView {
    let expanded = node.expanded;
    let children = node.children;
    let kind = node.kind;
    let path_for_nav = node.path.clone();
    let path_for_focus_style = node.path.clone();
    let path_for_click = node.path.clone();
    let name_text = node.name.clone();

    let focused_path_sig = app.tree_focused_path;
    let tree_focused_sig = app.tree_focused;
    let accent_sig = app.settings.accent_color;
    let theme_sig = app.settings.theme;

    let app_for_click = app.clone();
    let app_for_row_click = app.clone();
    let node_for_toggle = node.clone();

    let arrow = label(move || {
        if expanded.get() {
            String::from("▼")
        } else {
            String::from("▶")
        }
    })
    .style(|s| {
        s.width(14)
            .padding_horiz(2)
            .color(theme::text_dim())
            .cursor(CursorStyle::Pointer)
    })
    .on_click_stop(move |_| {
        let was = expanded.get();
        if !was {
            node_for_toggle.load_children();
        }
        expanded.set(!was);
    });

    let name_lbl = label(move || name_text.clone())
        .style(|s| {
            s.flex_grow(1.0)
                .padding_horiz(4)
                .cursor(CursorStyle::Pointer)
                .color(theme::text_normal())
        })
        .on_click_stop(move |_| {
            // UncServer (仮想 root) はナビゲートしない (実在しない path のため)
            if matches!(kind, TreeNodeKind::UncServer) {
                return;
            }
            if let Some(p) = app_for_click.active_pane() {
                p.navigate(path_for_nav.clone(), true);
            }
        });

    let indent = (depth as f32) * 14.0 + 4.0;
    let app_for_menu = app.clone();
    let path_for_menu = node.path.clone();
    let app_for_spring_enter = app.clone();
    let app_for_spring_leave = app.clone();
    let path_for_spring_enter = node.path.clone();
    let path_for_spring_leave = node.path.clone();
    let row = h_stack((arrow, name_lbl))
        .style(move |s| {
            // 自分のパスがフォーカス中なら淡いアクセント背景。tree_focused=false の時は薄く。
            let is_focused = focused_path_sig
                .get()
                .map(|p| p == path_for_focus_style)
                .unwrap_or(false);
            let s = s.height(22).items_center().padding_left(indent);
            if is_focused {
                // theme と accent 読み取りで明暗適応
                let _ = theme_sig.get();
                let _ = accent_sig.get();
                let bg = if tree_focused_sig.get() {
                    theme::accent_select()
                } else {
                    theme::bg_hover()
                };
                s.background(bg)
            } else {
                s
            }
        })
        .on_click_stop(move |_| {
            // クリックで focused_path を更新 (Enter キーの対象を切替)
            app_for_row_click
                .tree_focused_path
                .set(Some(path_for_click.clone()));
            app_for_row_click.request_tree_focus();
        })
        .on_event_cont(EventListener::PointerEnter, move |_| {
            // Spring-loaded tree: D&D 中のみ、UncServer 等の仮想 root を除外。
            if matches!(kind, TreeNodeKind::UncServer) {
                return;
            }
            if app_for_spring_enter.dragging.get_untracked().is_none() {
                return;
            }
            crate::ui::spring::arm_tree(&app_for_spring_enter, path_for_spring_enter.clone());
        })
        .on_event_cont(EventListener::PointerLeave, move |_| {
            crate::ui::spring::disarm_if_tree(&app_for_spring_leave, &path_for_spring_leave);
        })
        .context_menu(move || {
            build_tree_node_menu(app_for_menu.clone(), kind, path_for_menu.clone())
        });

    let app_for_kids = app.clone();
    let kids = dyn_container(
        move || (expanded.get(), children.get()),
        move |(open, kids)| {
            if !open {
                return container(label(String::new))
                    .style(|s| s.height(0))
                    .into_any();
            }
            let app = app_for_kids.clone();
            let items: Vec<floem::AnyView> = kids
                .into_iter()
                .map(|c| render_tree_node(app.clone(), c, depth + 1))
                .collect();
            floem::views::stack_from_iter(items)
                .style(|s| s.flex_col())
                .into_any()
        },
    );

    v_stack((row, kids)).style(|s| s.flex_col()).into_any()
}

/// ツリーノードの右クリックメニューを構築。
/// UncServer はそのサーバ配下の全 share を削除、UncShare は単一 share を削除。
fn build_tree_node_menu(app: AppState, kind: TreeNodeKind, path: PathBuf) -> Menu {
    match kind {
        TreeNodeKind::UncServer => {
            // \\server 形式 → 配下 (\\server\*) を全削除
            let server_prefix = {
                let s = path.to_string_lossy().to_string();
                // 末尾の \ を除いた \\server を比較用に
                s.trim_end_matches('\\').to_lowercase()
            };
            let app_for_act = app.clone();
            Menu::new("").entry(
                MenuItem::new("ツリーから削除 (サーバごと)").action(move || {
                    let cur = app_for_act.settings.tree_unc_shares.get_untracked();
                    let next: Vec<String> = cur
                        .into_iter()
                        .filter(|s| {
                            // \\server\share の prefix が \\server (大文字小文字無視) と一致するものを除外
                            let lower = s.to_lowercase();
                            !(lower.starts_with(&format!("{}\\", server_prefix))
                                || lower == server_prefix)
                        })
                        .collect();
                    app_for_act.settings.tree_unc_shares.set(next);
                    // フォーカスがこのサーバ配下を指していたらクリア
                    if let Some(fp) = app_for_act.tree_focused_path.get_untracked() {
                        let fp_lower = fp.to_string_lossy().to_lowercase();
                        if fp_lower.starts_with(&server_prefix) {
                            app_for_act.tree_focused_path.set(None);
                        }
                    }
                }),
            )
        }
        TreeNodeKind::UncShare => {
            let share_path = path.to_string_lossy().to_string();
            let share_lower = share_path.to_lowercase();
            let app_for_act = app.clone();
            Menu::new("").entry(MenuItem::new("ツリーから削除").action(move || {
                let cur = app_for_act.settings.tree_unc_shares.get_untracked();
                let next: Vec<String> = cur
                    .into_iter()
                    .filter(|s| s.to_lowercase() != share_lower)
                    .collect();
                app_for_act.settings.tree_unc_shares.set(next);
                if let Some(fp) = app_for_act.tree_focused_path.get_untracked() {
                    let fp_lower = fp.to_string_lossy().to_lowercase();
                    if fp_lower == share_lower
                        || fp_lower.starts_with(&format!("{}\\", share_lower))
                    {
                        app_for_act.tree_focused_path.set(None);
                    }
                }
            }))
        }
        _ => Menu::new(""),
    }
}

pub fn tree_pane(app: AppState) -> impl IntoView {
    // tree_roots は AppState の effect (app_view.rs) で reconcile される。
    // ここでは空の roots でも OK (起動直後は空でも UI は壊れない)。
    let roots_sig = app.tree_roots;
    // スクロール先 Y 座標 (フォローエフェクトが set、scroll が読む)
    let scroll_target: RwSignal<Option<floem::kurbo::Point>> =
        floem::reactive::Scope::new().create_rw_signal(None);

    // tree_tick を track し、展開済みノードを再帰的に reload する単一 effect。
    // tree_pane の scope に置くことで、レンダリング再生成で破棄されないようにする。
    let tree_tick = app.tree_tick;
    let roots_for_effect = roots_sig;
    floem::reactive::create_effect(move |_| {
        let t = tree_tick.get();
        let roots = roots_for_effect.get_untracked();
        let mut count = 0u32;
        for r in roots.iter() {
            count += reload_expanded_recursive(r);
        }
        if count > 0 {
            crate::flog!("[tree] tick={} reloaded {} expanded nodes", t, count);
        }
    });

    // アクティブペインの cur_path に追従してツリーを自動展開する effect。
    // active タブ → そのタブの active_pane → そのペインの cur_path を全て track。
    let app_for_follow = app.clone();
    let roots_for_follow = roots_sig;
    floem::reactive::create_effect(move |_| {
        // tree_roots 自体の変化 (UNC 登録等) でも再走する
        let _ = roots_for_follow.get();
        // active タブ id を track
        let id = app_for_follow.active.get();
        let tabs = app_for_follow.tabs.get();
        let Some(tab) = tabs.iter().find(|t| t.id == id).cloned() else {
            return;
        };
        // active_pane id を track
        let active_pane_id = tab.active_pane.get();
        // root を track して再描画にも追従
        tab.root.with(|_| {});
        let panes = tab.all_panes();
        let pane = panes
            .iter()
            .find(|p| p.id == active_pane_id)
            .cloned()
            .or_else(|| panes.first().cloned());
        let Some(pane) = pane else { return };
        // cur_path を track
        let path = pane.cur_path.get();
        let roots = roots_for_follow.get_untracked();
        let depth = expand_to_path(&roots, &path);
        // 表示インデックス → Y 座標 (row 22px、上下に少し余白)
        if let Some(idx) = visual_index(&roots, &path) {
            let y = (idx as f64) * 22.0;
            // 上に少しマージンを残してスクロール
            let target_y = (y - 44.0).max(0.0);
            scroll_target.set(Some(floem::kurbo::Point::new(0.0, target_y)));
            crate::flog!(
                "[tree] follow pane={} path={} depth={} idx={} scroll_y={}",
                pane.id,
                path.display(),
                depth,
                idx,
                target_y
            );
        } else {
            crate::flog!(
                "[tree] follow pane={} path={} depth={} (no idx)",
                pane.id,
                path.display(),
                depth
            );
        }
    });

    // focused_path が変わったときも scroll で追従させる effect
    let app_for_focus_scroll = app.clone();
    floem::reactive::create_effect(move |_| {
        let Some(path) = app_for_focus_scroll.tree_focused_path.get() else {
            return;
        };
        let roots = app_for_focus_scroll.tree_roots.get_untracked();
        if let Some(idx) = visual_index(&roots, &path) {
            let y = (idx as f64) * 22.0;
            let target_y = (y - 44.0).max(0.0);
            scroll_target.set(Some(floem::kurbo::Point::new(0.0, target_y)));
        }
    });

    let app_for_render = app.clone();
    let tree = dyn_container(
        move || roots_sig.get(),
        move |roots| {
            let app = app_for_render.clone();
            let items: Vec<floem::AnyView> = roots
                .into_iter()
                .map(|n| render_tree_node(app.clone(), n, 0))
                .collect();
            floem::views::stack_from_iter(items)
                .style(|s| s.flex_col())
                .into_any()
        },
    );

    let header = label(|| String::from("Tree"))
        .style(|s| s.padding(6).font_bold().color(theme::text_label()));

    let body = v_stack((
        header,
        scroll(tree)
            .scroll_to(move || scroll_target.get())
            .style(|s| s.flex_grow(1.0).min_height(0).width_full()),
    ))
    .style(|s| s.flex_col().size_full());

    let tree_width_sig = app.settings.tree_width;
    let app_for_key = app.clone();
    let app_for_focus = app.clone();
    let tree_focus_req_sig = app.tree_focus_req;
    container(body)
        .keyboard_navigable()
        .request_focus(move || {
            tree_focus_req_sig.get();
        })
        .on_event_cont(EventListener::FocusGained, move |_| {
            let app = &app_for_focus;
            app.tree_focused.set(true);
            // 初期位置: なければ active pane の cur_path
            if app.tree_focused_path.get_untracked().is_none() {
                if let Some(p) = app.active_pane() {
                    let cur = p.cur_path.get_untracked();
                    app.tree_focused_path.set(Some(cur));
                }
            }
        })
        .on_event_cont(EventListener::FocusLost, {
            let app_lost = app.clone();
            move |_| {
                app_lost.tree_focused.set(false);
            }
        })
        .on_event(EventListener::KeyDown, move |ev| {
            if let Event::KeyDown(ke) = ev {
                if handle_tree_key(&app_for_key, ke) {
                    return EventPropagation::Stop;
                }
            }
            EventPropagation::Continue
        })
        .style(move |s| {
            let w = tree_width_sig
                .get()
                .parse::<f32>()
                .unwrap_or(240.0)
                .clamp(120.0, 600.0);
            s.width(w)
                .height_full()
                .background(theme::bg_panel())
                .border_right(1)
                .border_color(theme::border_default())
        })
}

/// ツリーペインのキーボード操作。
/// 処理した場合 true (caller は EventPropagation::Stop)。
fn handle_tree_key(app: &AppState, ke: &KeyEvent) -> bool {
    use floem::keyboard::Modifiers;
    if ke.modifiers != Modifiers::empty() {
        return false;
    }
    let roots = app.tree_roots.get_untracked();
    let cur = app.tree_focused_path.get_untracked();
    let visible = visible_paths(&roots);
    if visible.is_empty() {
        return false;
    }
    let cur_idx = cur
        .as_ref()
        .and_then(|p| visible.iter().position(|x| x == p));

    match &ke.key.logical_key {
        Key::Named(NamedKey::ArrowDown) => {
            let next = match cur_idx {
                Some(i) if i + 1 < visible.len() => i + 1,
                None => 0,
                Some(i) => i,
            };
            app.tree_focused_path.set(Some(visible[next].clone()));
            true
        }
        Key::Named(NamedKey::ArrowUp) => {
            let next = match cur_idx {
                Some(i) if i > 0 => i - 1,
                None => 0,
                Some(i) => i,
            };
            app.tree_focused_path.set(Some(visible[next].clone()));
            true
        }
        Key::Named(NamedKey::Home) => {
            app.tree_focused_path.set(Some(visible[0].clone()));
            true
        }
        Key::Named(NamedKey::End) => {
            app.tree_focused_path
                .set(Some(visible[visible.len() - 1].clone()));
            true
        }
        Key::Named(NamedKey::ArrowRight) => {
            if let Some(p) = cur.as_ref() {
                if let Some(node) = find_node(&roots, p) {
                    if node.expanded.get_untracked() {
                        // 展開中: 最初の子へ
                        let kids = node.children.get_untracked();
                        if let Some(first) = kids.front() {
                            app.tree_focused_path.set(Some(first.path.clone()));
                        }
                    } else {
                        node.load_children();
                        node.expanded.set(true);
                    }
                    return true;
                }
            }
            false
        }
        Key::Named(NamedKey::ArrowLeft) => {
            if let Some(p) = cur.as_ref() {
                if let Some(node) = find_node(&roots, p) {
                    if node.expanded.get_untracked() {
                        node.expanded.set(false);
                    } else if let Some(parent) = parent_in_tree(&roots, p) {
                        app.tree_focused_path.set(Some(parent));
                    }
                    return true;
                }
            }
            false
        }
        Key::Named(NamedKey::Enter) => {
            if let Some(p) = cur.as_ref() {
                if let Some(pane) = app.active_pane() {
                    pane.navigate(p.clone(), true);
                }
                return true;
            }
            false
        }
        Key::Named(NamedKey::Escape) => {
            app.tree_focused.set(false);
            // フォーカスを解除するため、active pane の何か (ペイン側) に request_focus
            // 今は tree_focused フラグだけ落とせば視覚的には外れる
            true
        }
        _ => false,
    }
}
