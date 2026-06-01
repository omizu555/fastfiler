//! 設定ダイアログ本体 (`settings_view`) と、各タブ (general/workspace/search/hotkeys/debug) のビュー。

use floem::prelude::*;
use floem::style::CursorStyle;
use floem::views::{
    button, container, dyn_container, dyn_stack, empty, h_stack, label, scroll, text_input,
    v_stack, Decorators,
};

use crate::theme;

use super::model::AppSettings;
use super::widgets::{row_check, row_font, row_input, row_select, section_label};

fn tab_general(s: &AppSettings) -> floem::AnyView {
    let body = v_stack((
        section_label("General"),
        row_input("起動パス", s.initial_path),
        row_check("隠しファイルを表示", s.show_hidden),
        row_check("ペインツールバーを隠す", s.hide_pane_toolbar),
        section_label("Theme"),
        row_select("テーマ", s.theme, vec!["system", "dark", "light"]),
        row_select(
            "プリセット",
            s.theme_preset,
            vec![
                "default",
                "dracula",
                "solarizedDark",
                "solarizedLight",
                "nord",
                "monokai",
            ],
        ),
        row_input("アクセントカラー (#rrggbb)", s.accent_color),
        row_select(
            "アイコンセット",
            s.icon_set,
            vec!["emoji", "minimal", "colored"],
        ),
        section_label("Font"),
        row_font("UI フォント", s.ui_font),
        row_input("UI フォントサイズ", s.ui_font_size),
    ))
    .style(|s| s.flex_col());
    container(body).style(|s| s.padding(8)).into_any()
}

fn tab_workspace(s: &AppSettings) -> floem::AnyView {
    let body = v_stack((
        section_label("Workspace"),
        row_input("タブ列数 (1〜4)", s.tab_columns),
        row_input("タブ幅 (px)", s.tabs_width),
        row_input("ツリー幅 (px)", s.tree_width),
        row_select(
            "タブパネル位置",
            s.panel_dock_tabs,
            vec!["left", "right", "hidden"],
        ),
        row_select(
            "ツリーパネル位置",
            s.panel_dock_tree,
            vec!["left", "right", "hidden"],
        ),
    ))
    .style(|s| s.flex_col());
    container(body).style(|s| s.padding(8)).into_any()
}

fn tab_search(s: &AppSettings) -> floem::AnyView {
    let body = v_stack((
        section_label("Search"),
        row_select(
            "検索バックエンド",
            s.search_backend,
            vec!["builtin", "everything"],
        ),
        row_input("Everything ポート", s.everything_port),
        row_check("Everything スコープ", s.everything_scope),
    ))
    .style(|s| s.flex_col());
    container(body).style(|s| s.padding(8)).into_any()
}

fn tab_hotkeys(s: &AppSettings) -> floem::AnyView {
    let hotkeys = s.hotkeys.get();
    let mut rows: Vec<floem::AnyView> = Vec::new();
    rows.push(section_label("Hotkeys").into_any());
    for (action, sig) in hotkeys.iter() {
        let action_text = action.clone();
        let sig = *sig;
        let row = h_stack((
            label(move || action_text.clone())
                .style(|s| s.width(220).padding(6).color(theme::text_label())),
            text_input(sig).style(|s| {
                s.flex_grow(1.0)
                    .padding(4)
                    .border(1)
                    .border_color(theme::border_strong())
                    .background(theme::bg_modal())
                    .color(theme::text_normal())
            }),
        ))
        .style(|s| s.padding(4).items_center().gap(8));
        rows.push(row.into_any());
    }
    container(floem::views::stack_from_iter(rows).style(|s| s.flex_col()))
        .style(|s| s.padding(8))
        .into_any()
}

pub fn settings_view(settings: AppSettings, open: RwSignal<bool>) -> impl IntoView {
    let active_tab: RwSignal<&'static str> = RwSignal::new("general");

    // デバッグタブ用 polling: tab が "debug" かつ dialog が open の間だけ
    // 500ms 周期で perf::snapshot を取り直して RwSignal を更新する。
    // generation で多重起動を防止 (タブを離れて戻ってきたときに古い chain が動き続けない)。
    let perf_snapshot: RwSignal<crate::core::perf::Snapshot> =
        RwSignal::new(crate::core::perf::snapshot());
    let perf_poll_gen: RwSignal<u64> = RwSignal::new(0);
    {
        let snap_sig = perf_snapshot;
        let gen_sig = perf_poll_gen;
        let open_sig = open;
        floem::reactive::create_effect(move |prev: Option<&'static str>| {
            let now = active_tab.get();
            if prev.as_ref() == Some(&now) {
                return now;
            }
            if now == "debug" {
                snap_sig.set(crate::core::perf::snapshot());
                let g = gen_sig.get_untracked().wrapping_add(1);
                gen_sig.set(g);
                schedule_perf_poll(snap_sig, gen_sig, g, open_sig, active_tab);
            }
            now
        });
    }

    let make_tab = move |id: &'static str, title: &'static str| {
        let active_tab = active_tab;
        label(move || title.to_string())
            .style(move |s| {
                let on = active_tab.get() == id;
                let bg = if on {
                    theme::accent_select()
                } else {
                    theme::bg_chrome()
                };
                s.height(32)
                    .width_full()
                    .items_center()
                    .padding_horiz(12)
                    .background(bg)
                    .border_bottom(1)
                    .border_color(theme::border_default())
                    .cursor(CursorStyle::Pointer)
                    .color(theme::text_normal())
            })
            .on_click_stop(move |_| active_tab.set(id))
    };

    let tabs_col = v_stack((
        label(|| String::from("Settings")).style(|s| {
            s.padding(12)
                .font_bold()
                .color(theme::text_normal())
                .font_size(15.0)
        }),
        make_tab("general", "General"),
        make_tab("workspace", "Workspace"),
        make_tab("search", "Search"),
        make_tab("hotkeys", "Hotkeys"),
        make_tab("debug", "Debug"),
    ))
    .style(|s| {
        s.width(180)
            .height_full()
            .background(theme::bg_panel())
            .border_right(1)
            .border_color(theme::border_default())
            .flex_col()
    });

    let settings_for_body = settings.clone();
    let body = dyn_container(
        move || active_tab.get(),
        move |which| match which {
            "general" => tab_general(&settings_for_body),
            "workspace" => tab_workspace(&settings_for_body),
            "search" => tab_search(&settings_for_body),
            "hotkeys" => tab_hotkeys(&settings_for_body),
            "debug" => tab_debug(perf_snapshot),
            _ => label(|| String::new()).into_any(),
        },
    )
    .style(|s| s.size_full().flex_grow(1.0));

    let settings_for_save = settings.clone();
    let close_btn = button("× Close").action(move || {
        if let Err(e) = settings_for_save.save() {
            eprintln!("[settings] save error: {}", e);
        }
        open.set(false);
    });

    let header = h_stack((
        label(|| String::from("⚙ Settings")).style(|s| {
            s.padding(8)
                .font_bold()
                .font_size(15.0)
                .color(theme::text_normal())
                .flex_grow(1.0)
        }),
        close_btn,
    ))
    .style(|s| {
        s.height(40)
            .items_center()
            .padding_horiz(8)
            .background(theme::bg_chrome())
            .border_bottom(1)
            .border_color(theme::border_default())
    });

    let content = h_stack((
        tabs_col,
        scroll(body).style(|s| s.size_full().flex_grow(1.0)),
    ))
    .style(|s| s.size_full().flex_grow(1.0));

    v_stack((header, content)).style(|s| {
        s.size_full()
            .flex_col()
            .background(theme::bg_root())
            .color(theme::text_normal())
    })
}

// ─────────── Debug タブ ───────────

/// `active_tab` が "debug" の間だけ 500ms 周期で perf::snapshot を取り直す。
/// generation guard で多重起動と離脱後の再描画を防ぐ。
fn schedule_perf_poll(
    snap_sig: RwSignal<crate::core::perf::Snapshot>,
    gen_sig: RwSignal<u64>,
    my_gen: u64,
    open_sig: RwSignal<bool>,
    active_tab: RwSignal<&'static str>,
) {
    floem::action::exec_after(std::time::Duration::from_millis(500), move |_| {
        if !open_sig.get_untracked() || active_tab.get_untracked() != "debug" {
            return;
        }
        if gen_sig.get_untracked() != my_gen {
            return;
        }
        snap_sig.set(crate::core::perf::snapshot());
        schedule_perf_poll(snap_sig, gen_sig, my_gen, open_sig, active_tab);
    });
}

fn fmt_ms(v: f64) -> String {
    if v >= 1000.0 {
        format!("{:.2} s", v / 1000.0)
    } else if v >= 10.0 {
        format!("{:.1} ms", v)
    } else {
        format!("{:.2} ms", v)
    }
}

fn tab_debug(snap_sig: RwSignal<crate::core::perf::Snapshot>) -> floem::AnyView {
    let header_row = h_stack((
        label(|| String::from("Kind")).style(|s| s.width(110).font_bold()),
        label(|| String::from("Count")).style(|s| s.width(70).font_bold()),
        label(|| String::from("Avg")).style(|s| s.width(90).font_bold()),
        label(|| String::from("Max")).style(|s| s.width(90).font_bold()),
        label(|| String::from("Min")).style(|s| s.width(90).font_bold()),
        label(|| String::from("Last")).style(|s| s.width(90).font_bold()),
    ))
    .style(|s| {
        s.height(22)
            .items_center()
            .padding_horiz(4)
            .border_bottom(1)
            .border_color(theme::border_default())
    });

    let snap_for_aggs = snap_sig;
    let agg_rows = dyn_stack(
        move || {
            let snap = snap_for_aggs.get();
            snap.aggs.clone()
        },
        |(k, _): &(crate::core::perf::MetricKind, crate::core::perf::MetricAgg)| *k,
        |(k, a): (crate::core::perf::MetricKind, crate::core::perf::MetricAgg)| {
            let label_text = k.label();
            h_stack((
                label(move || label_text.to_string()).style(|s| s.width(110)),
                label(move || format!("{}", a.count)).style(|s| s.width(70)),
                label(move || {
                    if a.count == 0 {
                        String::from("-")
                    } else {
                        fmt_ms(a.avg_ms())
                    }
                })
                .style(|s| s.width(90)),
                label(move || {
                    if a.count == 0 {
                        String::from("-")
                    } else {
                        fmt_ms(a.max_ms)
                    }
                })
                .style(|s| s.width(90)),
                label(move || {
                    if a.count == 0 {
                        String::from("-")
                    } else {
                        fmt_ms(a.min_ms)
                    }
                })
                .style(|s| s.width(90)),
                label(move || {
                    if a.count == 0 {
                        String::from("-")
                    } else {
                        fmt_ms(a.last_ms)
                    }
                })
                .style(|s| s.width(90)),
            ))
            .style(|s| {
                s.height(20)
                    .items_center()
                    .padding_horiz(4)
                    .border_bottom(1)
                    .border_color(theme::border_default())
            })
            .into_any()
        },
    )
    .style(|s: floem::style::Style| s.flex_col());

    let snap_for_logs = snap_sig;
    let log_list = dyn_stack(
        move || {
            let snap = snap_for_logs.get();
            // snapshot は既に新しい順なのでそのまま
            snap.samples.clone()
        },
        |s: &crate::core::perf::MetricSample| (s.at, s.kind, s.dur_ms.to_bits(), s.detail.clone()),
        |s: crate::core::perf::MetricSample| {
            let ts = crate::core::perf::format_systemtime_jst(s.at);
            let kind = s.kind.label();
            let dur = fmt_ms(s.dur_ms);
            let detail = s.detail.clone();
            label(move || format!("{}  [{}]  {}  {}", ts, kind, dur, detail))
                .style(|s| s.height(18).padding_horiz(4).color(theme::text_normal()))
                .into_any()
        },
    )
    .style(|s: floem::style::Style| s.flex_col());

    let snap_for_count = snap_sig;
    let log_count = label(move || {
        let snap = snap_for_count.get();
        format!("直近ログ ({} 件 / 最大 500)", snap.samples.len())
    })
    .style(|s| s.padding(4).font_bold());

    let clear_btn = button("クリア").action(|| {
        crate::core::perf::clear();
    });
    let copy_btn = button("クリップボードにコピー").action(|| {
        let text = crate::core::perf::export_text();
        let _ = fastfiler_domain::win_clipboard::clipboard_write_text(&text);
    });

    let toolbar = h_stack((
        label(|| String::from("Performance")).style(|s| s.font_bold().flex_grow(1.0)),
        clear_btn,
        empty().style(|s| s.width(8)),
        copy_btn,
    ))
    .style(|s| s.height(36).items_center().padding_horiz(4));

    let body = v_stack((
        toolbar,
        section_label("集計 (起動以降の累計)"),
        header_row,
        agg_rows,
        log_count,
        scroll(log_list).style(|s| s.height(280).width_full()),
    ))
    .style(|s| s.flex_col());

    container(body).style(|s| s.padding(8)).into_any()
}
