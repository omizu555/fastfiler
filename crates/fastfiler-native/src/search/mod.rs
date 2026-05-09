//! 検索機能。現状は Everything HTTP API 連携 effect のみ。
//!
//! - Builtin (パネル内フィルタ) は `pane.search_query` を直接 view 側で参照する形で十分なので不要
//! - Everything は debounce (200ms) + リクエスト世代管理が必要なので effect として切り出している

use floem::reactive::{create_effect, Scope, SignalGet, SignalUpdate, SignalWith};

use crate::fs_model::FileRow;
use crate::state::{AppState, PaneState};

/// `search_open && backend == "everything"` のとき、`search_query` の変化を 200ms debounce して
/// `fastfiler_domain::everything::query` を別スレッドで実行し、結果を `pane.search_results` に詰める。
///
/// 閉じる / builtin / 空クエリ時は `search_results` を `None` にリセットする。
///
/// この関数は `pane_view` のセットアップフェーズで 1 度だけ呼び、effect を pane の寿命中ずっと走らせる想定。
pub fn attach_everything_effect(pane: &PaneState, app: &AppState) {
    let cur_path_sig = pane.cur_path;
    let backend_sig = app.settings.search_backend;
    let port_sig = app.settings.everything_port;
    let scope_sig = app.settings.everything_scope;
    let req_gen_sig = pane.search_request_gen;
    let results_sig = pane.search_results;
    let status_sig = pane.status_msg;
    let search_query_sig = pane.search_query;
    let search_open_sig = pane.search_open;

    create_effect(move |_prev: Option<()>| {
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
        let gen = req_gen_sig.get_untracked().wrapping_add(1);
        req_gen_sig.set(gen);
        let req_gen_for_cb = req_gen_sig;
        let results_for_cb = results_sig;
        let status_for_cb = status_sig;
        let cb = floem::ext_event::create_ext_action(
            Scope::current(),
            move |hits: Result<Vec<fastfiler_domain::everything::EverythingHit>, String>| {
                if req_gen_for_cb.get_untracked() != gen {
                    return;
                }
                match hits {
                    Ok(list) => {
                        let mut v: im::Vector<FileRow> = im::Vector::new();
                        for h in list {
                            v.push_back(FileRow {
                                name: h.name.clone(),
                                is_dir: h.is_dir,
                                size: 0,
                                modified: 0,
                                size_text: String::new(),
                                mtime_text: String::new(),
                                full_path: Some(h.path.clone()),
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
