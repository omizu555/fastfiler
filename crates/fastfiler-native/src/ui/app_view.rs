// app_view + persist_window_state — アプリ全体レイアウト

use floem::event::{Event, EventListener, EventPropagation};
use floem::prelude::*;
use floem::reactive::{SignalGet, SignalUpdate};
use floem::views::{container, dyn_container, label, v_stack, Decorators};

use crate::fs_model::initial_path;
use crate::settings::{settings_view, AppSettings};
use crate::state::{AppState, PaneSplitterDrag, SplitDir, SplitNode, SplitterTarget};
use crate::theme;
use crate::ui::footer::footer_bar;
use crate::ui::pane::pane_view;
use crate::ui::splitter::splitter;
use crate::ui::tabs::tabs_panel;
use crate::ui::tree::tree_pane;

pub fn persist_window_state(settings: &AppSettings) {
    if let Err(e) = settings.save() {
        eprintln!("[settings] window-state save error: {}", e);
    }
}

/// SplitNode を再帰的に描画 (BSP)
fn render_split_node(node: SplitNode, app: AppState) -> floem::AnyView {
    match node {
        SplitNode::Leaf(p) => container(pane_view(p, app))
            .style(|s| s.size_full().min_width(0).min_height(0))
            .into_any(),
        SplitNode::Split {
            dir,
            children,
            ratios,
        } => {
            let count = children.len();
            // Split コンテナのピクセルサイズ (該当軸) を追跡
            let container_size: floem::reactive::RwSignal<f32> =
                floem::reactive::Scope::new().create_rw_signal(0.0_f32);
            let mut items: Vec<floem::AnyView> = Vec::with_capacity(count.saturating_mul(2));
            for (i, child) in children.into_iter().enumerate() {
                // 子の前にスプリッタ (i > 0)
                if i > 0 && count > 1 {
                    let bar_dir = dir;
                    let ratios_for_bar = ratios;
                    let app_for_bar = app.clone();
                    let idx_a = i - 1;
                    let bar = floem::views::empty()
                        .style(move |s| {
                            let s = s
                                .background(theme::border_default())
                                .hover(|s| s.background(theme::accent_select()));
                            match bar_dir {
                                SplitDir::Horizontal => s
                                    .width(5.0)
                                    .height_full()
                                    .cursor(floem::style::CursorStyle::ColResize),
                                SplitDir::Vertical => s
                                    .height(5.0)
                                    .width_full()
                                    .cursor(floem::style::CursorStyle::RowResize),
                            }
                        })
                        .on_event_stop(EventListener::PointerDown, move |e| {
                            if let Event::PointerDown(p) = e {
                                if !p.button.is_primary() {
                                    return;
                                }
                                app_for_bar.pane_splitter_drag.set(Some(PaneSplitterDrag {
                                    dir: bar_dir,
                                    ratios: ratios_for_bar,
                                    idx_a,
                                }));
                            }
                        });
                    items.push(bar.into_any());
                }
                let app_for_child = app.clone();
                let child_view = render_split_node(child, app_for_child);
                let child_idx = i;
                let ratios_for_child = ratios;
                items.push(
                    container(child_view)
                        .style(move |s| {
                            // ratio が極端に小さいと描画崩れするので最低値を確保
                            let r = ratios_for_child
                                .with(|v| v.get(child_idx).copied().unwrap_or(1.0))
                                .max(0.01);
                            s.flex_grow(r).flex_basis(0).min_width(0).min_height(0)
                        })
                        .into_any(),
                );
            }
            let stack = floem::views::stack_from_iter(items).style(move |s| match dir {
                SplitDir::Horizontal => s.flex_row().size_full(),
                SplitDir::Vertical => s.flex_col().size_full(),
            });
            let ratios_for_move = ratios;
            let app_for_move = app.clone();
            container(stack)
                .style(|s| s.size_full())
                .on_resize(move |rect| {
                    let v = match dir {
                        SplitDir::Horizontal => rect.width() as f32,
                        SplitDir::Vertical => rect.height() as f32,
                    };
                    if (container_size.get_untracked() - v).abs() > 0.5 {
                        container_size.set(v);
                    }
                })
                .on_event_cont(EventListener::PointerMove, move |e| {
                    if let Event::PointerMove(p) = e {
                        let Some(pd) = app_for_move.pane_splitter_drag.get_untracked() else {
                            return;
                        };
                        // この Split コンテナ自身のドラッグでなければ無視
                        if pd.ratios != ratios_for_move || pd.dir != dir {
                            return;
                        }
                        let total = container_size.get_untracked();
                        if total < 1.0 {
                            return;
                        }
                        // p.pos はこの Split コンテナのローカル座標
                        let cur_px = match dir {
                            SplitDir::Horizontal => p.pos.x as f32,
                            SplitDir::Vertical => p.pos.y as f32,
                        };
                        ratios_for_move.update(|v| {
                            if pd.idx_a + 1 >= v.len() {
                                return;
                            }
                            // [idx_a] と [idx_a+1] の合計を保ったまま境界位置を更新
                            let prev_boundary_ratio: f32 = v[..pd.idx_a].iter().sum();
                            let prev_boundary_px = prev_boundary_ratio * total;
                            let pair_sum = v[pd.idx_a] + v[pd.idx_a + 1];
                            let min_r = (0.05_f32).min(pair_sum * 0.5);
                            let max_a = pair_sum - min_r;
                            let new_a = ((cur_px - prev_boundary_px) / total).clamp(min_r, max_a);
                            let new_b = pair_sum - new_a;
                            v[pd.idx_a] = new_a;
                            v[pd.idx_a + 1] = new_b;
                        });
                    }
                })
                .into_any()
        }
    }
}

pub fn app_view() -> impl IntoView {
    let app = AppState::new(initial_path());
    let settings_open = app.settings_open;

    // ジョブ進捗 (コピー / 移動 / 削除) の inbox polling を起動
    app.jobs.start_polling();

    // 設定値変化時の自動保存 (タブ列数 / タブペイン幅 / ツリーペイン幅 / open_tabs)
    {
        let settings_for_save = app.settings.clone();
        let tab_columns_sig = app.settings.tab_columns;
        let tabs_width_sig = app.settings.tabs_width;
        let tree_width_sig = app.settings.tree_width;
        floem::reactive::create_effect(move |prev: Option<(String, String, String)>| {
            let cols = tab_columns_sig.get();
            let tw = tabs_width_sig.get();
            let trw = tree_width_sig.get();
            let cur = (cols, tw, trw);
            let changed = prev.as_ref().map_or(false, |p| p != &cur);
            if changed {
                if let Err(e) = settings_for_save.save() {
                    eprintln!("[settings] auto-save error: {}", e);
                }
            }
            cur
        });
    }
    // show_hidden 切替時に全 pane の rows を再読込 (dyn_container 再構築を避けるため独立 effect)
    {
        let app_for_hidden = app.clone();
        let show_hidden_sig = app.settings.show_hidden;
        floem::reactive::create_effect(move |prev: Option<bool>| {
            let cur = show_hidden_sig.get();
            if let Some(p) = prev {
                if p != cur {
                    let tabs_v = app_for_hidden.tabs.get_untracked();
                    for tab in tabs_v.iter() {
                        for pane in tab.all_panes() {
                            pane.refresh_rows_only();
                        }
                    }
                }
            }
            cur
        });
    }
    // タブ一覧 / 各タブの primary パス / BSP 構造 / locked 変化時に永続化
    {
        let app_for_tabs = app.clone();
        floem::reactive::create_effect(
            move |prev: Option<(Vec<String>, Vec<String>, Vec<bool>)>| {
                let tabs_v = app_for_tabs.tabs.get();
                let paths: Vec<String> = tabs_v
                    .iter()
                    .map(|t| t.primary().cur_path.get().to_string_lossy().into_owned())
                    .collect();
                let layouts: Vec<String> = tabs_v
                    .iter()
                    .map(|t| {
                        // すべての leaf の cur_path を track して、副ペインの navigate でも保存が走るように
                        for pane in t.all_panes() {
                            let _ = pane.cur_path.get();
                        }
                        let saved = t.root.with(|r| r.to_saved());
                        serde_json::to_string(&saved).unwrap_or_default()
                    })
                    .collect();
                let locked: Vec<bool> = tabs_v.iter().map(|t| t.locked.get()).collect();
                let cur = (paths, layouts, locked);
                let changed = prev.as_ref().map_or(true, |p| p != &cur);
                if changed {
                    app_for_tabs.settings.open_tabs.set(cur.0.clone());
                    app_for_tabs.settings.tab_layouts.set(cur.1.clone());
                    app_for_tabs.settings.tab_locked.set(cur.2.clone());
                    if prev.is_some() {
                        if let Err(e) = app_for_tabs.settings.save() {
                            eprintln!("[settings] tabs auto-save error: {}", e);
                        }
                    }
                }
                cur
            },
        );
    }

    // ─────────────────────────────────────────────────────────────
    //  ワークスペースツリー: UNC 自動登録 + tree_roots 再構築 + auto-save
    // ─────────────────────────────────────────────────────────────
    // 全タブ全ペインの cur_path を track。UNC を検出したら share root を
    // tree_unc_shares に追加 (正規化 + 大文字小文字無視 dedup)。
    {
        let app_for_unc = app.clone();
        floem::reactive::create_effect(move |_| {
            let tabs_v = app_for_unc.tabs.get();
            let mut found_shares: Vec<String> = Vec::new();
            for tab in tabs_v.iter() {
                // BSP 構造変化も track
                tab.root.with(|_| {});
                for pane in tab.all_panes() {
                    let p = pane.cur_path.get();
                    if let Some(norm) = crate::core::tree_model::normalize_unc_share(&p) {
                        let s = norm.to_string_lossy().into_owned();
                        if !found_shares.contains(&s) {
                            found_shares.push(s);
                        }
                    }
                }
            }
            if found_shares.is_empty() {
                return;
            }
            let cur = app_for_unc.settings.tree_unc_shares.get_untracked();
            let cur_set: std::collections::HashSet<String> =
                cur.iter().map(|s| s.to_lowercase()).collect();
            let mut next = cur.clone();
            let mut added = false;
            for s in found_shares {
                if !cur_set.contains(&s.to_lowercase()) {
                    next.push(s);
                    added = true;
                }
            }
            if added {
                app_for_unc.settings.tree_unc_shares.set(next);
            }
        });
    }

    // tree_unc_shares の変化を watch → tree_roots を rebuild + settings.save()。
    // 初回 (prev=None) の rebuild は読み込み済 shares で行うが save はスキップ。
    {
        let app_for_rebuild = app.clone();
        floem::reactive::create_effect(move |prev: Option<Vec<String>>| {
            let shares = app_for_rebuild.settings.tree_unc_shares.get();
            // ローカルドライブ一覧 (起動中変化しない想定)
            let drives = crate::fs_model::list_drives();
            let existing = app_for_rebuild.tree_roots.get_untracked();
            let new_roots =
                crate::core::tree_model::reconcile_tree_roots(&existing, &drives, &shares);
            app_for_rebuild.tree_roots.set(new_roots);

            // 初回は save しない、2 回目以降で内容が変わった時のみ save
            if let Some(p) = &prev {
                if p != &shares {
                    if let Err(e) = app_for_rebuild.settings.save() {
                        eprintln!("[settings] tree_unc_shares save error: {}", e);
                    }
                }
            }
            shares
        });
    }

    // テーマ/プリセット/アクセント変更時に theme_rev をインクリメントして全 UI を再構築する。
    // theme.rs の関数はクロージャ評価のたびに最新値を返すので、再構築すれば即時反映される。
    {
        let app_for_theme = app.clone();
        let theme_sig = app.settings.theme;
        let preset_sig = app.settings.theme_preset;
        let accent_sig = app.settings.accent_color;
        let icon_set_sig = app.settings.icon_set;
        floem::reactive::create_effect(move |prev: Option<(String, String, String, String)>| {
            let cur = (
                theme_sig.get(),
                preset_sig.get(),
                accent_sig.get(),
                icon_set_sig.get(),
            );
            if let Some(p) = prev.as_ref() {
                if p != &cur {
                    crate::theme::set_mode_from_str(&cur.0);
                    crate::theme::set_preset_from_str(&cur.1);
                    crate::theme::set_accent_from_str(&cur.2);
                    app_for_theme.theme_rev.update(|v| *v = v.wrapping_add(1));
                }
            }
            cur
        });
    }

    let theme_rev = app.theme_rev;
    let switcher = dyn_container(move || (settings_open.get(), theme_rev.get()), {
        let app = app.clone();
        move |(open, _rev)| {
            if open {
                settings_view(app.settings.clone(), settings_open).into_any()
            } else {
                let app = app.clone();
                let app_for_layout = app.clone();
                let dock_tabs_sig = app.settings.panel_dock_tabs;
                let dock_tree_sig = app.settings.panel_dock_tree;
                let main_row = dyn_container(
                    move || {
                        let dt = dock_tabs_sig.get();
                        let dr = dock_tree_sig.get();
                        (dt, dr)
                    },
                    move |(dock_tabs, dock_tree)| {
                        let app = app_for_layout.clone();
                        let tabs_hidden = dock_tabs == "hidden";
                        let tree_hidden = dock_tree == "hidden";
                        let tabs_right = dock_tabs == "right";
                        let tree_right = dock_tree == "right";

                        // 中央 active_panes はレイアウト再構築の都度新規生成する。
                        // AnyView は再 mount できないため holder で 1 回限り取り出す方式は破綻する
                        // (toggle-tree / タブ切替で `(no center)` 化 + その後 panic を招いた)。
                        let active = app.active;
                        let tabs = app.tabs;
                        let app_for_panes = app.clone();
                        let active_panes = dyn_container(
                            move || {
                                let id = active.get();
                                let tabs_v = tabs.get();
                                let active_tab = tabs_v
                                    .iter()
                                    .find(|t| t.id == id)
                                    .cloned()
                                    .or_else(|| tabs_v.iter().next().cloned());
                                active_tab.map(|t| t.root.get())
                            },
                            move |root: Option<SplitNode>| {
                                let app = app_for_panes.clone();
                                match root {
                                    None => label(|| String::from("(no tab)"))
                                        .style(|s| s.size_full().padding(20))
                                        .into_any(),
                                    Some(node) => render_split_node(node, app).into_any(),
                                }
                            },
                        )
                        .style(|s| s.flex_grow(1.0).min_height(0).min_width(0).flex_col());

                        let mut items: Vec<floem::AnyView> = Vec::new();
                        if !tabs_hidden && !tabs_right {
                            items.push(tabs_panel(app.clone()).into_any());
                            items.push(splitter(app.clone(), SplitterTarget::Tabs).into_any());
                        }
                        if !tree_hidden && !tree_right {
                            items.push(tree_pane(app.clone()).into_any());
                            items.push(splitter(app.clone(), SplitterTarget::Tree).into_any());
                        }
                        items.push(active_panes.into_any());
                        if !tree_hidden && tree_right {
                            items.push(splitter(app.clone(), SplitterTarget::Tree).into_any());
                            items.push(tree_pane(app.clone()).into_any());
                        }
                        if !tabs_hidden && tabs_right {
                            items.push(splitter(app.clone(), SplitterTarget::Tabs).into_any());
                            items.push(tabs_panel(app.clone()).into_any());
                        }
                        floem::views::stack_from_iter(items)
                            .style(|s| s.flex_row().flex_grow(1.0).min_height(0).width_full())
                            .into_any()
                    },
                )
                .style(|s| s.flex_grow(1.0).min_height(0).width_full());
                v_stack((main_row, footer_bar(app.clone())))
                    .style(|s| s.size_full().flex_col())
                    .into_any()
            }
        }
    })
    .style(|s| s.size_full());

    let ui_font_size_sig = app.settings.ui_font_size;
    let ui_font_sig = app.settings.ui_font;
    let jobs_for_overlay = app.jobs.clone();
    let app_for_modal = app.clone();
    floem::views::stack((
        switcher,
        crate::ui::modal_dialog::modal_dialog(app_for_modal),
        crate::ui::progress::progress_dialogs(jobs_for_overlay),
    ))
    .style(move |s| {
        let fs = ui_font_size_sig
            .get()
            .parse::<f32>()
            .unwrap_or(13.0)
            .clamp(8.0, 32.0);
        let family = ui_font_sig.get();
        let s = s
            .size_full()
            .background(theme::bg_root())
            .color(theme::text_normal())
            .font_size(fs);
        if family.trim().is_empty() {
            s
        } else {
            s.font_family(family)
        }
    })
    .on_event_cont(EventListener::PointerMove, {
        let app_for_split = app.clone();
        move |e| {
            if let Event::PointerMove(p) = e {
                let target = app_for_split.splitter_drag.get_untracked();
                if let Some(target) = target {
                    let x = p.pos.x as f32;
                    match target {
                        SplitterTarget::Tabs => {
                            let w = x.clamp(120.0, 600.0);
                            app_for_split.settings.tabs_width.set(format!("{:.0}", w));
                        }
                        SplitterTarget::Tree => {
                            let tabs_w = app_for_split
                                .settings
                                .tabs_width
                                .get_untracked()
                                .parse::<f32>()
                                .unwrap_or(220.0);
                            // 4px 程度のスプリッタ自身の幅も考慮 (≒5)
                            let w = (x - tabs_w - 5.0).clamp(120.0, 600.0);
                            app_for_split.settings.tree_width.set(format!("{:.0}", w));
                        }
                    }
                }
            }
        }
    })
    .on_event_cont(EventListener::PointerUp, {
        let app_for_split = app.clone();
        move |_| {
            if app_for_split.splitter_drag.get_untracked().is_some() {
                app_for_split.splitter_drag.set(None);
            }
            if app_for_split.pane_splitter_drag.get_untracked().is_some() {
                app_for_split.pane_splitter_drag.set(None);
                // ratios 変更を永続化 (ドラッグ中は重いので終了時に 1 回だけ保存)
                let tabs_v = app_for_split.tabs.get_untracked();
                let layouts: Vec<String> = tabs_v
                    .iter()
                    .map(|t| {
                        let saved = t.root.with_untracked(|r| r.to_saved());
                        serde_json::to_string(&saved).unwrap_or_default()
                    })
                    .collect();
                app_for_split.settings.tab_layouts.set(layouts);
                if let Err(e) = app_for_split.settings.save() {
                    eprintln!("[settings] pane splitter save error: {}", e);
                }
            }
        }
    })
    .on_event_cont(EventListener::PointerLeave, {
        let app_for_leave = app.clone();
        move |_e| {
            // ── #D Phase 2: OLE D&D 起動トリガ ──
            // window root から PointerLeave が出た瞬間に、ドラッグ中なら
            // OLE DoDragDrop を起動してエクスプローラ等へ受け渡す (UI スレッド同期)。
            let Some(ds) = app_for_leave.dragging.get_untracked() else {
                return;
            };
            if !ds.active || ds.paths.is_empty() {
                return;
            }
            #[cfg(windows)]
            {
                if !fastfiler_domain::ole_dnd::is_ole_available() {
                    crate::flog!("[ole-dnd] skip: OleInitialize 未成功");
                    return;
                }
                trigger_external_drag(&app_for_leave, ds);
            }
            #[cfg(not(windows))]
            {
                let _ = ds;
            }
        }
    })
    .on_event_stop(EventListener::WindowResized, {
        let settings = app.settings.clone();
        move |e| {
            if let Event::WindowResized(sz) = e {
                settings.window_w.set(Some(sz.width.max(0.0) as u32));
                settings.window_h.set(Some(sz.height.max(0.0) as u32));
                persist_window_state(&settings);
            }
        }
    })
    .on_event_stop(EventListener::WindowMoved, {
        let settings = app.settings.clone();
        move |e| {
            if let Event::WindowMoved(p) = e {
                settings.window_x.set(Some(p.x as i32));
                settings.window_y.set(Some(p.y as i32));
                persist_window_state(&settings);
            }
        }
    })
    .on_event_stop(EventListener::WindowMaximizeChanged, {
        let settings = app.settings.clone();
        move |e| {
            if let Event::WindowMaximizeChanged(m) = e {
                settings.window_maximized.set(*m);
                persist_window_state(&settings);
            }
        }
    })
    .on_event_cont(EventListener::WindowGotFocus, {
        let app_for_dnd_reg = app.clone();
        move |_e| {
            #[cfg(windows)]
            {
                ensure_drop_target_registered(&app_for_dnd_reg);
                ensure_right_drag_hook_installed(&app_for_dnd_reg);
            }
            #[cfg(not(windows))]
            {
                let _ = &app_for_dnd_reg;
            }
        }
    })
    .on_event(EventListener::KeyDown, {
        let app = app.clone();
        move |e| {
            if let Event::KeyDown(ke) = e {
                if let Some(action) = crate::hotkeys::resolve_action(&app, ke) {
                    if crate::hotkeys::dispatch_action(&app, &action) {
                        return EventPropagation::Stop;
                    }
                }
            }
            EventPropagation::Continue
        }
    })
}

// ────────────────────────────────────────────────────────────────
// OLE D&D (送信側) — PointerLeave で起動
// ────────────────────────────────────────────────────────────────

#[cfg(windows)]
fn trigger_external_drag(app: &AppState, ds: crate::state::DragState) {
    use fastfiler_domain::ole_dnd::{start_drag, DragOutcome, DragRequest, PreferredEffect};

    // 修飾キーで Copy / Move を切り替える。
    // Ctrl 押下 → Copy / それ以外 (Shift / 修飾なし) → Move
    // Shift+Ctrl は Ctrl 優先 (Win 標準と一致)。
    // 修飾なしは Move を推奨ヒントとして渡す。シェルが拒否すれば Copy にフォールバックされる。
    let preferred = unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL};
        let ctrl = (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;
        if ctrl {
            PreferredEffect::Copy
        } else {
            PreferredEffect::Move
        }
    };

    let source_pane = ds.source_pane;
    let paths = ds.paths.clone();
    crate::flog!(
        "[ole-dnd] start_drag begin: paths={} preferred={:?}",
        paths.len(),
        preferred
    );

    let req = DragRequest {
        paths: paths.clone(),
        preferred,
    };
    let outcome = start_drag(req);

    // DoDragDrop は modal なので戻り次第クリア。
    app.dragging.set(None);
    crate::ui::spring::disarm(app);

    match &outcome {
        Ok(DragOutcome::None) => {
            crate::flog!("[ole-dnd] outcome: None (ドロップ拒否)");
        }
        Ok(DragOutcome::Copy) => {
            crate::flog!("[ole-dnd] outcome: Copy 成功");
        }
        Ok(DragOutcome::Move { delete_source }) => {
            crate::flog!("[ole-dnd] outcome: Move delete_source={}", delete_source);
            if *delete_source {
                let (ok, ng) = delete_sources(&paths);
                crate::flog!("[ole-dnd] source delete OK={} NG={}", ok, ng);
                if let Some(sp) = app.find_pane(source_pane) {
                    sp.status_msg
                        .set(format!("外部移動: 元削除 OK={} / NG={}", ok, ng));
                }
            } else if let Some(sp) = app.find_pane(source_pane) {
                sp.status_msg
                    .set("外部移動: 結果不明のため元削除をスキップしました".to_string());
            }
        }
        Ok(DragOutcome::Cancel) => {
            crate::flog!("[ole-dnd] outcome: Cancel (ESC)");
        }
        Ok(DragOutcome::Error(msg)) => {
            crate::flog!("[ole-dnd] outcome: Error {}", msg);
            if let Some(sp) = app.find_pane(source_pane) {
                sp.status_msg.set(format!("外部 D&D エラー: {}", msg));
            }
        }
        Err(e) => {
            crate::flog!("[ole-dnd] start_drag err: {}", e);
            if let Some(sp) = app.find_pane(source_pane) {
                sp.status_msg.set(format!("外部 D&D エラー: {}", e));
            }
        }
    }

    // outcome に関わらず source pane を常に reload する。
    // シェル (Explorer) がターゲットの場合、CFSTR_PERFORMEDDROPEFFECT を返さず
    // pdwEffect=NONE で DoDragDrop が戻ることがあるが、内部では実体ファイルが
    // すでに移動/削除されているケースがある。watcher 通知を待つと体感遅延が出るので
    // 即時 reload で UI を最新化する。
    if let Some(sp) = app.find_pane(source_pane) {
        sp.reload();
    }
}

/// OLE D&D Move 成功後の元削除 (永続削除、ゴミ箱には送らない)。
///
/// シェル D&D の MOVE は target 側が destination を既に書き込んでいる前提なので、
/// source の削除はゴミ箱経由ではなく直接削除する (Explorer 互換動作)。
#[cfg(windows)]
fn delete_sources(paths: &[std::path::PathBuf]) -> (u32, u32) {
    let mut ok = 0u32;
    let mut ng = 0u32;
    for p in paths {
        let r = if p.is_dir() {
            std::fs::remove_dir_all(p)
        } else {
            std::fs::remove_file(p)
        };
        match r {
            Ok(()) => ok += 1,
            Err(e) => {
                ng += 1;
                crate::flog!("[ole-dnd] source delete err {}: {}", p.display(), e);
            }
        }
    }
    (ok, ng)
}

// ────────────────────────────────────────────────────────────────
// OLE D&D (受信側) — IDropTarget 登録 + callbacks
// ────────────────────────────────────────────────────────────────

#[cfg(windows)]
fn ensure_drop_target_registered(app: &AppState) {
    use fastfiler_domain::ole_dnd::{is_ole_available, register_drop_target, DropTargetCallbacks};
    use std::sync::atomic::{AtomicBool, Ordering};

    // 重複登録ガード (WindowGotFocus は何度も飛ぶ)
    static REGISTERED: AtomicBool = AtomicBool::new(false);
    if REGISTERED.load(Ordering::Acquire) {
        return;
    }

    if !is_ole_available() {
        crate::flog!("[ole-dnd-recv] skip: OleInitialize 未成功");
        return;
    }

    let Some(hwnd) = resolve_app_hwnd() else {
        crate::flog!("[ole-dnd-recv] HWND 未解決 (次の WindowGotFocus でリトライ)");
        return;
    };
    let hwnd_addr = hwnd.0 as usize;

    let app_for_enter = app.clone();
    let app_for_over = app.clone();
    let app_for_leave = app.clone();
    let app_for_drop = app.clone();

    let callbacks = DropTargetCallbacks {
        on_enter: Box::new(move |paths, pt, key, allowed| {
            let hwnd = windows::Win32::Foundation::HWND(hwnd_addr as *mut _);
            handle_drag_over(&app_for_enter, hwnd, paths, pt, key, allowed)
        }),
        on_over: Box::new(move |paths, pt, key, allowed| {
            let hwnd = windows::Win32::Foundation::HWND(hwnd_addr as *mut _);
            handle_drag_over(&app_for_over, hwnd, paths, pt, key, allowed)
        }),
        on_leave: Box::new(move || {
            if app_for_leave.external_drop_hover.get_untracked().is_some() {
                app_for_leave.external_drop_hover.set(None);
            }
        }),
        on_drop: Box::new(move |paths, pt, key, allowed| {
            let hwnd = windows::Win32::Foundation::HWND(hwnd_addr as *mut _);
            let effect = handle_drag_over(&app_for_drop, hwnd, paths, pt, key, allowed);
            if app_for_drop.external_drop_hover.get_untracked().is_some() {
                app_for_drop.external_drop_hover.set(None);
            }
            if effect == DROPEFFECT_NONE_U32 {
                return effect;
            }
            let win_pt = match screen_to_window(hwnd, pt) {
                Some(p) => p,
                None => return DROPEFFECT_NONE_U32,
            };
            let Some(target_id) = hit_test_pane(&app_for_drop, win_pt) else {
                return DROPEFFECT_NONE_U32;
            };
            let Some(target_pane) = app_for_drop.find_pane(target_id) else {
                return DROPEFFECT_NONE_U32;
            };
            // MK_RBUTTON = 0x0002。右ボタン D&D で受信した場合はメニューを出す
            // (内部 D&D の右ボタン経路と同じ UX, ADR 0010)。
            let is_right_button = (key & 0x0002) != 0;
            if is_right_button {
                crate::flog!(
                    "[ole-dnd-recv] drop right-button paths={} target={}",
                    paths.len(),
                    target_id
                );
                crate::ui::drop_exec::show_right_drop_menu(
                    app_for_drop.clone(),
                    target_pane,
                    None,
                    paths.to_vec(),
                    win_pt,
                );
                // 効果は呼び出し元 (Windows) に「成功」と返す。実コピー/移動は
                // メニュー選択時に行われる。allowed が COPY のみなら COPY を返す。
                return if (allowed & DROPEFFECT_MOVE_U32) != 0 {
                    DROPEFFECT_MOVE_U32
                } else {
                    DROPEFFECT_COPY_U32
                };
            }
            let ctrl = (key & 0x0008) != 0; // MK_CONTROL
            let shift = (key & 0x0004) != 0; // MK_SHIFT
            perform_external_drop(
                &app_for_drop,
                &target_pane,
                paths.to_vec(),
                ctrl,
                shift,
                allowed,
            )
        }),
    };

    match register_drop_target(hwnd, callbacks) {
        Ok(reg) => {
            *app.drop_target_reg.lock() = crate::state::DropTargetCell(Some(reg));
            REGISTERED.store(true, Ordering::Release);
            crate::flog!("[ole-dnd-recv] IDropTarget 登録成功 hwnd={:?}", hwnd.0);
        }
        Err(e) => {
            crate::flog!("[ole-dnd-recv] RegisterDragDrop 失敗: {}", e);
        }
    }
}

/// 右ボタン D&D の `WM_RBUTTONUP` フックを登録する (ADR 0011)。
/// `WindowGotFocus` の度に呼ばれるが、内部で二重登録ガードしている。
#[cfg(windows)]
fn ensure_right_drag_hook_installed(app: &AppState) {
    let Some(hwnd) = resolve_app_hwnd() else {
        crate::flog!("[right-drag-hook] HWND 未解決 (次の WindowGotFocus でリトライ)");
        return;
    };
    crate::win32::right_drag_hook::install(hwnd, app.clone());
}

#[cfg(windows)]
const DROPEFFECT_NONE_U32: u32 = 0;
#[cfg(windows)]
const DROPEFFECT_COPY_U32: u32 = 1;
#[cfg(windows)]
const DROPEFFECT_MOVE_U32: u32 = 2;

/// DragEnter/DragOver 共通: ペイン hit-test + ハイライト更新 + effect 計算。
#[cfg(windows)]
fn handle_drag_over(
    app: &AppState,
    hwnd: windows::Win32::Foundation::HWND,
    paths: &[std::path::PathBuf],
    pt: windows::Win32::Foundation::POINTL,
    _key_state: u32,
    allowed: u32,
) -> u32 {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_SHIFT};

    let Some(win_pt) = screen_to_window(hwnd, pt) else {
        if app.external_drop_hover.get_untracked().is_some() {
            app.external_drop_hover.set(None);
        }
        return DROPEFFECT_NONE_U32;
    };
    let Some(target_id) = hit_test_pane(app, win_pt) else {
        if app.external_drop_hover.get_untracked().is_some() {
            app.external_drop_hover.set(None);
        }
        return DROPEFFECT_NONE_U32;
    };
    let Some(target_pane) = app.find_pane(target_id) else {
        return DROPEFFECT_NONE_U32;
    };
    let dest_dir = target_pane.cur_path.get_untracked();

    // 修飾キーは grfKeyState (MK_CONTROL=0x0008, MK_SHIFT=0x0004) でも来るが、
    // 内部 D&D との一貫性のため GetAsyncKeyState を使う。
    let (ctrl, shift) = unsafe {
        let c = (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;
        let s = (GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0;
        (c, s)
    };
    let (is_move, _reason) = crate::ui::drag_common::compute_effect(paths, &dest_dir, ctrl, shift);
    let desired = if is_move {
        DROPEFFECT_MOVE_U32
    } else {
        DROPEFFECT_COPY_U32
    };
    let effect = desired & allowed;
    let effect_label = if (effect & DROPEFFECT_MOVE_U32) != 0 {
        "move"
    } else if (effect & DROPEFFECT_COPY_U32) != 0 {
        "copy"
    } else {
        "none"
    };
    let new_hover = if effect == DROPEFFECT_NONE_U32 {
        None
    } else {
        Some(crate::state::ExternalDropHover {
            pane_id: target_id,
            effect: effect_label,
        })
    };
    if app.external_drop_hover.get_untracked() != new_hover {
        app.external_drop_hover.set(new_hover);
    }
    effect
}

#[cfg(windows)]
fn screen_to_window(
    hwnd: windows::Win32::Foundation::HWND,
    pt: windows::Win32::Foundation::POINTL,
) -> Option<floem::kurbo::Point> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::ScreenToClient;
    let mut p = POINT { x: pt.x, y: pt.y };
    let ok = unsafe { ScreenToClient(hwnd, &mut p) };
    if !ok.as_bool() {
        return None;
    }
    Some(floem::kurbo::Point::new(p.x as f64, p.y as f64))
}

#[cfg(windows)]
fn hit_test_pane(app: &AppState, win_pt: floem::kurbo::Point) -> Option<u64> {
    let allowed: std::collections::HashSet<u64> = app
        .active_tab()
        .map(|t| t.all_panes().iter().map(|p| p.id).collect())
        .unwrap_or_default();
    app.pane_rects.with_untracked(|m| {
        m.iter()
            .filter(|(id, _)| allowed.contains(id))
            .find_map(|(id, r)| if r.contains(win_pt) { Some(*id) } else { None })
    })
}

/// Drop 確定時の fops 実行。内部 D&D の drop ロジックと同じ閾値判定で
/// 大量は jobs 経由、それ以外は同期 + Undo push。
#[cfg(windows)]
fn perform_external_drop(
    app: &AppState,
    target_pane: &crate::state::PaneState,
    paths: Vec<std::path::PathBuf>,
    ctrl: bool,
    shift: bool,
    allowed: u32,
) -> u32 {
    use fastfiler_domain::file_ops as fops;
    use fastfiler_domain::undo::{MoveItem, UndoOp};
    use std::path::PathBuf;

    let dest_dir = target_pane.cur_path.get_untracked();
    let (is_move_pref, reason) =
        crate::ui::drag_common::compute_effect(&paths, &dest_dir, ctrl, shift);
    // allowed mask で move/copy の最終決定
    let is_move = if is_move_pref {
        if (allowed & DROPEFFECT_MOVE_U32) != 0 {
            true
        } else if (allowed & DROPEFFECT_COPY_U32) != 0 {
            false
        } else {
            return DROPEFFECT_NONE_U32;
        }
    } else if (allowed & DROPEFFECT_COPY_U32) != 0 {
        false
    } else if (allowed & DROPEFFECT_MOVE_U32) != 0 {
        true
    } else {
        return DROPEFFECT_NONE_U32;
    };
    let op_label = if is_move { "移動" } else { "コピー" };
    crate::flog!(
        "[ole-dnd-recv] drop dest={} op={} reason={} files={}",
        dest_dir.display(),
        if is_move { "move" } else { "copy" },
        reason,
        paths.len()
    );

    let mut items: Vec<(PathBuf, PathBuf)> = Vec::with_capacity(paths.len());
    let mut skipped_same_dir = 0u32;
    for src in &paths {
        if is_move {
            if let Some(parent) = src.parent() {
                if parent == dest_dir.as_path() {
                    skipped_same_dir += 1;
                    continue;
                }
            }
        }
        let Some(name) = src.file_name().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        let dst = crate::fs_model::unique_dest(&dest_dir, &name);
        items.push((src.clone(), dst));
    }
    if items.is_empty() {
        if skipped_same_dir > 0 {
            target_pane.status_msg.set(format!(
                "外部 D&D {} スキップ ({} 件は同一フォルダ)",
                op_label, skipped_same_dir
            ));
        }
        return DROPEFFECT_NONE_U32;
    }

    let (total_files, total_bytes) =
        crate::core::jobs::scan_total_for_threshold(items.iter().map(|(f, _)| f.as_path()));
    let big = total_files >= crate::core::jobs::THRESHOLD_FILES
        || total_bytes >= crate::core::jobs::THRESHOLD_BYTES;
    if big {
        target_pane.status_msg.set(format!(
            "外部 D&D {} 開始 ({} 件、進捗表示中 / Undo 不可)",
            op_label,
            items.len()
        ));
        if is_move {
            app.jobs.spawn_move(items, |_ok| {});
        } else {
            app.jobs.spawn_copy(items, |_ok| {});
        }
        return if is_move {
            DROPEFFECT_MOVE_U32
        } else {
            DROPEFFECT_COPY_U32
        };
    }

    let mut ok = 0u32;
    let mut err = 0u32;
    let mut moved: Vec<MoveItem> = Vec::new();
    for (from, dst) in &items {
        let res = if is_move {
            fops::move_path(
                from.to_string_lossy().into_owned(),
                dst.to_string_lossy().into_owned(),
            )
        } else {
            fops::copy_path(
                from.to_string_lossy().into_owned(),
                dst.to_string_lossy().into_owned(),
            )
        };
        match res {
            Ok(()) => {
                ok += 1;
                if is_move {
                    moved.push(MoveItem {
                        from: from.clone(),
                        to: dst.clone(),
                    });
                }
            }
            Err(e) => {
                crate::flog!("[ole-dnd-recv] op error: {}", e);
                err += 1;
            }
        }
    }
    if !moved.is_empty() {
        app.undo_manager.lock().push(UndoOp::Move { items: moved });
    }
    let suffix = if skipped_same_dir > 0 {
        format!(" / スキップ={}", skipped_same_dir)
    } else {
        String::new()
    };
    target_pane.status_msg.set(format!(
        "外部 D&D {} OK={} / NG={}{}",
        op_label, ok, err, suffix
    ));
    target_pane.reload();
    if is_move {
        DROPEFFECT_MOVE_U32
    } else {
        DROPEFFECT_COPY_U32
    }
}

/// アプリのトップレベル HWND を取得する。
///
/// `WindowGotFocus` のタイミングで呼ばれるので `GetForegroundWindow()` が
/// 自プロセスのウィンドウを返す可能性が高い。安全策として thread/process 一致と
/// GA_ROOT/IsWindowVisible を確認する。fallback で EnumThreadWindows で探す。
#[cfg(windows)]
fn resolve_app_hwnd() -> Option<windows::Win32::Foundation::HWND> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumThreadWindows, GetAncestor, GetForegroundWindow, GetWindowThreadProcessId,
        IsWindowVisible, GA_ROOT,
    };

    let cur_pid = unsafe { GetCurrentProcessId() };
    let cur_tid = unsafe { GetCurrentThreadId() };

    unsafe {
        let fg = GetForegroundWindow();
        if !fg.0.is_null() {
            let mut pid: u32 = 0;
            let _tid = GetWindowThreadProcessId(fg, Some(&mut pid));
            if pid == cur_pid {
                let root = GetAncestor(fg, GA_ROOT);
                if !root.0.is_null() && IsWindowVisible(root).as_bool() {
                    return Some(root);
                }
            }
        }
    }

    // fallback: 現在の thread が持つ可視 top-level window
    struct Found(Option<HWND>);
    unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            let found = &mut *(lparam.0 as *mut Found);
            if IsWindowVisible(hwnd).as_bool() {
                found.0 = Some(hwnd);
                return false.into();
            }
        }
        true.into()
    }
    let mut found = Found(None);
    let lparam = LPARAM(&mut found as *mut Found as isize);
    unsafe {
        let _ = EnumThreadWindows(cur_tid, Some(cb), lparam);
    }
    found.0
}
