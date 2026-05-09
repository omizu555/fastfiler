// Phase 3 step 3: 縦型タブ (N 列) + フォルダペインの GUI 実装。
//
// 構造:
//   App
//     ├─ Sidebar      (ドライブ一覧 — グローバル, 左端)
//     ├─ TabsPanel    (縦型タブを N 列で表示 + 列数セレクタ + [+])
//     └─ ActivePane   (1 タブ = 1 PaneState)
//           ├─ Toolbar (← → ↑ ⟳ パス入力 Open)
//           ├─ FileList (virtual_stack)
//           └─ Footer (status)
//
// タブはブラウザのような上部水平タブではなく、左側に縦に並ぶ。
// ユーザーは列数 (1〜4) を選択でき、タブを行優先で N 列に分割して表示する。
// PaneState は全フィールドが RwSignal/Arc で Clone 可能。

use std::path::PathBuf;

use fastfiler_domain::fs as ffs;
use fastfiler_domain::file_ops as fops;
use floem::event::{Event, EventListener};
use floem::keyboard::{Key, NamedKey};
use floem::kurbo::{Point, Rect};
use floem::menu::{Menu, MenuItem};
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::prelude::*;
use floem::style::CursorStyle;
use floem::views::{
    button, container, dyn_container, h_stack, img, label, scroll, text, text_input,
    v_stack, virtual_stack, Decorators, VirtualDirection, VirtualItemSize,
};

mod fs_model;
mod settings;
mod state;
mod theme;
use fs_model::{initial_path, list_drives, unique_dest, FileRow, SortKey};
use settings::{settings_view, AppSettings};
use state::{AppState, DragState, ModalKind, PaneState, SplitterTarget, Tab};
use fastfiler_domain::icons as ficons;

// ────────────────────────────────────────────────────────────────
// (値型・状態は fs_model / state モジュールへ移動済)
// ────────────────────────────────────────────────────────────────

// ────────────────────────────────────────────────────────────────
// Views
// ────────────────────────────────────────────────────────────────

fn tab_button(app: AppState, tab: Tab) -> impl IntoView {
    let id = tab.id;
    let columns_sig = tab.columns;
    let active = app.active;

    let title_label = label(move || {
        // primary ペインの title を反応的に取得 (columns[0][0])
        columns_sig.with(|cols| {
            cols.head()
                .and_then(|col| {
                    col.with(|panes| {
                        panes.head().map(|p| {
                            let t = p.title.get();
                            if t.is_empty() { String::from("(root)") } else { t }
                        })
                    })
                })
                .unwrap_or_else(|| String::from("(empty)"))
        })
    })
    .style(|s| s.flex_grow(1.0).padding_horiz(8));

    let close_btn = label(|| String::from("×"))
        .style(|s| {
            s.padding_horiz(8)
                .color(theme::text_label())
                .cursor(CursorStyle::Pointer)
        })
        .on_click_stop({
            let app = app.clone();
            move |_| app.close_tab(id)
        });

    h_stack((title_label, close_btn))
        .style(move |s| {
            let is_active = active.get() == id;
            let bg = if is_active { theme::accent_select() } else { theme::bg_zebra_b() };
            s.height(28)
                .width_full()
                .items_center()
                .background(bg)
                .border(1)
                .border_color(theme::border_default())
                .cursor(CursorStyle::Pointer)
        })
        .on_click_stop(move |_| active.set(id))
}

/// 列数セレクタ (1 / 2 / 3 / 4)
fn cols_selector(app: AppState) -> impl IntoView {
    let cols = app.tab_cols;
    let settings_tab_columns = app.settings.tab_columns;
    let make_btn = move |n: usize| {
        let cols = cols;
        label(move || format!("{}", n))
            .style(move |s| {
                let active = cols.get() == n;
                let bg = if active { theme::accent_select() } else { theme::bg_header() };
                s.width(22)
                    .height(22)
                    .items_center()
                    .padding_horiz(4)
                    .background(bg)
                    .border(1)
                    .border_color(theme::border_default())
                    .cursor(CursorStyle::Pointer)
                    .color(theme::text_normal())
            })
            .on_click_stop(move |_| {
                cols.set(n);
                settings_tab_columns.set(n.to_string());
            })
    };
    h_stack((
        label(|| String::from("Cols:")).style(|s| s.padding_horiz(4).color(theme::text_dim())),
        make_btn(1),
        make_btn(2),
        make_btn(3),
        make_btn(4),
    ))
    .style(|s| s.gap(2).items_center().padding(4))
}

/// タブを N 列に行優先でチャンク分割した縦型タブパネル。
fn tabs_panel(app: AppState) -> impl IntoView {
    let tabs_sig = app.tabs;
    let cols_sig = app.tab_cols;
    let app_for_add = app.clone();

    let plus = label(|| String::from("+ New Tab"))
        .style(|s| {
            s.height(26)
                .width_full()
                .items_center()
                .padding_horiz(8)
                .color(theme::text_success())
                .cursor(CursorStyle::Pointer)
                .background(theme::bg_zebra_b())
                .border(1)
                .border_color(theme::border_default())
        })
        .on_click_stop(move |_| app_for_add.add_tab(initial_path()));

    let app_for_grid = app.clone();
    let grid = dyn_container(
        move || (tabs_sig.get(), cols_sig.get().max(1)),
        move |(tabs, cols)| {
            let app = app_for_grid.clone();
            let total = tabs.len();
            let per_col = if total == 0 { 0 } else { (total + cols - 1) / cols };
            let mut columns: Vec<floem::AnyView> = Vec::with_capacity(cols);
            for c in 0..cols {
                let start = c * per_col;
                let end = ((c + 1) * per_col).min(total);
                let mut col_items: Vec<floem::AnyView> = Vec::new();
                if start < end {
                    for t in tabs.iter().skip(start).take(end - start) {
                        col_items.push(tab_button(app.clone(), t.clone()).into_any());
                    }
                }
                let col_view = floem::views::stack_from_iter(col_items)
                    .style(|s| s.flex_col().flex_grow(1.0).gap(2));
                columns.push(container(col_view).style(|s| s.flex_grow(1.0)).into_any());
            }
            floem::views::stack_from_iter(columns)
                .style(|s| s.flex_row().gap(2).width_full())
                .into_any()
        },
    )
    .style(|s| s.flex_col().width_full());

    let header = h_stack((
        label(|| String::from("Tabs")).style(|s| s.padding(6).font_bold().flex_grow(1.0).color(theme::text_label())),
        cols_selector(app.clone()),
    ))
    .style(|s| s.items_center().gap(4).padding(2));

    // Drives セクション (TabsPanel 内に配置)
    let drives_items: Vec<floem::AnyView> = list_drives()
        .into_iter()
        .map(|d| {
            let app = app.clone();
            let d_label = d.clone();
            label(move || d_label.clone())
                .style(|s| {
                    s.height(24)
                        .padding_horiz(8)
                        .items_center()
                        .cursor(CursorStyle::Pointer)
                        .color(theme::text_normal())
                })
                .on_click_stop(move |_| {
                    if let Some(p) = app.active_pane() {
                        p.navigate(PathBuf::from(d.clone()), true);
                    }
                })
                .into_any()
        })
        .collect();
    let drives_section = v_stack((
        label(|| String::from("Drives"))
            .style(|s| s.padding_horiz(6).padding_vert(4).font_bold().color(theme::text_dim())),
        floem::views::stack_from_iter(drives_items).style(|s| s.flex_col()),
    ))
    .style(|s| {
        s.flex_col()
            .border_bottom(1)
            .border_color(theme::border_default())
    });

    let body = v_stack((header, drives_section, plus, scroll(grid).style(|s| s.flex_grow(1.0).width_full())))
        .style(|s| s.flex_col().size_full().gap(4).padding(4));

    let tabs_width_sig = app.settings.tabs_width;
    container(body).style(move |s| {
        let w = tabs_width_sig
            .get()
            .parse::<f32>()
            .unwrap_or(220.0)
            .clamp(120.0, 600.0);
        s.width(w)
            .height_full()
            .background(theme::bg_panel())
            .border_right(1)
            .border_color(theme::border_default())
    })
}

fn pane_view(pane: PaneState, app: AppState) -> impl IntoView {
    let cur_path = pane.cur_path;
    let path_input = pane.path_input;
    let rows = pane.rows;
    let stats = pane.stats;
    let selected = pane.selected;
    let anchor = pane.anchor;
    let status_msg = pane.status_msg;
    let modal_kind = pane.modal_kind;
    let modal_input = pane.modal_input;
    let sink = pane.sink.clone();
    let fs_event_signal = pane.fs_event_signal;
    let fs_change_tick = pane.fs_change_tick;

    // ファイル監視 → 自動 reload (デバウンスなしの素朴版)
    let pane_for_fs = pane.clone();
    floem::reactive::create_effect(move |_| {
        // signal を track して変化時に reload
        if fs_event_signal.get().is_some() {
            fs_change_tick.update(|n| *n = n.wrapping_add(1));
            pane_for_fs.reload();
        }
    });

    let pane_for_open = pane.clone();
    let pane_for_back = pane.clone();
    let pane_for_forward = pane.clone();
    let pane_for_up = pane.clone();
    let pane_for_reload = pane.clone();
    let pane_for_dblclick = pane.clone();
    let pane_for_addr_enter = pane.clone();
    let pane_for_newfolder = pane.clone();
    let pane_for_newfile = pane.clone();
    let pane_for_rename = pane.clone();
    let pane_for_delete = pane.clone();
    let pane_for_modal_ok = pane.clone();
    let pane_for_modal_cancel = pane.clone();
    let pane_for_modal_enter = pane.clone();
    let pane_for_keys = pane.clone();
    let pane_for_click = pane.clone();
    let pane_for_ctxmenu = pane.clone();
    let pane_for_sort_name = pane.clone();
    let pane_for_sort_size = pane.clone();
    let pane_for_sort_mtime = pane.clone();
    let sort_key_sig = pane.sort_key;
    let sort_desc_sig = pane.sort_desc;

    let app_for_split_h = app.clone();
    let app_for_split_v = app.clone();
    let app_for_close_pane = app.clone();
    let pane_id_for_close = pane.id;
    let toolbar = h_stack((
        button("←").action(move || pane_for_back.back()),
        button("→").action(move || pane_for_forward.forward()),
        button("↑").action(move || pane_for_up.up()),
        button("⟳").action(move || pane_for_reload.reload()),
        text_input(path_input)
            .style(|s| {
                s.flex_grow(1.0)
                    .padding(4)
                    .border(1)
                    .border_color(theme::border_focus())
            })
            .on_event_stop(EventListener::KeyDown, move |e| {
                if let Event::KeyDown(ke) = e {
                    if matches!(ke.key.logical_key, Key::Named(NamedKey::Enter)) {
                        let s = path_input.get();
                        let p = PathBuf::from(s.trim());
                        pane_for_addr_enter.navigate(p, true);
                    }
                }
            }),
        button("Open").action(move || {
            let s = path_input.get();
            let p = PathBuf::from(s.trim());
            pane_for_open.navigate(p, true);
        }),
        button("New Folder").action(move || pane_for_newfolder.open_new_folder_modal()),
        button("New File").action(move || pane_for_newfile.open_new_file_modal()),
        button("Rename").action(move || pane_for_rename.open_rename_modal()),
        button("Delete").action(move || pane_for_delete.delete_selected()),
        button("⊟+").action(move || app_for_split_h.split_active(false)),
        button("⊞+").action(move || app_for_split_v.split_active(true)),
        button("✕").action(move || app_for_close_pane.close_pane(pane_id_for_close)),
    ))
    .style(|s| s.gap(6).padding(6).items_center());

    // パンくずリスト (現在パスを「>」区切りでクリック可能セグメント表示)
    let pane_for_crumb = pane.clone();
    let breadcrumb = dyn_container(
        move || cur_path.get(),
        move |p: PathBuf| {
            let mut acc = PathBuf::new();
            let mut items: Vec<floem::AnyView> = Vec::new();
            let mut first = true;
            for comp in p.components() {
                let part = comp.as_os_str().to_string_lossy().into_owned();
                if part.is_empty() {
                    continue;
                }
                acc.push(comp);
                if !first {
                    items.push(
                        label(|| String::from("›"))
                            .style(|s| {
                                s.padding_horiz(4).color(theme::text_very_dim())
                            })
                            .into_any(),
                    );
                }
                first = false;
                let target = acc.clone();
                let pane_seg = pane_for_crumb.clone();
                let display = if part.ends_with('\\') || part.ends_with('/') {
                    part.trim_end_matches(|c| c == '\\' || c == '/').to_string()
                } else {
                    part
                };
                let display = if display.is_empty() { String::from("/") } else { display };
                items.push(
                    label(move || display.clone())
                        .style(|s| {
                            s.padding_horiz(4)
                                .padding_vert(2)
                                .cursor(CursorStyle::Pointer)
                                .color(theme::text_emphasis())
                        })
                        .on_click_stop(move |_| pane_seg.navigate(target.clone(), true))
                        .into_any(),
                );
            }
            container(
                floem::views::stack_from_iter(items)
                    .style(|s| s.flex_row().items_center().gap(0)),
            )
            .style(|s| s.padding_horiz(8).padding_vert(2))
            .into_any()
        },
    )
    .style(|s| {
        s.height(22)
            .width_full()
            .background(theme::bg_modal())
            .border_bottom(1)
            .border_color(theme::border_modal())
    });

    let arrow = move |k: SortKey| -> String {
        if sort_key_sig.get() == k {
            if sort_desc_sig.get() { String::from(" ▼") } else { String::from(" ▲") }
        } else {
            String::new()
        }
    };

    let header = h_stack((
        text("#").style(|s| s.width(60).padding_horiz(6).font_bold()),
        label(move || format!("Name{}", arrow(SortKey::Name)))
            .style(|s| s.flex_grow(1.0).padding_horiz(6).font_bold().cursor(CursorStyle::Pointer))
            .on_click_stop(move |_| pane_for_sort_name.click_sort(SortKey::Name)),
        label(move || format!("Size{}", arrow(SortKey::Size)))
            .style(|s| s.width(110).padding_horiz(6).font_bold().cursor(CursorStyle::Pointer))
            .on_click_stop(move |_| pane_for_sort_size.click_sort(SortKey::Size)),
        label(move || format!("Modified{}", arrow(SortKey::Modified)))
            .style(|s| s.width(140).padding_horiz(6).font_bold().cursor(CursorStyle::Pointer))
            .on_click_stop(move |_| pane_for_sort_mtime.click_sort(SortKey::Modified)),
    ))
    .style(|s| {
        s.height(24)
            .border_bottom(1)
            .border_color(theme::border_strong())
            .background(theme::bg_header())
    });

    let row_height: f64 = 22.0;

    let app_for_rows = app.clone();
    let pane_for_rows = pane.clone();
    let list = virtual_stack(
        VirtualDirection::Vertical,
        VirtualItemSize::Fixed(Box::new(move || row_height)),
        move || rows.get().enumerate(),
        move |(idx, row): &(usize, FileRow)| (*idx, row.name.clone(), row.is_dir),
        move |(idx, row): (usize, FileRow)| {
            let is_dir = row.is_dir;
            let bg_idx = idx;
            let name_for_open = row.name.clone();
            let row_name_for_drag = row.name.clone();
            let pane_dbl = pane_for_dblclick.clone();
            let pane_clk = pane_for_click.clone();
            let pane_for_drag = pane_for_rows.clone();
            let app_for_drag = app_for_rows.clone();

            // アイコン (拡張子モード: 高速 + LRU キャッシュ)
            let icon_name = row.name.clone();
            let icon = img(move || {
                let res = if is_dir {
                    ficons::folder_icon_png(false)
                } else {
                    ficons::system_icon_png(&icon_name, false, true)
                };
                res.map(|arc| (*arc).clone()).unwrap_or_default()
            })
            .style(|s| s.width(16).height(16));

            h_stack((
                container(icon).style(|s| s.width(24).items_center()),
                text(row.name).style(move |s| {
                    let s = s.flex_grow(1.0).padding_horiz(6);
                    if is_dir { s.color(theme::text_dir()) } else { s }
                }),
                text(row.size_text)
                    .style(|s| s.width(110).padding_horiz(6).color(theme::text_dim())),
                text(row.mtime_text)
                    .style(|s| s.width(140).padding_horiz(6).color(theme::text_dim())),
            ))
            .style(move |s| {
                let zebra = if bg_idx % 2 == 0 {
                    theme::bg_zebra_a()
                } else {
                    theme::bg_zebra_b()
                };
                let sel = selected.with(|s| s.contains(&bg_idx));
                let bg = if sel { theme::accent_select() } else { zebra };
                s.height(row_height)
                    .items_center()
                    .background(bg)
                    .cursor(CursorStyle::Pointer)
            })
            .on_event_cont(EventListener::PointerDown, move |e| {
                if let Event::PointerDown(p) = e {
                    if !p.button.is_primary() {
                        return;
                    }
                    let cur = pane_for_drag.cur_path.get_untracked();
                    let row_path = cur.join(&row_name_for_drag);
                    let in_sel =
                        pane_for_drag.selected.with_untracked(|s| s.contains(&bg_idx));
                    let paths: Vec<PathBuf> = if in_sel {
                        let sel = pane_for_drag.selected.get_untracked();
                        let rs = pane_for_drag.rows.get_untracked();
                        sel.iter()
                            .filter_map(|i| rs.get(*i).map(|r| cur.join(&r.name)))
                            .collect()
                    } else {
                        vec![row_path]
                    };
                    app_for_drag.dragging.set(Some(DragState {
                        source_pane: pane_for_drag.id,
                        paths,
                        start_window: None,
                        current_window: Point::ZERO,
                        active: false,
                    }));
                }
            })
            .on_click_stop(move |e| {
                let (ctrl, shift) = if let Event::PointerUp(p) = e {
                    (p.modifiers.control(), p.modifiers.shift())
                } else {
                    (false, false)
                };
                pane_clk.click_row(bg_idx, ctrl, shift);
            })
            .on_double_click_stop(move |_| {
                let cur = cur_path.get();
                let target = cur.join(&name_for_open);
                if is_dir {
                    pane_dbl.navigate(target, true);
                } else {
                    let _ = fastfiler_domain::shell::open_with_shell(
                        target.to_string_lossy().into_owned(),
                    );
                }
            })
            .context_menu({
                let pane_ctx = pane_for_ctxmenu.clone();
                move || {
                    let p_open = pane_ctx.clone();
                    let p_reveal = pane_ctx.clone();
                    let p_cut = pane_ctx.clone();
                    let p_copy = pane_ctx.clone();
                    let p_paste = pane_ctx.clone();
                    let p_rename = pane_ctx.clone();
                    let p_delete = pane_ctx.clone();
                    let p_props = pane_ctx.clone();
                    // 右クリックされた行が未選択なら単独選択にする
                    if !p_open.selected.with(|s| s.contains(&bg_idx)) {
                        p_open.click_row(bg_idx, false, false);
                    }
                    Menu::new("")
                        .entry(MenuItem::new("開く").action({
                            let p = p_open.clone();
                            move || {
                                let cur = p.cur_path.get();
                                let name =
                                    p.rows.with(|v| v.get(bg_idx).map(|r| r.name.clone()));
                                let isd =
                                    p.rows.with(|v| v.get(bg_idx).map(|r| r.is_dir).unwrap_or(false));
                                if let Some(n) = name {
                                    let target = cur.join(n);
                                    if isd {
                                        p.navigate(target, true);
                                    } else {
                                        let _ = fastfiler_domain::shell::open_with_shell(
                                            target.to_string_lossy().into_owned(),
                                        );
                                    }
                                }
                            }
                        }))
                        .entry(MenuItem::new("エクスプローラで表示").action(move || {
                            let cur = p_reveal.cur_path.get();
                            let name = p_reveal
                                .rows
                                .with(|v| v.get(bg_idx).map(|r| r.name.clone()));
                            if let Some(n) = name {
                                let target = cur.join(n);
                                let _ = fastfiler_domain::shell::reveal_in_explorer(
                                    target.to_string_lossy().into_owned(),
                                );
                            }
                        }))
                        .separator()
                        .entry(MenuItem::new("切り取り").action(move || p_cut.clipboard_write("move")))
                        .entry(MenuItem::new("コピー").action(move || p_copy.clipboard_write("copy")))
                        .entry(MenuItem::new("貼り付け").action(move || p_paste.clipboard_paste()))
                        .separator()
                        .entry(MenuItem::new("名前の変更").action(move || p_rename.open_rename_modal()))
                        .entry(MenuItem::new("削除").action(move || p_delete.delete_selected()))
                        .separator()
                        .entry(MenuItem::new("プロパティ").action(move || {
                            let cur = p_props.cur_path.get();
                            let name = p_props
                                .rows
                                .with(|v| v.get(bg_idx).map(|r| r.name.clone()));
                            if let Some(n) = name {
                                let target = cur.join(n);
                                let _ = fastfiler_domain::shell::show_properties(
                                    target.to_string_lossy().into_owned(),
                                );
                            }
                        }))
                }
            })
        },
    )
    .style(|s| s.flex_col().width_full());

    let scrollable = scroll(list).style(|s| s.width_full().flex_grow(1.0).min_height(0));

    let status = label(move || {
        let st = stats.get();
        let sel_count = selected.with(|s| s.len());
        let cnt = sink.counter.lock();
        let msg = status_msg.get();
        format!(
            "items: {}   load: {:.2} ms   selected: {}   fs-change: {}   {}",
            st.count, st.load_ms, sel_count, *cnt, msg
        )
    })
    .style(|s| {
        s.height(22)
            .padding_horiz(8)
            .items_center()
            .background(theme::bg_chrome())
            .border_top(1)
            .border_color(theme::border_default())
    });

    // モーダル (新規フォルダ / リネーム入力)
    let modal_bar = dyn_container(
        move || modal_kind.get(),
        move |kind| match kind {
            ModalKind::None => container(label(|| String::new())).style(|s| s.height(0)).into_any(),
            other => {
                let title = match &other {
                    ModalKind::NewFolder => "新規フォルダ名",
                    ModalKind::NewFile => "新規ファイル名",
                    ModalKind::Rename(_) => "新しい名前",
                    ModalKind::None => "",
                };
                let pane_ok = pane_for_modal_ok.clone();
                let pane_cancel = pane_for_modal_cancel.clone();
                let pane_enter = pane_for_modal_enter.clone();
                h_stack((
                    label(move || title.to_string())
                        .style(|s| s.padding_horiz(8).color(theme::text_normal())),
                    text_input(modal_input)
                        .style(|s| {
                            s.flex_grow(1.0)
                                .padding(4)
                                .border(1)
                                .border_color(theme::border_focus())
                        })
                        .on_event_stop(EventListener::KeyDown, move |e| {
                            if let Event::KeyDown(ke) = e {
                                match &ke.key.logical_key {
                                    Key::Named(NamedKey::Enter) => pane_enter.confirm_modal(),
                                    Key::Named(NamedKey::Escape) => pane_enter.close_modal(),
                                    _ => {}
                                }
                            }
                        }),
                    button("OK").action(move || pane_ok.confirm_modal()),
                    button("Cancel").action(move || pane_cancel.close_modal()),
                ))
                .style(|s| {
                    s.gap(6)
                        .padding(6)
                        .items_center()
                        .background(theme::bg_status())
                        .border_bottom(1)
                        .border_color(theme::border_strong())
                })
                .into_any()
            }
        },
    );

    let pane_for_xbuttons = pane.clone();
    let pane_id = pane.id;
    let app_for_rect = app.clone();
    let app_for_rect2 = app.clone();
    let app_for_move = app.clone();
    let app_for_up = app.clone();
    let app_for_focus = app.clone();
    v_stack((toolbar, breadcrumb, modal_bar, header, scrollable, status))
        .style(|s| s.size_full().flex_col())
        .on_event_cont(EventListener::PointerDown, move |_| {
            // クリックされたペインを active に
            if let Some(t) = app_for_focus.active_tab() {
                if t.locate(pane_id).is_some() {
                    t.active_pane.set(pane_id);
                }
            }
        })
        .on_resize(move |rect| {
            app_for_rect.pane_rects.update(|m| {
                let cur = m.get(&pane_id).copied().unwrap_or(Rect::ZERO);
                let new_rect = Rect::from_origin_size(cur.origin(), rect.size());
                m.insert(pane_id, new_rect);
            });
        })
        .on_move(move |pt| {
            app_for_rect2.pane_rects.update(|m| {
                let cur = m.get(&pane_id).copied().unwrap_or(Rect::ZERO);
                let new_rect = Rect::from_origin_size(pt, cur.size());
                m.insert(pane_id, new_rect);
            });
        })
        .on_event_cont(EventListener::PointerMove, move |e| {
            if let Event::PointerMove(p) = e {
                let dragging = app_for_move.dragging.get_untracked();
                let Some(_) = dragging else { return };
                let pane_origin = app_for_move
                    .pane_rects
                    .with_untracked(|m| m.get(&pane_id).map(|r| r.origin()).unwrap_or(Point::ZERO));
                let win_pt = Point::new(pane_origin.x + p.pos.x, pane_origin.y + p.pos.y);
                app_for_move.dragging.update(|d| {
                    if let Some(ds) = d {
                        if ds.source_pane != pane_id {
                            return;
                        }
                        if ds.start_window.is_none() {
                            ds.start_window = Some(win_pt);
                        }
                        ds.current_window = win_pt;
                        if !ds.active {
                            let s = ds.start_window.unwrap();
                            let dx = win_pt.x - s.x;
                            let dy = win_pt.y - s.y;
                            if (dx * dx + dy * dy).sqrt() > 5.0 {
                                ds.active = true;
                            }
                        }
                    }
                });
            }
        })
        .on_event_cont(EventListener::PointerUp, move |e| {
            if let Event::PointerUp(p) = e {
                let drag_opt = app_for_up.dragging.get_untracked();
                let Some(ds) = drag_opt else { return };
                app_for_up.dragging.set(None);
                if !ds.active || ds.source_pane != pane_id {
                    return;
                }
                let pane_origin = app_for_up
                    .pane_rects
                    .with_untracked(|m| m.get(&pane_id).map(|r| r.origin()).unwrap_or(Point::ZERO));
                let win_pt = Point::new(pane_origin.x + p.pos.x, pane_origin.y + p.pos.y);
                let copy = p.modifiers.control();
                let target_id = app_for_up.pane_rects.with_untracked(|m| {
                    m.iter()
                        .find_map(|(id, r)| if r.contains(win_pt) { Some(*id) } else { None })
                });
                let Some(target_id) = target_id else { return };
                if target_id == ds.source_pane {
                    return;
                }
                let target_pane = app_for_up.find_pane(target_id);
                let Some(tp) = target_pane else { return };
                let dest_dir = tp.cur_path.get_untracked();
                let mut ok = 0u32;
                let mut err = 0u32;
                for src in &ds.paths {
                    let name = match src.file_name() {
                        Some(n) => n.to_string_lossy().into_owned(),
                        None => {
                            err += 1;
                            continue;
                        }
                    };
                    let dst = unique_dest(&dest_dir, &name);
                    let res = if copy {
                        fops::copy_path(
                            src.to_string_lossy().into_owned(),
                            dst.to_string_lossy().into_owned(),
                        )
                    } else {
                        fops::move_path(
                            src.to_string_lossy().into_owned(),
                            dst.to_string_lossy().into_owned(),
                        )
                    };
                    match res {
                        Ok(()) => ok += 1,
                        Err(_) => err += 1,
                    }
                }
                let label = if copy { "コピー" } else { "移動" };
                tp.status_msg
                    .set(format!("D&D {} OK={} / NG={}", label, ok, err));
                tp.reload();
                if let Some(sp) = app_for_up.find_pane(ds.source_pane) {
                    sp.reload();
                }
            }
        })
        .on_event_stop(EventListener::PointerDown, move |e| {
            if let Event::PointerDown(p) = e {
                if p.button.is_x1() {
                    pane_for_xbuttons.back();
                } else if p.button.is_x2() {
                    pane_for_xbuttons.forward();
                }
            }
        })
        .on_event_stop(EventListener::KeyDown, move |e| {
            if let Event::KeyDown(ke) = e {
                let mods = &ke.modifiers;
                let ctrl = mods.control();
                let shift = mods.shift();
                match &ke.key.logical_key {
                    Key::Named(NamedKey::Delete) => {
                        pane_for_keys.delete_selected();
                        return;
                    }
                    Key::Named(NamedKey::F2) => {
                        pane_for_keys.open_rename_modal();
                        return;
                    }
                    Key::Named(NamedKey::Escape) => {
                        pane_for_keys.close_modal();
                        return;
                    }
                    Key::Character(c) if ctrl && (c == "a" || c == "A") => {
                        pane_for_keys.select_all();
                        return;
                    }
                    Key::Character(c) if ctrl && (c == "c" || c == "C") => {
                        pane_for_keys.clipboard_write("copy");
                        return;
                    }
                    Key::Character(c) if ctrl && (c == "x" || c == "X") => {
                        pane_for_keys.clipboard_write("move");
                        return;
                    }
                    Key::Character(c) if ctrl && (c == "v" || c == "V") => {
                        pane_for_keys.clipboard_paste();
                        return;
                    }
                    _ => {}
                }
                let len = rows.with(|v| v.len());
                if len == 0 {
                    return;
                }
                let cur = anchor.get().unwrap_or(0);
                let next = match &ke.key.logical_key {
                    Key::Named(NamedKey::ArrowDown) => Some((cur + 1).min(len - 1)),
                    Key::Named(NamedKey::ArrowUp) => Some(cur.saturating_sub(1)),
                    Key::Named(NamedKey::PageDown) => Some((cur + 30).min(len - 1)),
                    Key::Named(NamedKey::PageUp) => Some(cur.saturating_sub(30)),
                    Key::Named(NamedKey::Home) => Some(0),
                    Key::Named(NamedKey::End) => Some(len - 1),
                    _ => None,
                };
                if let Some(n) = next {
                    pane_for_keys.click_row(n, false, shift);
                }
            }
        })
}

fn _sidebar_unused(app: AppState) -> impl IntoView {
    let _ = app;
    label(|| String::new())
}

// ────────────────────────────────────────────────────────────────
// TreePane (フォルダツリー)
// ────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct TreeNode {
    path: PathBuf,
    name: String,
    expanded: RwSignal<bool>,
    loaded: RwSignal<bool>,
    children: RwSignal<im::Vector<TreeNode>>,
}

impl TreeNode {
    fn new(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        Self {
            path,
            name,
            expanded: RwSignal::new(false),
            loaded: RwSignal::new(false),
            children: RwSignal::new(im::Vector::new()),
        }
    }

    /// 子フォルダを 1 階層だけロード (lazy)。
    fn load_children(&self) {
        if self.loaded.get() {
            return;
        }
        let s = self.path.to_string_lossy().into_owned();
        if let Ok(dirs) = ffs::list_dirs(s, Some(false)) {
            let mut tmp: Vec<TreeNode> = dirs
                .into_iter()
                .map(|e| TreeNode::new(self.path.join(&e.name)))
                .collect();
            tmp.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            self.children.set(tmp.into_iter().collect());
        }
        self.loaded.set(true);
    }
}

fn render_tree_node(app: AppState, node: TreeNode, depth: usize) -> floem::AnyView {
    let expanded = node.expanded;
    let children = node.children;
    let path_for_nav = node.path.clone();
    let name_text = node.name.clone();

    let app_for_click = app.clone();
    let node_for_toggle = node.clone();

    let arrow = label(move || {
        if expanded.get() { String::from("▼") } else { String::from("▶") }
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
            if let Some(p) = app_for_click.active_pane() {
                p.navigate(path_for_nav.clone(), true);
            }
        });

    let indent = (depth as f32) * 14.0 + 4.0;
    let row = h_stack((arrow, name_lbl)).style(move |s| {
        s.height(22).items_center().padding_left(indent)
    });

    let app_for_kids = app.clone();
    let kids = dyn_container(
        move || (expanded.get(), children.get()),
        move |(open, kids)| {
            if !open {
                return container(label(|| String::new()))
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

fn tree_pane(app: AppState) -> impl IntoView {
    let roots: im::Vector<TreeNode> = list_drives()
        .into_iter()
        .map(|d| TreeNode::new(PathBuf::from(d)))
        .collect();
    let roots_sig = RwSignal::new(roots);

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

    let body = v_stack((header, scroll(tree).style(|s| s.flex_grow(1.0).width_full())))
        .style(|s| s.flex_col().size_full());

    let tree_width_sig = app.settings.tree_width;
    container(body).style(move |s| {
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

fn footer_bar(app: AppState) -> impl IntoView {
    let settings_open = app.settings_open;
    let active = app.active;
    let tabs = app.tabs;

    // フッター右側ステータス: アクティブペインのアイテム数
    let status = label(move || {
        let id = active.get();
        let cnt = tabs
            .get()
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.primary().stats.get().count)
            .unwrap_or(0);
        format!("items: {}", cnt)
    })
    .style(|s| s.flex_grow(1.0).padding_horiz(8).color(theme::text_dim()));

    let gear = label(|| String::from("⚙ Settings"))
        .style(|s| {
            s.height(22)
                .padding_horiz(10)
                .items_center()
                .cursor(CursorStyle::Pointer)
                .color(theme::text_normal())
                .border_left(1)
                .border_color(theme::border_default())
        })
        .on_click_stop(move |_| settings_open.set(true));

    h_stack((status, gear)).style(|s| {
        s.height(26)
            .width_full()
            .items_center()
            .background(theme::bg_chrome())
            .border_top(1)
            .border_color(theme::border_default())
    })
}

fn app_view() -> impl IntoView {
    let app = AppState::new(initial_path());
    let settings_open = app.settings_open;
    let active = app.active;

    // 設定値変化時の自動保存 (タブ列数 / タブペイン幅 / ツリーペイン幅 / open_tabs)
    {
        let settings_for_save = app.settings.clone();
        let tab_columns_sig = app.settings.tab_columns;
        let tabs_width_sig = app.settings.tabs_width;
        let tree_width_sig = app.settings.tree_width;
        floem::reactive::create_effect(move |prev: Option<()>| {
            // 全シグナルを track
            let _ = tab_columns_sig.get();
            let _ = tabs_width_sig.get();
            let _ = tree_width_sig.get();
            // 初回 (購読登録のための実行) は保存スキップ
            if prev.is_none() {
                return;
            }
            if let Err(e) = settings_for_save.save() {
                eprintln!("[settings] auto-save error: {}", e);
            }
        });
    }
    // タブ一覧 / 各タブの primary パス変化時に open_tabs を更新 + 保存
    {
        let app_for_tabs = app.clone();
        floem::reactive::create_effect(move |prev: Option<()>| {
            let tabs_v = app_for_tabs.tabs.get();
            let mut paths: Vec<String> = Vec::with_capacity(tabs_v.len());
            for t in tabs_v.iter() {
                let p = t.primary().cur_path.get();
                paths.push(p.to_string_lossy().into_owned());
            }
            app_for_tabs.settings.open_tabs.set(paths);
            if prev.is_none() {
                return;
            }
            if let Err(e) = app_for_tabs.settings.save() {
                eprintln!("[settings] tabs auto-save error: {}", e);
            }
        });
    }
    let tabs = app.tabs;

    let switcher = dyn_container(
        move || settings_open.get(),
        {
            let app = app.clone();
            move |open| {
                if open {
                    settings_view(app.settings.clone(), settings_open).into_any()
                } else {
                    let app = app.clone();
                    let tab_columns_sig = app.settings.tab_columns;
                    let app_for_panes = app.clone();
                    let show_hidden_sig = app.settings.show_hidden;
                    let active_panes = dyn_container(
                        move || {
                            let id = active.get();
                            let _setting_cols = tab_columns_sig
                                .get()
                                .parse::<usize>()
                                .unwrap_or(1)
                                .clamp(1, 4);
                            let _ = show_hidden_sig.get();
                            let tabs_v = tabs.get();
                            let active_tab = tabs_v.iter().find(|t| t.id == id).cloned()
                                .or_else(|| tabs_v.iter().next().cloned());
                            // 各列の (col_index, Vec<PaneState>) を集める
                            let layout: Vec<Vec<PaneState>> = if let Some(t) = active_tab {
                                t.columns.with(|cols| {
                                    cols.iter()
                                        .map(|col| col.with(|panes| panes.iter().cloned().collect()))
                                        .collect()
                                })
                            } else {
                                Vec::new()
                            };
                            layout
                        },
                        move |layout: Vec<Vec<PaneState>>| {
                            if layout.is_empty() || layout.iter().all(|c| c.is_empty()) {
                                return label(|| String::from("(no tab)"))
                                    .style(|s| s.size_full().padding(20))
                                    .into_any();
                            }
                            let col_count = layout.len();
                            let columns_views: Vec<floem::AnyView> = layout
                                .into_iter()
                                .enumerate()
                                .map(|(ci, panes)| {
                                    let app_for_col = app_for_panes.clone();
                                    let row_count = panes.len();
                                    let pane_views: Vec<floem::AnyView> = panes
                                        .into_iter()
                                        .enumerate()
                                        .map(|(ri, p)| {
                                            let app_for_pv = app_for_col.clone();
                                            container(pane_view(p, app_for_pv))
                                                .style(move |s| {
                                                    let s = s
                                                        .flex_grow(1.0)
                                                        .min_height(0)
                                                        .flex_basis(0)
                                                        .width_full();
                                                    if ri > 0 && row_count > 1 {
                                                        s.border_top(1)
                                                            .border_color(theme::border_default())
                                                    } else {
                                                        s
                                                    }
                                                })
                                                .into_any()
                                        })
                                        .collect();
                                    let col_view = floem::views::stack_from_iter(pane_views)
                                        .style(|s| s.flex_col().size_full());
                                    container(col_view)
                                        .style(move |s| {
                                            let s = s
                                                .flex_grow(1.0)
                                                .min_width(0)
                                                .flex_basis(0)
                                                .height_full();
                                            if ci > 0 && col_count > 1 {
                                                s.border_left(1)
                                                    .border_color(theme::border_default())
                                            } else {
                                                s
                                            }
                                        })
                                        .into_any()
                                })
                                .collect();
                            container(
                                floem::views::stack_from_iter(columns_views)
                                    .style(|s| s.flex_row().size_full()),
                            )
                            .style(|s| s.size_full())
                            .into_any()
                        },
                    )
                    .style(|s| s.flex_grow(1.0).min_height(0).flex_col());
                    let main_row = h_stack((
                        tabs_panel(app.clone()),
                        splitter(app.clone(), SplitterTarget::Tabs),
                        tree_pane(app.clone()),
                        splitter(app.clone(), SplitterTarget::Tree),
                        active_panes,
                    ))
                    .style(|s| s.flex_grow(1.0).min_height(0).width_full());
                    v_stack((main_row, footer_bar(app.clone())))
                        .style(|s| s.size_full().flex_col())
                        .into_any()
                }
            }
        },
    )
    .style(|s| s.size_full());

    container(switcher)
        .style(|s| {
            s.size_full()
                .background(theme::bg_root())
                .color(theme::text_normal())
                .font_size(13.0)
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
}

fn persist_window_state(settings: &AppSettings) {
    if let Err(e) = settings.save() {
        eprintln!("[settings] window-state save error: {}", e);
    }
}

/// 縦のドラッグ可能なスプリッタ (4px 幅)
fn splitter(app: AppState, target: SplitterTarget) -> impl IntoView {
    let drag = app.splitter_drag;
    container(label(|| String::from("")))
        .style(|s| {
            s.width(5.0)
                .height_full()
                .background(theme::border_default())
                .cursor(CursorStyle::ColResize)
        })
        .on_event_stop(EventListener::PointerDown, move |_| {
            drag.set(Some(target));
        })
}

fn main() {
    use floem::kurbo::{Point as KPoint, Size as KSize};
    use floem::window::WindowConfig;
    use settings::PersistedSettings;

    let p = PersistedSettings::load_or_default();
    let mut cfg = WindowConfig::default().title("FastFiler");
    if let (Some(w), Some(h)) = (p.window_w, p.window_h) {
        if w >= 200 && h >= 150 {
            cfg = cfg.size(KSize::new(w as f64, h as f64));
        }
    }
    if let (Some(x), Some(y)) = (p.window_x, p.window_y) {
        cfg = cfg.position(KPoint::new(x as f64, y as f64));
    }

    floem::Application::new()
        .window(move |_| app_view(), Some(cfg))
        .run();
}
