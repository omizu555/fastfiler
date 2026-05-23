//! プログレスダイアログ overlay。
//!
//! `app.jobs.list` を `dyn_stack` で track し、各 JobView を 1 つのカード
//! として右下に縦積みで表示する。完了 (成功) したジョブは jobs.rs 側で
//! 500ms 後に自動で list から外れる。エラー / キャンセルは × で手動 dismiss。

use floem::peniko::Color;
use floem::reactive::SignalGet;
use floem::views::{dyn_stack, empty, h_stack, label, v_stack, Decorators};
use floem::IntoView;

use crate::core::jobs::{JobStatus, JobView, JobsState};

/// 右下に縦積みする overlay 全体。app_view のルート v_stack に absolute 配置で重ねる。
pub fn progress_dialogs(jobs: JobsState) -> impl IntoView {
    let list_sig = jobs.list;
    let jobs_for_each = jobs.clone();
    dyn_stack(
        move || list_sig.get(),
        move |jv| jv.id,
        move |jv| job_card(jv, jobs_for_each.clone()),
    )
    .style(|s| {
        s.flex_col()
            .gap(8)
            .position(floem::style::Position::Absolute)
            .inset_right(16)
            .inset_bottom(40)
            .width(360)
    })
}

fn job_card(jv: JobView, jobs: JobsState) -> impl IntoView {
    let id = jv.id;
    let kind = jv.kind.clone();
    let indeterminate = jv.indeterminate;
    let total_files_sig = jv.total_files;
    let done_files_sig = jv.done_files;
    let total_bytes_sig = jv.total_bytes;
    let done_bytes_sig = jv.done_bytes;
    let current_sig = jv.current;
    let status_sig = jv.status;

    let title = label(move || {
        let st = status_sig.get();
        let suffix = match st {
            JobStatus::Running => "",
            JobStatus::Success => " ✓",
            JobStatus::Canceled => " (キャンセル)",
            JobStatus::Error(_) => " ✗",
        };
        format!("{}{}", kind, suffix)
    })
    .style(|s| s.font_bold());

    let progress_text = label(move || {
        let df = done_files_sig.get();
        let tf = total_files_sig.get();
        if indeterminate {
            format!("{} / {} 件", df, tf)
        } else {
            let db = done_bytes_sig.get();
            let tb = total_bytes_sig.get();
            format!("{} / {} 件   {} / {}", df, tf, fmt_bytes(db), fmt_bytes(tb))
        }
    });

    let current_label = label(move || {
        let s = current_sig.get();
        if s.is_empty() {
            String::new()
        } else {
            // 長すぎるパスは末尾から表示寄りに truncate
            let max = 48;
            if s.chars().count() > max {
                let tail: String = s
                    .chars()
                    .rev()
                    .take(max)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                format!("…{}", tail)
            } else {
                s
            }
        }
    })
    .style(|s| s.color(Color::rgb8(0x88, 0x88, 0x88)).font_size(11.0));

    // 簡易プログレスバー: 背景 + 前景 (幅は割合で動的に)
    let bar_bg = empty().style(move |s| {
        s.width(320)
            .height(6)
            .background(Color::rgb8(0xdd, 0xdd, 0xdd))
            .border_radius(3)
    });
    let bar_fill = empty().style(move |s| {
        let st = status_sig.get();
        let color = match st {
            JobStatus::Running => Color::rgb8(0x2b, 0x88, 0xd8),
            JobStatus::Success => Color::rgb8(0x4c, 0xaf, 0x50),
            JobStatus::Canceled => Color::rgb8(0xaa, 0xaa, 0xaa),
            JobStatus::Error(_) => Color::rgb8(0xd0, 0x4a, 0x4a),
        };
        let pct = if indeterminate {
            let df = done_files_sig.get();
            let tf = total_files_sig.get().max(1);
            (df as f64 / tf as f64).clamp(0.0, 1.0)
        } else {
            let db = done_bytes_sig.get();
            let tb = total_bytes_sig.get().max(1);
            (db as f64 / tb as f64).clamp(0.0, 1.0)
        };
        let w = (320.0 * pct).max(0.0);
        s.position(floem::style::Position::Absolute)
            .width(w)
            .height(6)
            .background(color)
            .border_radius(3)
    });
    let bar = floem::views::stack((bar_bg, bar_fill)).style(|s| s.width(320).height(6));

    // 右端の操作ボタン (Running ならキャンセル, それ以外なら閉じる)
    let jobs_for_btn = jobs.clone();
    let action_btn = label(move || {
        match status_sig.get() {
            JobStatus::Running => "×",
            _ => "閉じる",
        }
        .to_string()
    })
    .style(|s| {
        s.padding_horiz(8)
            .padding_vert(2)
            .border(1)
            .border_color(Color::rgb8(0xbb, 0xbb, 0xbb))
            .border_radius(4)
            .cursor(floem::style::CursorStyle::Pointer)
    })
    .on_click_stop(move |_| {
        let st = status_sig.get_untracked();
        match st {
            JobStatus::Running => jobs_for_btn.cancel(id),
            _ => jobs_for_btn.dismiss(id),
        }
    });

    let header =
        h_stack((title.style(|s| s.flex_grow(1.0)), action_btn)).style(|s| s.items_center().gap(8));

    v_stack((header, progress_text, current_label, bar))
        .style(|s| {
            s.background(Color::rgb8(0xff, 0xff, 0xff))
                .border(1)
                .border_color(Color::rgb8(0xcc, 0xcc, 0xcc))
                .border_radius(6)
                .padding(10)
                .gap(4)
                .width(340)
        })
        .into_any()
}

fn fmt_bytes(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if n >= GB {
        format!("{:.2} GB", n as f64 / GB as f64)
    } else if n >= MB {
        format!("{:.1} MB", n as f64 / MB as f64)
    } else if n >= KB {
        format!("{:.1} KB", n as f64 / KB as f64)
    } else {
        format!("{} B", n)
    }
}
