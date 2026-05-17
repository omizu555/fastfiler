// Pane view (1 タブ内 1 ペインの表示・操作・D&D・モーダル)
// main.rs から抽出 — UI ロジックの本体

use std::path::PathBuf;

use floem::event::{Event, EventListener};
use floem::keyboard::{Key, NamedKey};
use floem::kurbo::{Point, Rect};
use floem::menu::{Menu, MenuItem};
use floem::prelude::*;
use floem::reactive::{SignalGet, SignalUpdate, SignalWith};
use floem::style::CursorStyle;
use floem::views::{
    button, container, dyn_container, h_stack, img, label, scroll, text, text_input, v_stack,
    virtual_stack, Decorators, VirtualDirection, VirtualItemSize,
};

use fastfiler_domain::file_ops as fops;
use fastfiler_domain::icons as ficons;

use crate::fs_model::{unique_dest, FileRow, SortKey};
use crate::state::{AppState, DragState, ModalKind, PaneState};
use crate::theme;

/// 拡張子からカテゴリ別の絵文字を返す (icon_set=emoji 用)。
fn ext_emoji(name: &str, is_dir: bool) -> String {
    if is_dir {
        return "📁".to_string();
    }
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let s = match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "ico" | "tiff" => "🖼",
        "mp4" | "mkv" | "mov" | "avi" | "webm" | "wmv" | "flv" | "m4v" => "🎞",
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "wma" => "🎵",
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" => "🗜",
        "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "go" | "c" | "cpp" | "h" | "hpp" | "java"
        | "cs" | "rb" | "php" | "sh" | "ps1" | "lua" => "💻",
        "pdf" => "📕",
        "doc" | "docx" | "odt" => "📘",
        "xls" | "xlsx" | "csv" => "📗",
        "ppt" | "pptx" => "📙",
        "txt" | "md" | "log" | "rst" => "📝",
        "exe" | "msi" | "bat" | "cmd" => "⚙",
        "json" | "yaml" | "yml" | "toml" | "xml" | "ini" => "🔧",
        _ => "📄",
    };
    s.to_string()
}

fn elide_for_width(input: &str, col_width: f32, padding_px: f32) -> String {
    let sanitized = input.replace(['\r', '\n', '\t'], " ");
    let usable = (col_width - padding_px * 2.0).max(0.0);
    if usable < 12.0 {
        return String::new();
    }
    // フォント差異で折り返しが起きにくいよう、幅見積もりは保守的に取る。
    let ellipsis_px = 12.0f32;
    let budget_px = (usable - ellipsis_px).max(0.0);
    if budget_px <= 0.0 {
        return String::new();
    }

    let mut used = 0.0f32;
    let mut out = String::new();
    let mut truncated = false;
    for ch in sanitized.chars() {
        let ch_px = if ch.is_ascii() { 9.0 } else { 18.0 };
        if used + ch_px > budget_px {
            truncated = true;
            break;
        }
        out.push(ch);
        used += ch_px;
    }

    if !truncated {
        return sanitized;
    }
    let out = out.trim_end().to_string();
    if out.is_empty() {
        return String::from("…");
    }
    format!("{}…", out)
}

#[derive(Clone, Copy)]
enum ColumnResizeTarget {
    Name,
    Size,
}

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
    let name_col_width_sig = pane.name_col_width;
    let size_col_width_sig = pane.size_col_width;
    let mtime_col_width_sig = pane.mtime_col_width;
    let col_resize_drag = floem::reactive::Scope::new()
        .create_rw_signal(None::<(ColumnResizeTarget, f64, f32, f32, f32)>);
    // ドラッグ候補 (PointerDown 時の文脈): (source_pane_id, paths, start_pos_in_pane).
    // PointerMove で閾値を超えるまでは dragging に乗せず、誤発火を防ぐ。
    let drag_candidate =
        floem::reactive::Scope::new().create_rw_signal(None::<(u64, Vec<PathBuf>, Point)>);
    let pane_width_sig = floem::reactive::Scope::new().create_rw_signal(0.0f32);
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
            crate::flog!(
                "[fs] pane={} event received, tick {}->{}",
                pane_id_for_fs,
                cur,
                next
            );
            pane_for_fs.refresh_rows_only();
            // ツリーペインにも変化を通知 (展開済みノードが再ロードされる)
            app_for_fs.tree_tick.update(|n| *n = n.wrapping_add(1));
        }
        prev.unwrap_or(0).wrapping_add(1)
    });

    // Everything 検索 effect (search/mod.rs に分離)
    crate::search::attach_everything_effect(&pane, &app);

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
    let search_open_for_btn = pane.search_open;
    let search_query_for_btn = pane.search_query;
    let search_results_for_btn = pane.search_results;
    let hide_toolbar_sig = app.settings.hide_pane_toolbar;
    let toolbar = h_stack((
        button("↑").action(move || pane_for_up.up()),
        button("⟳").action(move || pane_for_reload.reload()),
        button("🔍").action(move || {
            let cur = search_open_for_btn.get_untracked();
            if cur {
                search_query_for_btn.set(String::new());
                search_results_for_btn.set(None);
                search_open_for_btn.set(false);
            } else {
                search_open_for_btn.set(true);
            }
        }),
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
                h_stack((text_input(path_input)
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
                    }),))
                .style(|s| {
                    s.padding_horiz(4)
                        .padding_vert(2)
                        .width_full()
                        .height(28)
                        .items_center()
                })
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
                    let display = if display.is_empty() {
                        String::from("(root)")
                    } else {
                        display
                    };
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
                .style(|s| {
                    s.padding_horiz(4)
                        .padding_vert(1)
                        .width_full()
                        .cursor(CursorStyle::Text)
                })
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
            if sort_desc_sig.get() {
                String::from(" ▼")
            } else {
                String::from(" ▲")
            }
        } else {
            String::new()
        }
    };

    let header_search_open = pane.search_open;
    let app_for_resize_name = app.clone();
    let app_for_resize_size = app.clone();
    let pane_id_for_resize = pane.id;
    let col_resize_for_name = col_resize_drag;
    let col_resize_for_size = col_resize_drag;
    let name_w_sig_for_name = name_col_width_sig;
    let size_w_sig_for_name = size_col_width_sig;
    let mtime_w_sig_for_name = mtime_col_width_sig;
    let name_w_sig_for_size = name_col_width_sig;
    let size_w_sig_for_size = size_col_width_sig;
    let mtime_w_sig_for_size = mtime_col_width_sig;
    let header = h_stack((
        text("#").style(|s| s.width(60).padding_horiz(6).font_bold()),
        label(move || format!("Name{}", arrow(SortKey::Name)))
            .style(move |s| {
                let w = name_col_width_sig.get().clamp(24.0, 1200.0);
                s.width(w)
                    .padding_horiz(6)
                    .font_bold()
                    .cursor(CursorStyle::Pointer)
            })
            .on_click_stop(move |_| pane_for_sort_name.click_sort(SortKey::Name)),
        container(label(|| String::new()))
            .style(|s| {
                s.width(5.0)
                    .height_full()
                    .cursor(CursorStyle::ColResize)
                    .background(theme::border_default())
            })
            .on_event_stop(EventListener::PointerDown, move |e| {
                if let Event::PointerDown(_p) = e {
                    if let Some(t) = app_for_resize_name.active_tab() {
                        if t.active_pane.get_untracked() != pane_id_for_resize {
                            t.active_pane.set(pane_id_for_resize);
                        }
                    }
                    let n = name_w_sig_for_name.get_untracked().clamp(24.0, 1200.0);
                    let s = size_w_sig_for_name.get_untracked().clamp(24.0, 600.0);
                    let m = mtime_w_sig_for_name.get_untracked().clamp(24.0, 600.0);
                    let start_x = 60.0 + n as f64 + 2.5;
                    col_resize_for_name.set(Some((ColumnResizeTarget::Name, start_x, n, s, m)));
                }
            }),
        label(move || format!("Size{}", arrow(SortKey::Size)))
            .style(move |s| {
                let w = size_col_width_sig.get().clamp(24.0, 600.0);
                s.width(w)
                    .padding_horiz(6)
                    .font_bold()
                    .cursor(CursorStyle::Pointer)
            })
            .on_click_stop(move |_| pane_for_sort_size.click_sort(SortKey::Size)),
        container(label(|| String::new()))
            .style(|s| {
                s.width(5.0)
                    .height_full()
                    .cursor(CursorStyle::ColResize)
                    .background(theme::border_default())
            })
            .on_event_stop(EventListener::PointerDown, move |e| {
                if let Event::PointerDown(_p) = e {
                    if let Some(t) = app_for_resize_size.active_tab() {
                        if t.active_pane.get_untracked() != pane_id_for_resize {
                            t.active_pane.set(pane_id_for_resize);
                        }
                    }
                    let n = name_w_sig_for_size.get_untracked().clamp(24.0, 1200.0);
                    let s = size_w_sig_for_size.get_untracked().clamp(24.0, 600.0);
                    let m = mtime_w_sig_for_size.get_untracked().clamp(24.0, 600.0);
                    let start_x = 60.0 + n as f64 + 5.0 + s as f64 + 2.5;
                    col_resize_for_size.set(Some((ColumnResizeTarget::Size, start_x, n, s, m)));
                }
            }),
        label(move || format!("Modified{}", arrow(SortKey::Modified)))
            .style(move |s| {
                let w = mtime_col_width_sig.get().clamp(24.0, 600.0);
                s.width(w)
                    .padding_horiz(6)
                    .font_bold()
                    .cursor(CursorStyle::Pointer)
            })
            .on_click_stop(move |_| pane_for_sort_mtime.click_sort(SortKey::Modified)),
        label(|| String::from("Path")).style(move |s| {
            let s = s.padding_horiz(6).font_bold();
            if header_search_open.get() {
                s.flex_grow(1.0).flex_basis(0).min_width(0)
            } else {
                s.width(0).hide()
            }
        }),
    ))
    .style(|s| {
        s.height(24)
            .width_full()
            .min_width(0)
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
        // search_open を依存に入れて、検索バー開閉時に list を再構築させる
        // (path 列の出し入れを反映するため)
        let _ = search_open.get();
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
            let row_name_for_drag_style = row.name.clone();
            let pane_dbl = pane_for_dblclick.clone();
            let pane_clk = pane_for_click.clone();
            let pane_for_drag = pane_for_rows.clone();
            let app_for_drag = app_for_rows.clone();
            let pane_for_drag_style = pane_for_rows.clone();
            let app_for_drag_style = app_for_rows.clone();
            let drag_candidate_for_row = drag_candidate;
            let drag_candidate_for_click = drag_candidate;
            let drag_candidate_for_row_up = drag_candidate;
            let app_for_click_clear = app_for_rows.clone();
            let app_for_row_pointer_up = app_for_rows.clone();
            let app_for_row_pointer_down_clear = app_for_rows.clone();
            let pane_for_click_log = pane_for_rows.clone();
            let _ = &app_for_drag;

            // アイコン: icon_set 設定で表示形式を切替 (theme_rev で再構築されるので
            // get_untracked で OK)
            let icon_name = row.name.clone();
            let icon_name_for_emoji = row.name.clone();
            let name_raw = row.name.clone();
            let size_raw = row.size_text.clone();
            let mtime_raw = row.mtime_text.clone();
            let icon_set = app_for_rows.settings.icon_set.get_untracked();
            let icon: floem::AnyView = if icon_set == "emoji" {
                let s = ext_emoji(&icon_name_for_emoji, is_dir);
                label(move || s.clone())
                    .style(|s| s.width(20).font_size(14.0).items_center())
                    .into_any()
            } else if icon_set == "minimal" {
                let g = if is_dir { "▸" } else { "·" };
                label(move || g.to_string())
                    .style(|s| s.width(20).color(theme::text_dim()).items_center())
                    .into_any()
            } else {
                img(move || {
                    let res = if is_dir {
                        ficons::folder_icon_png(false)
                    } else {
                        ficons::system_icon_png(&icon_name, false, true)
                    };
                    res.map(|arc| (*arc).clone()).unwrap_or_default()
                })
                .style(|s| s.width(16).height(16))
                .into_any()
            };

            let show_path_col = pane_for_rows.search_open.get_untracked();
            let path_text = if show_path_col {
                row.full_path.clone().unwrap_or_else(|| {
                    pane_for_rows
                        .cur_path
                        .get_untracked()
                        .join(&row.name)
                        .to_string_lossy()
                        .into_owned()
                })
            } else {
                String::new()
            };
            let path_label: floem::AnyView = if show_path_col {
                container(label(move || path_text.clone()).style(|s| {
                    s.color(theme::text_dim())
                        .min_width(0)
                        .cursor(CursorStyle::Pointer)
                }))
                .style(|s| {
                    s.flex_grow(1.0)
                        .flex_basis(0)
                        .min_width(0)
                        .height(22)
                        .padding_horiz(6)
                        .items_center()
                })
                .into_any()
            } else {
                container(label(|| String::new()))
                    .style(|s| s.width(0))
                    .into_any()
            };

            h_stack((
                container(
                    h_stack((container(icon).style(|s| s.width(24).items_center()),))
                        .style(|s| s.width_full().height(22).padding_horiz(6).items_center()),
                )
                .style(|s| s.width(60).height(22).items_center()),
                container(
                    label(move || {
                        elide_for_width(
                            &name_raw,
                            name_col_width_sig.get().clamp(24.0, 1200.0),
                            6.0,
                        )
                    })
                    .style(move |s| {
                        let s = s
                            .min_width(0)
                            .cursor(CursorStyle::Pointer)
                            .selectable(false);
                        if is_dir {
                            s.color(theme::text_dir())
                        } else {
                            s
                        }
                    }),
                )
                .style(move |s| {
                    s.width(name_col_width_sig.get().clamp(24.0, 1200.0))
                        .min_width(0)
                        .height(22)
                        .padding_horiz(6)
                        .items_center()
                }),
                container(label(|| String::new())).style(|s| s.width(5.0).height(22)),
                container(
                    label(move || {
                        elide_for_width(&size_raw, size_col_width_sig.get().clamp(24.0, 600.0), 6.0)
                    })
                    .style(|s| {
                        s.color(theme::text_dim())
                            .cursor(CursorStyle::Pointer)
                            .selectable(false)
                    }),
                )
                .style(move |s| {
                    s.width(size_col_width_sig.get().clamp(24.0, 600.0))
                        .height(22)
                        .padding_horiz(6)
                        .items_center()
                }),
                container(label(|| String::new())).style(|s| s.width(5.0).height(22)),
                container(
                    label(move || {
                        elide_for_width(
                            &mtime_raw,
                            mtime_col_width_sig.get().clamp(24.0, 600.0),
                            6.0,
                        )
                    })
                    .style(|s| {
                        s.color(theme::text_dim())
                            .cursor(CursorStyle::Pointer)
                            .selectable(false)
                    }),
                )
                .style(move |s| {
                    s.width(mtime_col_width_sig.get().clamp(24.0, 600.0))
                        .height(22)
                        .padding_horiz(6)
                        .items_center()
                }),
                path_label,
            ))
            .style(move |s| {
                let zebra = if bg_idx % 2 == 0 {
                    theme::bg_zebra_a()
                } else {
                    theme::bg_zebra_b()
                };
                let sel = selected.with(|s| s.contains(&bg_idx));
                let drag_picked = app_for_drag_style
                    .dragging
                    .get()
                    .map(|ds| {
                        if !ds.active || ds.source_pane != pane_for_drag_style.id {
                            return false;
                        }
                        let cur = pane_for_drag_style.cur_path.get_untracked();
                        let row_path = cur.join(&row_name_for_drag_style);
                        ds.paths.iter().any(|p| p == &row_path)
                    })
                    .unwrap_or(false);
                let bg = if sel || drag_picked {
                    theme::accent_select()
                } else {
                    zebra
                };
                let row_style = s
                    .height(row_height)
                    .width_full()
                    .min_width(0)
                    .items_center()
                    .background(bg)
                    .cursor(CursorStyle::Pointer);
                if sel || drag_picked {
                    // 選択中/掴み中はホバーで色を変えない（accent を維持）。
                    row_style
                } else {
                    row_style.hover(|s| s.background(theme::bg_hover()))
                }
            })
            .on_event_stop(EventListener::PointerDown, move |e| {
                if let Event::PointerDown(p) = e {
                    if !p.button.is_primary() {
                        return;
                    }
                    // 前回のドラッグ状態が残っていれば、ここで必ずリセット。
                    // (前回 PointerUp が捕捉漏れしていた保険)
                    if app_for_row_pointer_down_clear
                        .dragging
                        .get_untracked()
                        .is_some()
                    {
                        crate::flog!("[drag] stale dragging cleared on new PointerDown");
                        app_for_row_pointer_down_clear.dragging.set(None);
                    }
                    let cur = pane_for_drag.cur_path.get_untracked();
                    let row_path = cur.join(&row_name_for_drag);
                    let in_sel = pane_for_drag
                        .selected
                        .with_untracked(|s| s.contains(&bg_idx));
                    // 選択外の行で押下 (修飾キーなし) → エクスプローラ同様その行に選択を切替。
                    // これでクリック済みの別ファイルと併せて「2 個選択に見える」現象を防ぐ。
                    if !in_sel && !p.modifiers.control() && !p.modifiers.shift() {
                        let mut s = im::OrdSet::new();
                        s.insert(bg_idx);
                        pane_for_drag.selected.set(s);
                        pane_for_drag.anchor.set(Some(bg_idx));
                    }
                    let paths: Vec<PathBuf> = if in_sel {
                        let sel = pane_for_drag.selected.get_untracked();
                        let rs = pane_for_drag.rows.get_untracked();
                        sel.iter()
                            .filter_map(|i| rs.get(*i).map(|r| cur.join(&r.name)))
                            .collect()
                    } else {
                        vec![row_path]
                    };
                    crate::flog!(
                        "[drag] candidate pane={} row={} in_sel={} paths={}",
                        pane_for_drag.id,
                        bg_idx,
                        in_sel,
                        paths.len()
                    );
                    // ここでは dragging を作らない。閾値を超えた PointerMove で初めて作る。
                    drag_candidate_for_row.set(Some((pane_for_drag.id, paths, p.pos)));
                }
            })
            .on_event_cont(EventListener::PointerUp, move |_| {
                // 安全網: クリック判定にならず、かつペイン側 PointerUp にも届かない
                // 経路があり得るので、行レベルでも必ず drag 関連状態を解除する。
                // (同一ペイン内で離した時に drag 状態が残る現象への対策)
                if drag_candidate_for_row_up.get_untracked().is_some() {
                    drag_candidate_for_row_up.set(None);
                }
                if app_for_row_pointer_up.dragging.get_untracked().is_some() {
                    crate::flog!("[drag] cleared by row PointerUp safety net");
                    app_for_row_pointer_up.dragging.set(None);
                }
            })
            .on_click_stop(move |e| {
                // floem では Click ハンドラが Stop を返すと PointerUp リスナが消費される。
                // そのためペイン側 PointerUp で行う候補クリアがここに届かない経路がある。
                // クリック確定時点で必ず drag 関連状態を掃除する。
                drag_candidate_for_click.set(None);
                if app_for_click_clear.dragging.get_untracked().is_some() {
                    app_for_click_clear.dragging.set(None);
                }
                let (ctrl, shift) = if let Event::PointerUp(p) = e {
                    (p.modifiers.control(), p.modifiers.shift())
                } else {
                    (false, false)
                };
                crate::flog!(
                    "[click] row pane={} idx={} ctrl={} shift={}",
                    pane_for_click_log.id,
                    bg_idx,
                    ctrl,
                    shift
                );
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
                                let name = p.rows.with(|v| v.get(bg_idx).map(|r| r.name.clone()));
                                let isd = p
                                    .rows
                                    .with(|v| v.get(bg_idx).map(|r| r.is_dir).unwrap_or(false));
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
                        .entry(
                            MenuItem::new("切り取り").action(move || p_cut.clipboard_write("move")),
                        )
                        .entry(
                            MenuItem::new("コピー").action(move || p_copy.clipboard_write("copy")),
                        )
                        .entry(MenuItem::new("貼り付け").action(move || p_paste.clipboard_paste()))
                        .separator()
                        .entry(
                            MenuItem::new("名前の変更")
                                .action(move || p_rename.open_rename_modal()),
                        )
                        .entry(MenuItem::new("削除").action(move || p_delete.delete_selected()))
                        .separator()
                        .entry(MenuItem::new("プロパティ").action(move || {
                            let cur = p_props.cur_path.get();
                            let name = p_props.rows.with(|v| v.get(bg_idx).map(|r| r.name.clone()));
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
                return container(label(|| String::new()))
                    .style(|s| s.height(0))
                    .into_any();
            }
            let q = search_query;
            let results_clear = pane.search_results;
            h_stack((
                label(|| String::from("🔍")).style(|s| s.padding_horiz(6).color(theme::text_dim())),
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
                                results_clear.set(None);
                                search_open.set(false);
                            }
                        }
                    }),
                button("✕").action(move || {
                    q.set(String::new());
                    results_clear.set(None);
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
            ModalKind::None => container(label(|| String::new()))
                .style(|s| s.height(0))
                .into_any(),
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
    let app_for_active_style = app.clone();
    let app_for_drag_style = app.clone();
    let name_w_sig_for_drag = name_col_width_sig;
    let size_w_sig_for_drag = size_col_width_sig;
    let name_w_sig_for_drag_read = name_col_width_sig;
    let size_w_sig_for_drag_read = size_col_width_sig;
    let mtime_w_sig_for_drag_read = mtime_col_width_sig;
    let pane_width_for_drag = pane_width_sig;
    let col_resize_for_drag = col_resize_drag;
    let col_resize_for_drag_end = col_resize_drag;
    let drag_candidate_for_move = drag_candidate;
    let drag_candidate_for_up = drag_candidate;
    let top_bar = h_stack((toolbar, breadcrumb)).style(|s| {
        s.width_full()
            .items_center()
            .border_bottom(1)
            .border_color(theme::border_modal())
    });
    v_stack((top_bar, search_bar, modal_bar, header, scrollable, status))
        .style(move |s| {
            let is_active = app_for_active_style
                .active_tab()
                .map(|t| t.active_pane.get() == pane_id)
                .unwrap_or(false);
            let drag_state = app_for_drag_style.dragging.get();
            let (is_drag_source, is_drag_target) = if let Some(ds) = drag_state {
                if !ds.active {
                    (false, false)
                } else {
                    let is_source = ds.source_pane == pane_id;
                    let is_target = app_for_drag_style
                        .pane_rects
                        .with_untracked(|m| m.get(&pane_id).map(|r| r.contains(ds.current_window)))
                        .unwrap_or(false)
                        && !is_source;
                    (is_source, is_target)
                }
            } else {
                (false, false)
            };
            let s = s.size_full().flex_col().border(2);
            if is_drag_target {
                s.border_color(theme::border_focus())
                    .background(theme::accent_select())
            } else if is_drag_source {
                s.border_color(theme::border_focus())
            } else if is_active {
                s.border_color(theme::border_focus())
            } else {
                s.border_color(theme::border_default())
            }
        })
        .on_event_cont(EventListener::PointerDown, move |_| {
            // クリックされたペインを active に
            if let Some(t) = app_for_focus.active_tab() {
                if t.all_panes().iter().any(|p| p.id == pane_id)
                    && t.active_pane.get_untracked() != pane_id
                {
                    t.active_pane.set(pane_id);
                }
            }
        })
        .on_resize(move |rect| {
            pane_width_sig.set(rect.width() as f32);
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
                if let Some((target, start_x, start_name, start_size, _start_mtime)) =
                    col_resize_for_drag.get_untracked()
                {
                    let dx = (p.pos.x - start_x) as f32;
                    let pane_w = pane_width_for_drag.get_untracked();
                    let sep_total = 10.0f32;
                    let base = 60.0f32 + sep_total;
                    match target {
                        ColumnResizeTarget::Name => {
                            let mut new_w = (start_name + dx).clamp(24.0, 1200.0);
                            if pane_w > 0.0 {
                                let size_w = size_w_sig_for_drag_read.get_untracked();
                                let mtime_w = mtime_w_sig_for_drag_read.get_untracked();
                                let max_w =
                                    (pane_w - base - size_w - mtime_w).max(24.0).min(1200.0);
                                new_w = new_w.min(max_w);
                            }
                            name_w_sig_for_drag.set(new_w);
                        }
                        ColumnResizeTarget::Size => {
                            let mut new_w = (start_size + dx).clamp(24.0, 600.0);
                            if pane_w > 0.0 {
                                let name_w = name_w_sig_for_drag_read.get_untracked();
                                let mtime_w = mtime_w_sig_for_drag_read.get_untracked();
                                let max_w = (pane_w - base - name_w - mtime_w).max(24.0).min(600.0);
                                new_w = new_w.min(max_w);
                            }
                            size_w_sig_for_drag.set(new_w);
                        }
                    }
                    return;
                }
                let dragging = app_for_move.dragging.get_untracked();
                if dragging.is_none() {
                    // dragging 未生成: 候補があり閾値超えなら、ここで初めて生成する。
                    if let Some((source_pane, paths, start_pos)) =
                        drag_candidate_for_move.get_untracked()
                    {
                        let dx = (p.pos.x - start_pos.x) as f32;
                        let dy = (p.pos.y - start_pos.y) as f32;
                        if (dx * dx + dy * dy).sqrt() > 5.0 {
                            let pane_origin = app_for_move.pane_rects.with_untracked(|m| {
                                m.get(&pane_id).map(|r| r.origin()).unwrap_or(Point::ZERO)
                            });
                            let win_start = Point::new(
                                pane_origin.x + start_pos.x,
                                pane_origin.y + start_pos.y,
                            );
                            let win_cur =
                                Point::new(pane_origin.x + p.pos.x, pane_origin.y + p.pos.y);
                            crate::flog!(
                                "[drag] start (threshold passed) source_pane={} paths={}",
                                source_pane,
                                paths.len()
                            );
                            app_for_move.dragging.set(Some(DragState {
                                source_pane,
                                paths,
                                start_window: Some(win_start),
                                current_window: win_cur,
                                active: true,
                            }));
                        }
                    }
                    return;
                }
                let pane_origin = app_for_move
                    .pane_rects
                    .with_untracked(|m| m.get(&pane_id).map(|r| r.origin()).unwrap_or(Point::ZERO));
                let win_pt = Point::new(pane_origin.x + p.pos.x, pane_origin.y + p.pos.y);
                app_for_move.dragging.update(|d| {
                    if let Some(ds) = d {
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
                if col_resize_for_drag_end.get_untracked().is_some() {
                    col_resize_for_drag_end.set(None);
                    return;
                }
                // クリックの押し戻し時にも候補は確実にクリア（ドラッグ未成立含む）。
                drag_candidate_for_up.set(None);
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
                crate::flog!(
                    "[drop] PointerUp pane={} ds.active={} ds.source_pane={} ds.paths={}",
                    pane_id,
                    ds.active,
                    ds.source_pane,
                    ds.paths.len()
                );
                if !ds.active {
                    crate::flog!("[drop] skip (active={})", ds.active);
                    return;
                }
                let pane_origin = app_for_up
                    .pane_rects
                    .with_untracked(|m| m.get(&pane_id).map(|r| r.origin()).unwrap_or(Point::ZERO));
                let win_pt = Point::new(pane_origin.x + p.pos.x, pane_origin.y + p.pos.y);
                let rects_dump: Vec<(u64, Rect)> = app_for_up
                    .pane_rects
                    .with_untracked(|m| m.iter().map(|(k, v)| (*k, *v)).collect());
                crate::flog!(
                    "[drop] win_pt=({:.1},{:.1}) mode=COPY pane_rects={:?}",
                    win_pt.x,
                    win_pt.y,
                    rects_dump
                );
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
                crate::flog!(
                    "[drop] target_id={} (source_pane={})",
                    target_id,
                    ds.source_pane
                );
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
                crate::flog!("[drop] dest_dir={} mode={}", dest_dir.display(), "COPY");
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
                    crate::flog!(
                        "[drop] copy_path src={} dst={}",
                        src.display(),
                        dst.display()
                    );
                    let res = fops::copy_path(
                        src.to_string_lossy().into_owned(),
                        dst.to_string_lossy().into_owned(),
                    );
                    match res {
                        Ok(()) => ok += 1,
                        Err(e) => {
                            crate::flog!("[drop] op error: {}", e);
                            err += 1;
                        }
                    }
                }
                tp.status_msg
                    .set(format!("D&D コピー OK={} / NG={}", ok, err));
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
