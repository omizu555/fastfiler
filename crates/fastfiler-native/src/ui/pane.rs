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
    virtual_stack, ClipExt, Decorators, VirtualDirection, VirtualItemSize,
};

use fastfiler_domain::icons as ficons;

use crate::fs_model::{FileRow, SortKey};
use crate::state::{AppState, DragState, PaneState};
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

fn elide_for_width(input: &str, col_width: f32, padding_px: f32, font_px: f32) -> String {
    let sanitized = input.replace(['\r', '\n', '\t'], " ");
    let usable = (col_width - padding_px * 2.0).max(0.0);
    if usable < font_px {
        return String::new();
    }
    // 半角/全角の見積もり幅をフォントサイズに比例させる。
    // 数値は floem 0.2 の system font を実測してフィッティング (fs=13 で半角 ≒ 7px, 全角 ≒ 13-14px)。
    let ascii_px = font_px * 0.55;
    let wide_px = font_px * 1.05;
    let ellipsis_px = font_px * 0.6;
    let budget_px = (usable - ellipsis_px).max(0.0);
    if budget_px <= 0.0 {
        return String::new();
    }

    let mut used = 0.0f32;
    let mut out = String::new();
    let mut truncated = false;
    for ch in sanitized.chars() {
        let ch_px = if ch.is_ascii() { ascii_px } else { wide_px };
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
    let search_query = pane.search_query;
    let search_open = pane.search_open;
    let name_col_width_sig = pane.name_col_width;
    let size_col_width_sig = pane.size_col_width;
    let mtime_col_width_sig = pane.mtime_col_width;
    let ui_font_size_sig = app.settings.ui_font_size;
    // Undo マネージャはアプリ全体で 1 本 (ADR 0006/0008)。
    // UI からは clipboard_paste / delete_selected / confirm_modal の各クロージャに渡す必要がある。
    // 個別の use-site (ctxmenu / blank ctxmenu / modal) ごとに事前に clone を取り分けておく。
    let col_resize_drag = floem::reactive::Scope::new()
        .create_rw_signal(None::<(ColumnResizeTarget, f64, f32, f32, f32)>);
    // ドラッグ候補 (PointerDown 時の文脈): (source_pane_id, paths, start_pos_in_pane, right_button).
    // PointerMove で閾値を超えるまでは dragging に乗せず、誤発火を防ぐ。
    let drag_candidate =
        floem::reactive::Scope::new().create_rw_signal(None::<(u64, Vec<PathBuf>, Point, bool)>);
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

    // cur_path 変化を監視: ナビゲーション発生時に drag 関連状態を必ず掃除する。
    // ダブルクリックで子フォルダに入った場合などは、対応する PointerUp が
    // 元の view 破棄により発火しないので、drag_candidate / dragging が残留する
    // ことがある (戻った後にドラッグ状態が継続する症状の原因)。
    {
        let app_for_nav_clear = app.clone();
        let drag_candidate_for_nav = drag_candidate;
        let pane_id_for_nav = pane.id;
        let cur_path_sig = pane.cur_path;
        floem::reactive::create_effect(move |prev: Option<PathBuf>| {
            let cur = cur_path_sig.get();
            if let Some(ref p) = prev {
                if p != &cur {
                    if drag_candidate_for_nav.get_untracked().is_some() {
                        crate::flog!(
                            "[drag] cleared by navigation pane={} (drag_candidate)",
                            pane_id_for_nav
                        );
                        drag_candidate_for_nav.set(None);
                    }
                    // このペインを発生源とする dragging のみクリア (他ペインの D&D は維持)
                    let should_clear = app_for_nav_clear.dragging.with_untracked(|d| {
                        d.as_ref()
                            .is_some_and(|ds| ds.source_pane == pane_id_for_nav)
                    });
                    if should_clear {
                        crate::flog!(
                            "[drag] cleared by navigation pane={} (dragging)",
                            pane_id_for_nav
                        );
                        app_for_nav_clear.dragging.set(None);
                    }
                }
            }
            cur
        });
    }

    // Everything 検索 effect (search/mod.rs に分離)
    crate::search::attach_everything_effect(&pane, &app);

    // フィルタ計測 effect: search_query が変化したときに rows をフィルタする
    // コストを計測する。filtered_rows closure 内で計測すると毎描画ごとに
    // 二重計測されるのでここに独立 effect を置く。
    {
        let p_filter = pane.clone();
        floem::reactive::create_effect(move |prev: Option<String>| {
            let q = p_filter.search_query.get();
            if prev.as_ref() == Some(&q) {
                return q;
            }
            if prev.is_none() {
                return q;
            }
            let lq = q.to_lowercase();
            let rs = p_filter.rows.get_untracked();
            let t = std::time::Instant::now();
            let matched = rs
                .iter()
                .filter(|r| lq.is_empty() || r.name.to_lowercase().contains(&lq))
                .count();
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            crate::core::perf::record_manual(
                crate::core::perf::MetricKind::Filter,
                format!("q='{}' rows={} matched={}", q, rs.len(), matched),
                ms,
            );
            q
        });
    }

    let pane_for_up = pane.clone();
    let pane_for_reload = pane.clone();
    let pane_for_dblclick = pane.clone();
    let pane_for_addr_enter = pane.clone();
    let undo_mgr_ctxmenu = app.undo_manager.clone();
    let undo_mgr_blank = app.undo_manager.clone();
    let jobs_ctxmenu = app.jobs.clone();
    let jobs_blank = app.jobs.clone();
    let pane_for_keys = pane.clone();
    let pane_for_click = pane.clone();
    let pane_for_ctxmenu = pane.clone();
    let pane_for_blank_ctxmenu = pane.clone();
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
    let left_toolbar = h_stack((button("↑").action(move || pane_for_up.up()),)).style(move |s| {
        let s = s.gap(6).padding(6).items_center();
        if hide_toolbar_sig.get() {
            s.height(0).padding(0).hide()
        } else {
            s
        }
    });
    let right_toolbar = h_stack((
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
        button("⟳").action(move || pane_for_reload.reload()),
        button("⇔").action(move || app_for_split_h.split_active(false)),
        button("⇕").action(move || app_for_split_v.split_active(true)),
        button("✕").action(move || app_for_close_pane.close_pane(pane_id_for_close)),
    ))
    .style(move |s| {
        let s = s.gap(6).padding(6).items_center().flex_shrink(0.0);
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
    let col_resize_for_name_style = col_resize_drag;
    let col_resize_for_size_style = col_resize_drag;
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
            .style(move |s| {
                let active = matches!(
                    col_resize_for_name_style.get(),
                    Some((ColumnResizeTarget::Name, ..))
                );
                let bg = if active {
                    theme::border_focus()
                } else {
                    theme::border_default()
                };
                s.width(7.0)
                    .height_full()
                    .cursor(CursorStyle::ColResize)
                    .background(bg)
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
                    // start_x はペイン相対座標 (PointerMove と座標系を合わせる)
                    let start_x = 60.0 + n as f64 + 3.5;
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
            .style(move |s| {
                let active = matches!(
                    col_resize_for_size_style.get(),
                    Some((ColumnResizeTarget::Size, ..))
                );
                let bg = if active {
                    theme::border_focus()
                } else {
                    theme::border_default()
                };
                s.width(7.0)
                    .height_full()
                    .cursor(CursorStyle::ColResize)
                    .background(bg)
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
                    let start_x = 60.0 + n as f64 + 7.0 + s as f64 + 3.5;
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
            let app_for_spring_enter = app_for_rows.clone();
            let app_for_spring_leave = app_for_rows.clone();
            let pane_for_spring_enter = pane_for_rows.clone();
            let pane_for_spring_leave = pane_for_rows.clone();
            let row_name_for_spring_enter = row.name.clone();
            let row_name_for_spring_leave = row.name.clone();
            let drag_candidate_for_row = drag_candidate;
            let drag_candidate_for_click = drag_candidate;
            let drag_candidate_for_row_up = drag_candidate;
            let app_for_click_clear = app_for_rows.clone();
            let app_for_row_pointer_up = app_for_rows.clone();
            let app_for_row_pointer_down_clear = app_for_rows.clone();
            let pane_for_click_log = pane_for_rows.clone();
            let pane_for_drag_clear_id = pane_for_rows.id;
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
                            ui_font_size_sig
                                .get()
                                .parse::<f32>()
                                .unwrap_or(13.0)
                                .clamp(8.0, 32.0),
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
                container(label(|| String::new())).style(|s| s.width(7.0).height(22)),
                container(
                    label(move || {
                        elide_for_width(
                            &size_raw,
                            size_col_width_sig.get().clamp(24.0, 600.0),
                            6.0,
                            ui_font_size_sig
                                .get()
                                .parse::<f32>()
                                .unwrap_or(13.0)
                                .clamp(8.0, 32.0),
                        )
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
                container(label(|| String::new())).style(|s| s.width(7.0).height(22)),
                container(
                    label(move || {
                        elide_for_width(
                            &mtime_raw,
                            mtime_col_width_sig.get().clamp(24.0, 600.0),
                            6.0,
                            ui_font_size_sig
                                .get()
                                .parse::<f32>()
                                .unwrap_or(13.0)
                                .clamp(8.0, 32.0),
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
                    let is_secondary = p.button.is_secondary();
                    // 右クリック (secondary) → エクスプローラ準拠で「未選択ならその行だけ選択」。
                    // context_menu builder のタイミング依存を避け、メニュー表示前に確実に選択を整える。
                    if is_secondary {
                        let in_sel = pane_for_drag
                            .selected
                            .with_untracked(|s| s.contains(&bg_idx));
                        if !in_sel {
                            let mut s = im::OrdSet::new();
                            s.insert(bg_idx);
                            pane_for_drag.selected.set(s);
                            pane_for_drag.anchor.set(Some(bg_idx));
                        }
                        // 右ボタン D&D 候補として登録する。閾値を超えなければ
                        // PointerUp 時に通常の context_menu が出る。
                        let cur = pane_for_drag.cur_path.get_untracked();
                        let row_path = cur.join(&row_name_for_drag);
                        // 選択状態を更新後に in_sel を再評価する (上で選択を入れ替えた可能性)。
                        let now_in_sel = pane_for_drag
                            .selected
                            .with_untracked(|s| s.contains(&bg_idx));
                        let paths: Vec<PathBuf> = if now_in_sel {
                            let sel = pane_for_drag.selected.get_untracked();
                            let rs = pane_for_drag.rows.get_untracked();
                            sel.iter()
                                .filter_map(|i| rs.get(*i).map(|r| cur.join(&r.name)))
                                .collect()
                        } else {
                            vec![row_path]
                        };
                        crate::flog!(
                            "[drag] candidate(right) pane={} row={} paths={}",
                            pane_for_drag.id,
                            bg_idx,
                            paths.len()
                        );
                        drag_candidate_for_row.set(Some((pane_for_drag.id, paths, p.pos, true)));
                        return;
                    }
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
                    drag_candidate_for_row.set(Some((pane_for_drag.id, paths, p.pos, false)));
                }
            })
            .on_event_cont(EventListener::PointerUp, move |_| {
                // 安全網: クリック判定にならず、かつペイン側 PointerUp にも届かない
                // 経路があり得るので、行レベルでも drag 候補は必ず解除する。
                if drag_candidate_for_row_up.get_untracked().is_some() {
                    drag_candidate_for_row_up.set(None);
                }
                // dragging は「同一ペイン内で離した = ドロップ対象でない」場合のみクリアする。
                // 他ペイン由来のドロップは伝播先のペイン PointerUp で処理させる必要があるため、
                // ここでクリアすると行が密に並んでいる領域でドロップが成立しなくなる。
                let should_clear = app_for_row_pointer_up.dragging.with_untracked(|d| {
                    d.as_ref()
                        .is_some_and(|ds| ds.source_pane == pane_for_drag_clear_id)
                });
                if should_clear {
                    crate::flog!("[drag] cleared by row PointerUp safety net (same-pane)");
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
            .on_event_cont(EventListener::PointerEnter, move |_| {
                // Spring-loaded folder: D&D 中のみ arm。is_dir 以外はファイル → spring 不要。
                if !is_dir {
                    return;
                }
                if app_for_spring_enter.dragging.get_untracked().is_none() {
                    return;
                }
                let target = pane_for_spring_enter
                    .cur_path
                    .get_untracked()
                    .join(&row_name_for_spring_enter);
                // 自分のペインのカレント (= 既に開いている) は cd しても意味ないので無視。
                if target == pane_for_spring_enter.cur_path.get_untracked() {
                    return;
                }
                crate::ui::spring::arm_pane(
                    &app_for_spring_enter,
                    pane_for_spring_enter.id,
                    target,
                );
            })
            .on_event_cont(EventListener::PointerLeave, move |_| {
                if !is_dir {
                    return;
                }
                let target = pane_for_spring_leave
                    .cur_path
                    .get_untracked()
                    .join(&row_name_for_spring_leave);
                crate::ui::spring::disarm_if_pane(
                    &app_for_spring_leave,
                    pane_for_spring_leave.id,
                    &target,
                );
            })
            .context_menu({
                let pane_ctx = pane_for_ctxmenu.clone();
                let undo_ctx = undo_mgr_ctxmenu.clone();
                let jobs_ctx = jobs_ctxmenu.clone();
                move || {
                    let p_open = pane_ctx.clone();
                    let p_reveal = pane_ctx.clone();
                    let p_cut = pane_ctx.clone();
                    let p_copy = pane_ctx.clone();
                    let p_paste = pane_ctx.clone();
                    let p_rename = pane_ctx.clone();
                    let p_delete = pane_ctx.clone();
                    let p_props = pane_ctx.clone();
                    let p_tree = pane_ctx.clone();
                    let undo_mgr = undo_ctx.clone();
                    let jobs = jobs_ctx.clone();
                    // 選択切替は PointerDown (secondary) 側で済ませているため、ここでは行わない。
                    let _ = bg_idx; // 警告抑制 (以降のクロージャでは使用)
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
                        .entry(MenuItem::new("貼り付け").action({
                            let um = undo_mgr.clone();
                            let j = jobs.clone();
                            move || p_paste.clipboard_paste(&um, &j)
                        }))
                        .separator()
                        .entry(
                            MenuItem::new("名前の変更")
                                .action(move || p_rename.open_rename_modal()),
                        )
                        .entry(MenuItem::new("削除").action({
                            let um = undo_mgr.clone();
                            let j = jobs.clone();
                            move || p_delete.delete_selected(&um, &j)
                        }))
                        .separator()
                        .separator()
                        .entry(
                            MenuItem::new("ツリーをコピー")
                                .action(move || p_tree.copy_selected_tree()),
                        )
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

    // ステータスバー: 左側にメッセージ (status_msg)、右側に統計 (items/load/selected/fs-change)。
    // メッセージ側を flex_grow + min_width(0) にし、ペイン幅が狭くてもメッセージが必ず表示される
    // (はみ出した場合は統計側ではなくメッセージ側の末尾が省略される)。以前は 1 個の label に
    // 末尾結合で詰め込んでいたため、ペイン幅が狭いと `ready` 等の末尾が見えなくなっていた。
    let status = h_stack((
        label(move || status_msg.get()).style(|s| {
            s.flex_grow(1.0)
                .flex_basis(0)
                .min_width(0)
                .color(theme::text_normal())
        }),
        label(move || {
            let st = stats.get();
            let sel_count = selected.with(|s| s.len());
            let cnt = sink.counter.lock();
            format!(
                "items: {}   load: {:.2} ms   selected: {}   fs-change: {}",
                st.count, st.load_ms, sel_count, *cnt
            )
        })
        .style(|s| s.color(theme::text_dim())),
    ))
    .style(|s| {
        s.height(22)
            .width_full()
            .padding_horiz(8)
            .gap(12)
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
    let top_bar = h_stack((
        left_toolbar,
        breadcrumb.clip().style(|s| {
            s.flex_grow(1.0)
                .flex_basis(0)
                .min_width(0)
                .height(28)
                .items_center()
        }),
        right_toolbar,
    ))
    .style(|s| {
        s.width_full()
            .items_center()
            .border_bottom(1)
            .border_color(theme::border_modal())
    });
    v_stack((top_bar, search_bar, header, scrollable, status))
        .style(move |s| {
            let is_active = app_for_active_style
                .active_tab()
                .map(|t| t.active_pane.get() == pane_id)
                .unwrap_or(false);
            let drag_state = app_for_drag_style.dragging.get();
            let ext_hover = app_for_drag_style
                .external_drop_hover
                .get()
                .map(|h| h.pane_id == pane_id)
                .unwrap_or(false);
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
            if is_drag_target || ext_hover {
                s.border_color(theme::border_focus())
                    .background(theme::accent_select())
            } else if is_drag_source || is_active {
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
                    let sep_total = 14.0f32;
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
                    if let Some((source_pane, paths, start_pos, right_button)) =
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
                                "[drag] start (threshold passed) source_pane={} paths={} right={}",
                                source_pane,
                                paths.len(),
                                right_button
                            );
                            app_for_move.dragging.set(Some(DragState {
                                source_pane,
                                paths: paths.clone(),
                                start_window: Some(win_start),
                                current_window: win_cur,
                                active: true,
                                right_button,
                            }));
                            // 右ボタン D&D の場合は AppState.right_drag も set。
                            // Win32 サブクラスの WM_RBUTTONUP ハンドラがこれを読む (ADR 0011)。
                            if right_button {
                                app_for_move.right_drag.set(Some(
                                    crate::core::state::RightDragState {
                                        source_pane,
                                        paths,
                                        hover_pane: Some(pane_id),
                                    },
                                ));
                            }
                            // dragging を生成したら候補は役目を終える。
                            // 残しておくとドロップ完了後にマウスを動かしただけで
                            // 再度 dragging が立ち上がってしまう (ゴースト drag)。
                            drag_candidate_for_move.set(None);
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
                // 右ボタン D&D 中なら、現在カーソルが乗っているペインを記録する。
                // Win32 サブクラスがメニュー表示時に target pane として読む (ADR 0011)。
                app_for_move.right_drag.update(|rd| {
                    if let Some(state) = rd {
                        state.hover_pane = Some(pane_id);
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
                // 左ボタン以外: floem 0.2 は secondary PointerUp を listener に配信しないため、
                // ここに secondary が届くケースは limit されている。右ボタン D&D drop の検出は
                // Win32 サブクラス (`WM_RBUTTONUP`) 側で行う (ADR 0011)。安全策で cancel のみ。
                if !p.button.is_primary() {
                    if app_for_up.dragging.get_untracked().is_some() {
                        crate::flog!("[drop] PointerUp non-primary (rare), clear drag");
                        app_for_up.dragging.set(None);
                        app_for_up.right_drag.set(None);
                        crate::ui::spring::disarm(&app_for_up);
                    }
                    return;
                }
                let drag_opt = app_for_up.dragging.get_untracked();
                let Some(ds) = drag_opt else { return };
                app_for_up.dragging.set(None);
                // Spring-loaded folder の保留 hover も解除 (drop 確定なので一律 None)。
                crate::ui::spring::disarm(&app_for_up);
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
                // 右ボタンモードで primary up が来るのは想定外。安全側でメニュー出さず無視。
                if ds.right_button {
                    crate::flog!("[drop] primary-up with right_button drag, ignore");
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
                    "[drop] win_pt=({:.1},{:.1}) pane_rects={:?}",
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

                // ── op 決定 (drag_common::compute_effect で内部/外部 D&D 統一) ──
                let ctrl = p.modifiers.control();
                let shift = p.modifiers.shift();
                let (is_move, reason) =
                    crate::ui::drag_common::compute_effect(&ds.paths, &dest_dir, ctrl, shift);
                crate::ui::drop_exec::execute_drop(
                    &app_for_up,
                    &tp,
                    Some(ds.source_pane),
                    &ds.paths,
                    is_move,
                    reason,
                );
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
        .context_menu({
            // ペイン余白 (行のないところ) で開く context menu。
            // 行 context_menu は子側で先に消費されるので、ここに来るのは余白のみ。
            let p_open_here = pane_for_blank_ctxmenu.clone();
            let p_reveal_here = pane_for_blank_ctxmenu.clone();
            let p_newfolder = pane_for_blank_ctxmenu.clone();
            let p_newfile = pane_for_blank_ctxmenu.clone();
            let p_paste = pane_for_blank_ctxmenu.clone();
            let p_reload = pane_for_blank_ctxmenu.clone();
            let undo_blank = undo_mgr_blank.clone();
            let jobs_b = jobs_blank.clone();
            move || {
                Menu::new("")
                    .entry(MenuItem::new("エクスプローラで開く").action({
                        let p = p_open_here.clone();
                        move || {
                            let cur = p.cur_path.get_untracked();
                            let _ = fastfiler_domain::shell::open_with_shell(
                                cur.to_string_lossy().into_owned(),
                            );
                        }
                    }))
                    .entry(MenuItem::new("エクスプローラでこの場所を表示").action({
                        let p = p_reveal_here.clone();
                        move || {
                            let cur = p.cur_path.get_untracked();
                            let _ = fastfiler_domain::shell::reveal_in_explorer(
                                cur.to_string_lossy().into_owned(),
                            );
                        }
                    }))
                    .separator()
                    .entry(MenuItem::new("新規フォルダ").action({
                        let p = p_newfolder.clone();
                        move || p.open_new_folder_modal()
                    }))
                    .entry(build_new_file_submenu(p_newfile.clone()))
                    .entry(MenuItem::new("貼り付け").action({
                        let p = p_paste.clone();
                        let um = undo_blank.clone();
                        let j = jobs_b.clone();
                        move || p.clipboard_paste(&um, &j)
                    }))
                    .separator()
                    .entry(MenuItem::new("更新").action({
                        let p = p_reload.clone();
                        move || p.reload()
                    }))
            }
        })
}

/// 「新規ファイル」サブメニュー: ユーザー定義テンプレ一覧 + 空ファイル + テンプレフォルダを開く
fn build_new_file_submenu(pane: PaneState) -> Menu {
    let mut menu = Menu::new("新規ファイル");

    let templates = fastfiler_domain::templates::list_templates().unwrap_or_default();
    if templates.is_empty() {
        menu = menu.entry(MenuItem::new("(テンプレートがありません)").enabled(false));
    } else {
        for tpl in templates {
            let p = pane.clone();
            let path = tpl.path.clone();
            let label = tpl.name.clone();
            menu = menu
                .entry(MenuItem::new(label).action(move || p.create_from_template(path.clone())));
        }
    }
    menu = menu.separator();
    let p_empty = pane.clone();
    menu = menu
        .entry(MenuItem::new("空ファイルを作成…").action(move || p_empty.open_new_file_modal()));
    menu = menu.entry(MenuItem::new("テンプレートフォルダを開く").action(|| {
        if let Ok(dir) = fastfiler_domain::templates::templates_dir() {
            let _ = fastfiler_domain::shell::open_with_shell(dir);
        }
    }));
    menu
}
