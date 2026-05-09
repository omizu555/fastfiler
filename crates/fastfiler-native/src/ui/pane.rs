// Pane view (1 タブ内 1 ペインの表示・操作・D&D・モーダル)
// main.rs から抽出 — UI ロジックの本体

use std::path::PathBuf;

use floem::event::{Event, EventListener};
use floem::keyboard::{Key, NamedKey};
use floem::kurbo::{Point, Rect};
use floem::menu::{Menu, MenuItem};
use floem::reactive::{SignalGet, SignalUpdate, SignalWith};
use floem::prelude::*;
use floem::style::CursorStyle;
use floem::views::{
    button, container, dyn_container, h_stack, img, label, scroll, text, text_input,
    v_stack, virtual_stack, Decorators, VirtualDirection, VirtualItemSize,
};

use fastfiler_domain::file_ops as fops;
use fastfiler_domain::icons as ficons;

use crate::fs_model::{unique_dest, FileRow, SortKey};
use crate::state::{AppState, DragState, ModalKind, PaneState};
use crate::theme;
pub fn pane_view(pane: PaneState, app: AppState) -> impl IntoView {
    let cur_path = pane.cur_path;
    let path_input = pane.path_input;
    let rows = pane.rows;
    let stats = pane.stats;
    let selected = pane.selected;
    let anchor = pane.anchor;
    let status_msg = pane.status_msg;
    let modal_kind = pane.modal_kind;
    let modal_input = pane.modal_input;
    let search_query = pane.search_query;
    let search_open = pane.search_open;
    let sink = pane.sink.clone();
    let fs_event_signal = floem::ext_event::create_signal_from_channel(pane.fs_rx.clone());
    let fs_change_tick = pane.fs_change_tick;

    // ファイル監視 → 自動 reload (軽量版: rows のみ差分更新)
    let pane_for_fs = pane.clone();
    let app_for_fs = app.clone();
    let pane_id_for_fs = pane.id;
    floem::reactive::create_effect(move |prev: Option<u32>| {
        // signal を track して変化時に rows のみ更新
        let v = fs_event_signal.get();
        let cur = fs_change_tick.get_untracked();
        if v.is_some() {
            let next = cur.wrapping_add(1);
            fs_change_tick.set(next);
            crate::flog!("[fs] pane={} event received, tick {}->{}",
                pane_id_for_fs, cur, next);
            pane_for_fs.refresh_rows_only();
            // ツリーペインにも変化を通知 (展開済みノードが再ロードされる)
            app_for_fs.tree_tick.update(|n| *n = n.wrapping_add(1));
        }
        prev.unwrap_or(0).wrapping_add(1)
    });

    // Everything 検索: search_open && backend=="everything" のとき、search_query 変化を
    // debounce して fastfiler_domain::everything::query を別スレッドで実行し、
    // 結果を pane.search_results に詰める。閉じる/builtin/空クエリで search_results を None に。
    {
        let pane_for_es = pane.clone();
        let cur_path_sig = pane.cur_path;
        let backend_sig = app.settings.search_backend;
        let port_sig = app.settings.everything_port;
        let scope_sig = app.settings.everything_scope;
        let req_gen_sig = pane.search_request_gen;
        let results_sig = pane.search_results;
        let status_sig = pane.status_msg;
        let search_query_sig = pane.search_query;
        let search_open_sig = pane.search_open;
        floem::reactive::create_effect(move |_prev: Option<()>| {
            let q = search_query_sig.get();
            let open = search_open_sig.get();
            let backend = backend_sig.get();
            let port_s = port_sig.get();
            let scope = scope_sig.get();
            let cwd = cur_path_sig.get();
            if !open || backend != "everything" || q.trim().is_empty() {
                if results_sig.with_untracked(|r| r.is_some()) {
                    results_sig.set(None);
                }
                return ();
            }
            let port_u: u16 = port_s.parse().unwrap_or(80);
            let scope_path: Option<String> = if scope {
                Some(cwd.to_string_lossy().into_owned())
            } else {
                None
            };
            let scope_path_for_cb = scope_path.clone();
            let gen = req_gen_sig.get_untracked().wrapping_add(1);
            req_gen_sig.set(gen);
            let req_gen_for_cb = req_gen_sig;
            let results_for_cb = results_sig;
            let status_for_cb = status_sig;
            let cb = floem::ext_event::create_ext_action(
                floem::reactive::Scope::current(),
                move |hits: Result<Vec<fastfiler_domain::everything::EverythingHit>, String>| {
                    if req_gen_for_cb.get_untracked() != gen {
                        return;
                    }
                    match hits {
                        Ok(list) => {
                            let mut v: im::Vector<FileRow> = im::Vector::new();
                            for h in list {
                                v.push_back(FileRow {
                                    name: if scope_path_for_cb.is_some() {
                                        h.name.clone()
                                    } else {
                                        h.path.clone()
                                    },
                                    is_dir: h.is_dir,
                                    size: 0,
                                    modified: 0,
                                    size_text: String::new(),
                                    mtime_text: String::new(),
                                });
                            }
                            results_for_cb.set(Some(v));
                        }
                        Err(e) => {
                            status_for_cb.set(format!("Everything: {}", e));
                            results_for_cb.set(Some(im::Vector::new()));
                        }
                    }
                },
            );
            let q_for_thread = q.clone();
            let _ = pane_for_es.id;
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(200));
                let res = fastfiler_domain::everything::query(
                    port_u,
                    &q_for_thread,
                    scope_path.as_deref(),
                    false,
                    false,
                    1000,
                );
                cb(res.map_err(|e| e.0));
            });
            ()
        });
    }

    let pane_for_up = pane.clone();
    let pane_for_reload = pane.clone();
    let pane_for_dblclick = pane.clone();
    let pane_for_addr_enter = pane.clone();
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
    let hide_toolbar_sig = app.settings.hide_pane_toolbar;
    let toolbar = h_stack((
        button("↑").action(move || pane_for_up.up()),
        button("⟳").action(move || pane_for_reload.reload()),
        button("⇔分割").action(move || app_for_split_h.split_active(false)),
        button("⇕分割").action(move || app_for_split_v.split_active(true)),
        button("✕").action(move || app_for_close_pane.close_pane(pane_id_for_close)),
    ))
    .style(move |s| {
        let s = s.gap(6).padding(6).items_center();
        if hide_toolbar_sig.get() {
            s.height(0).padding(0).hide()
        } else {
            s
        }
    });

    // パンくず + 編集モード統合 (旧 toolbar の text_input は削除し、breadcrumb をクリックで編集に切替)
    let edit_mode = floem::reactive::Scope::new().create_rw_signal(false);
    let pane_for_crumb = pane.clone();
    let pane_for_addr_enter2 = pane_for_addr_enter.clone();
    let breadcrumb = dyn_container(
        move || (edit_mode.get(), cur_path.get()),
        move |(editing, p): (bool, PathBuf)| {
            if editing {
                let path_input = path_input;
                let pane_enter = pane_for_addr_enter2.clone();
                h_stack((
                    text_input(path_input)
                        .style(|s| {
                            s.flex_grow(1.0)
                                .flex_basis(0)
                                .min_width(0)
                                .width_full()
                                .height(24)
                                .padding_horiz(8)
                                .padding_vert(4)
                                .border(1)
                                .border_color(theme::border_focus())
                                .background(theme::bg_modal())
                                .color(theme::text_normal())
                        })
                        .on_event_stop(EventListener::KeyDown, move |e| {
                            if let Event::KeyDown(ke) = e {
                                if matches!(ke.key.logical_key, Key::Named(NamedKey::Enter)) {
                                    let s = path_input.get();
                                    let p2 = PathBuf::from(s.trim());
                                    pane_enter.navigate(p2, true);
                                    edit_mode.set(false);
                                } else if matches!(ke.key.logical_key, Key::Named(NamedKey::Escape)) {
                                    edit_mode.set(false);
                                }
                            }
                        }),
                ))
                .style(|s| s.padding_horiz(4).padding_vert(2).width_full().height(28).items_center())
                .into_any()
            } else {
                let mut acc = PathBuf::new();
                let mut items: Vec<floem::AnyView> = Vec::new();
                let mut first = true;
                for comp in p.components() {
                    use std::path::Component;
                    // RootDir は Prefix で代替して描画済みなのでスキップ (旧 "/" 表示バグ修正)
                    if matches!(comp, Component::RootDir) {
                        acc.push(comp);
                        continue;
                    }
                    let part = comp.as_os_str().to_string_lossy().into_owned();
                    if part.is_empty() {
                        continue;
                    }
                    acc.push(comp);
                    if !first {
                        items.push(
                            label(|| String::from("›"))
                                .style(|s| s.padding_horiz(4).color(theme::text_very_dim()))
                                .into_any(),
                        );
                    }
                    first = false;
                    let target = acc.clone();
                    let pane_seg = pane_for_crumb.clone();
                    let display = part.trim_end_matches(|c| c == '\\' || c == '/').to_string();
                    let display = if display.is_empty() { String::from("(root)") } else { display };
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
                // 末尾余白 (クリックで編集モード)
                items.push(
                    container(label(|| String::new()))
                        .style(|s| s.flex_grow(1.0).height(20).cursor(CursorStyle::Text))
                        .into_any(),
                );
                container(
                    floem::views::stack_from_iter(items)
                        .style(|s| s.flex_row().items_center().gap(0).width_full()),
                )
                .style(|s| s.padding_horiz(4).padding_vert(1).width_full().cursor(CursorStyle::Text))
                .on_click_stop(move |_| {
                    // 入力欄の中身を最新パスに同期してから編集モードへ
                    let cur = cur_path.get_untracked();
                    path_input.set(cur.to_string_lossy().into_owned());
                    edit_mode.set(true);
                })
                .into_any()
            }
        },
    )
    .style(|s| {
        s.min_height(28)
            .min_width(0)
            .flex_grow(1.0)
            .flex_basis(0)
            .background(theme::bg_modal())
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
    let search_results_sig = pane.search_results;
    // 表示行: search_results が Some なら Everything 結果をそのまま表示 (orig_idx は擬似)。
    // None なら従来の builtin filter (search_query で部分一致)。
    let filtered_rows = move || -> im::Vector<(usize, FileRow)> {
        if let Some(ev) = search_results_sig.get() {
            return ev.iter().enumerate().map(|(i, r)| (i, r.clone())).collect();
        }
        let q = search_query.get().to_lowercase();
        let rs = rows.get();
        let mut out: im::Vector<(usize, FileRow)> = im::Vector::new();
        for (i, r) in rs.iter().enumerate() {
            if q.is_empty() || r.name.to_lowercase().contains(&q) {
                out.push_back((i, r.clone()));
            }
        }
        out
    };
    let list = virtual_stack(
        VirtualDirection::Vertical,
        VirtualItemSize::Fixed(Box::new(move || row_height)),
        filtered_rows,
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
                    crate::flog!("[drag] start pane={} row={} in_sel={} paths={}",
                        pane_for_drag.id, bg_idx, in_sel, paths.len());
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

    // 検索バー (search_open=true で表示、入力は search_query をリアルタイム反映)
    let search_bar = dyn_container(
        move || search_open.get(),
        move |open| {
            if !open {
                return container(label(|| String::new())).style(|s| s.height(0)).into_any();
            }
            let q = search_query;
            h_stack((
                label(|| String::from("🔍"))
                    .style(|s| s.padding_horiz(6).color(theme::text_dim())),
                text_input(q)
                    .style(|s| {
                        s.flex_grow(1.0)
                            .flex_basis(0)
                            .min_width(0)
                            .height(24)
                            .padding_horiz(8)
                            .padding_vert(4)
                            .border(1)
                            .border_color(theme::border_focus())
                            .background(theme::bg_modal())
                            .color(theme::text_normal())
                    })
                    .on_event_stop(EventListener::KeyDown, move |e| {
                        if let Event::KeyDown(ke) = e {
                            if matches!(ke.key.logical_key, Key::Named(NamedKey::Escape)) {
                                q.set(String::new());
                                search_open.set(false);
                            }
                        }
                    }),
                button("✕").action(move || {
                    q.set(String::new());
                    search_open.set(false);
                }),
            ))
            .style(|s| {
                s.padding_horiz(4)
                    .padding_vert(2)
                    .gap(4)
                    .items_center()
                    .width_full()
                    .height(28)
                    .background(theme::bg_status())
                    .border_bottom(1)
                    .border_color(theme::border_default())
            })
            .into_any()
        },
    )
    .style(|s| s.width_full());

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
    let top_bar = h_stack((toolbar, breadcrumb))
        .style(|s| s.width_full().items_center().border_bottom(1).border_color(theme::border_modal()));
    v_stack((top_bar, search_bar, modal_bar, header, scrollable, status))
        .style(|s| s.size_full().flex_col())
        .on_event_cont(EventListener::PointerDown, move |_| {
            // クリックされたペインを active に
            if let Some(t) = app_for_focus.active_tab() {
                if t.all_panes().iter().any(|p| p.id == pane_id) && t.active_pane.get_untracked() != pane_id {
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
                // 左ボタン以外 (戻る/進む/中ボタン等) では drop 処理しない。
                // dragging 状態は安全のためクリアする。
                if !p.button.is_primary() {
                    if app_for_up.dragging.get_untracked().is_some() {
                        crate::flog!("[drop] PointerUp non-primary button, cancel drag");
                        app_for_up.dragging.set(None);
                    }
                    return;
                }
                let drag_opt = app_for_up.dragging.get_untracked();
                let Some(ds) = drag_opt else { return };
                app_for_up.dragging.set(None);
                crate::flog!("[drop] PointerUp pane={} ds.active={} ds.source_pane={} ds.paths={}",
                    pane_id, ds.active, ds.source_pane, ds.paths.len());
                if !ds.active || ds.source_pane != pane_id {
                    crate::flog!("[drop] skip (active={}, source_match={})",
                        ds.active, ds.source_pane == pane_id);
                    return;
                }
                let pane_origin = app_for_up
                    .pane_rects
                    .with_untracked(|m| m.get(&pane_id).map(|r| r.origin()).unwrap_or(Point::ZERO));
                let win_pt = Point::new(pane_origin.x + p.pos.x, pane_origin.y + p.pos.y);
                let copy = p.modifiers.control();
                let rects_dump: Vec<(u64, Rect)> = app_for_up.pane_rects.with_untracked(|m| {
                    m.iter().map(|(k, v)| (*k, *v)).collect()
                });
                crate::flog!("[drop] win_pt=({:.1},{:.1}) copy={} pane_rects={:?}",
                    win_pt.x, win_pt.y, copy, rects_dump);
                let target_id = app_for_up.pane_rects.with_untracked(|m| {
                    let allowed: std::collections::HashSet<u64> = app_for_up
                        .active_tab()
                        .map(|t| t.all_panes().iter().map(|p| p.id).collect())
                        .unwrap_or_default();
                    m.iter()
                        .filter(|(id, _)| allowed.contains(id))
                        .find_map(|(id, r)| if r.contains(win_pt) { Some(*id) } else { None })
                });
                let Some(target_id) = target_id else {
                    crate::flog!("[drop] no target pane found at win_pt");
                    return;
                };
                crate::flog!("[drop] target_id={} (source_pane={})", target_id, ds.source_pane);
                if target_id == ds.source_pane {
                    crate::flog!("[drop] same pane, skip");
                    return;
                }
                let target_pane = app_for_up.find_pane(target_id);
                let Some(tp) = target_pane else {
                    crate::flog!("[drop] target pane not found in tabs");
                    return;
                };
                let dest_dir = tp.cur_path.get_untracked();
                crate::flog!("[drop] dest_dir={} mode={}", dest_dir.display(),
                    if copy { "COPY" } else { "MOVE" });
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
                    crate::flog!("[drop] {} src={} dst={}",
                        if copy { "copy_path" } else { "move_path" },
                        src.display(), dst.display());
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
                        Err(e) => { crate::flog!("[drop] op error: {}", e); err += 1; }
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
                let shift = mods.shift();
                // Delete/F2/Ctrl+A/C/X/V 等のアクション系はルートの hotkeys に委譲。
                // ここではフォーカス中ペイン内のナビゲーション系 (Arrow/Page/Home/End)
                // と Escape (モーダル閉じ) のみハンドル。
                if matches!(ke.key.logical_key, Key::Named(NamedKey::Escape)) {
                    pane_for_keys.close_modal();
                    return;
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

