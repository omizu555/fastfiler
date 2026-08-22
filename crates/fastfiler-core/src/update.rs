//! ペインの純 reducer (計画書 §5.3)。I/O 禁止 — 副作用は `Effect` で返す。

use crate::domain_event::DomainEvent;
use crate::effect::Effect;
use crate::menu::{self, MenuAction, MenuItem};
use crate::model::{
    sort_entries, Column, Entry, JobStatus, ModalKind, Overlay, PaneId, PaneState, SearchUi,
    COL_W_MAX, COL_W_MIN,
};
use crate::msg::PaneMsg;
use crate::transfer::{self, ConflictChoice, TransferOp};

/// watcher 変化 → reload のデバウンス間隔 (現行値を変えない — LESSONS/計画書)。
pub const RELOAD_DEBOUNCE_MS: u64 = 150;

// ユーザー向け文言 (同文の重複を防ぐ)
const MSG_TAB_LOCKED: &str = "タブはロックされています";
const MSG_NO_OPS_IN_SEARCH: &str = "検索結果ではファイル操作できません (開いてから操作)";

/// ペインへの入力を状態遷移 + 副作用列に変換する。
/// `locked` はタブのロック状態 (F-104): 移動系は `Effect::OpenTabFor` へ逃がし、
/// 履歴 (戻る/進む) は「タブはロックされています」表示で不動作。
pub fn update_pane(p: &mut PaneState, id: PaneId, locked: bool, msg: PaneMsg) -> Vec<Effect> {
    // オーバーレイ表示中は、一覧向けのキー操作を横取りしない (§10-3 の 2 段ディスパッチ)
    if p.overlay.is_some() {
        if let Some(r) = update_overlay(p, id, locked, &msg) {
            return r;
        }
    }
    match msg {
        PaneMsg::Loaded {
            generation,
            entries,
        } => {
            if generation != self_gen(p) {
                return vec![]; // 古い世代 (キャンセル済み) は捨てる
            }
            let (names, cursor_name) = p.selection_names();
            // 親へ戻った直後などは pending の名前を優先してカーソルを合わせる
            let cursor_name = p.pending_cursor_name.take().or(cursor_name);
            p.entries = entries;
            sort_entries(&mut p.entries, p.sort);
            p.loading = false;
            p.load_error = None;
            p.loaded_path = Some(p.cur_path.clone());
            // 行番号が変わるので保留クリックは破棄
            p.pending_click = None;
            // watcher/reload でも選択は名前で維持 (USAGE.md §2)
            p.restore_selection(&names, cursor_name.as_deref());
            p.ensure_cursor_visible();
            p.scroll_offset = p.scroll_offset.clamp(0.0, p.max_scroll());
            vec![]
        }
        PaneMsg::LoadFailed { generation, error } => {
            if generation == self_gen(p) {
                p.loading = false;
                // 移動先の読み込み失敗: 偽パスに留まらず、表示中の一覧のパスへ
                // cur_path を戻す (エクスプローラ準拠)。戻さないと一覧は旧フォルダの
                // まま cur_path だけが偽になり、ダブルクリックで偽パスが伸び続ける。
                let revert = p
                    .loaded_path
                    .as_ref()
                    .filter(|prev| **prev != p.cur_path)
                    .cloned();
                match revert {
                    Some(prev) => {
                        p.cur_path = prev;
                        // 直前のナビゲーションが積んだ履歴エントリを掃除する。
                        // GoBack 失敗は fwd 側、navigate/GoForward 失敗は back 側に
                        // 復帰先が積まれている (fwd を先に見るのは GoBack 失敗時に
                        // back 側の正規エントリを誤って消さないため)。
                        if p.history_fwd.last() == Some(&p.cur_path) {
                            p.history_fwd.pop();
                        } else if p.history_back.last() == Some(&p.cur_path) {
                            p.history_back.pop();
                        }
                        p.status_msg = Some(format!("エラー: {error}"));
                    }
                    // 復帰先が無い (初回読み込み) / その場の reload 失敗
                    None => p.load_error = Some(error),
                }
            }
            vec![]
        }
        PaneMsg::RowPressed { ix, ctrl, shift } => {
            // 修飾なしで選択済み行を押下: 選択を崩さない (複数選択のまま
            // 左ドラッグを始められるように)。ドラッグに至らなければ
            // RowReleased で単一選択へ確定する (エクスプローラ準拠)。
            if !ctrl && !shift && p.selected.contains(&ix) {
                p.pending_click = Some(ix);
            } else {
                p.pending_click = None;
                p.click_row(ix, ctrl, shift);
            }
            vec![]
        }
        PaneMsg::RowReleased { ix } => {
            if p.pending_click.take() == Some(ix) {
                p.click_row(ix, false, false);
            }
            vec![]
        }
        PaneMsg::RowDoubleClicked { ix } => activate(p, id, locked, ix),
        PaneMsg::ActivateCursor => match p.cursor {
            Some(ix) => activate(p, id, locked, ix),
            None => vec![],
        },
        msg @ (PaneMsg::OpenSearch
        | PaneMsg::SearchInput(_)
        | PaneMsg::SearchCommit
        | PaneMsg::SearchClose
        | PaneMsg::SearchStarted(_)) => update_search(p, id, msg),
        PaneMsg::GoParent => {
            let from = p.cur_path.clone();
            match p.cur_path.parent() {
                Some(parent) => {
                    let parent = parent.to_path_buf();
                    if locked {
                        return vec![Effect::OpenTabFor { path: parent }];
                    }
                    let effects = navigate(p, id, parent);
                    // 親へ戻ったら、元いたフォルダにカーソルを合わせたいが
                    // 一覧はまだ無い → Loaded 後の復元に載せる (名前だけ先に設定)。
                    if let Some(name) = from.file_name().map(|s| s.to_string_lossy().to_string()) {
                        p.pending_cursor_name = Some(name);
                    }
                    effects
                }
                None => vec![],
            }
        }
        PaneMsg::HeaderClicked(col) => {
            if p.sort.col == col {
                p.sort.asc = !p.sort.asc;
            } else {
                p.sort.col = col;
                p.sort.asc = true;
            }
            resort_keeping_selection(p);
            p.ensure_cursor_visible();
            vec![]
        }
        PaneMsg::ColResized { col, width } => {
            let w = width.clamp(COL_W_MIN, COL_W_MAX);
            match col {
                Column::Modified => p.col_widths[0] = w,
                Column::Size => p.col_widths[1] = w,
                Column::Type => p.col_widths[2] = w,
                Column::Name => {} // 名前列は残り吸収なのでリサイズ対象外
            }
            vec![]
        }
        PaneMsg::Scrolled(offset) => {
            p.scroll_offset = offset.clamp(0.0, p.max_scroll());
            vec![]
        }
        PaneMsg::ViewportChanged { height } => {
            p.viewport_h = height;
            p.scroll_offset = p.scroll_offset.clamp(0.0, p.max_scroll());
            vec![]
        }
        PaneMsg::Nav(key, shift) => {
            p.key_nav(key, shift);
            vec![]
        }
        PaneMsg::SelectAll => {
            p.select_all();
            vec![]
        }
        PaneMsg::ClearSelection => {
            // Esc の優先順位: ジョブキャンセル > 検索を閉じる > 選択解除
            if let Some(job) = &p.job {
                return vec![Effect::CancelJob { id: job.id }];
            }
            if let Some(sui) = p.search.take() {
                // SearchClose と同じ: スクロールを entries 範囲へ戻し、検索も止める
                p.scroll_offset = p.scroll_offset.clamp(0.0, p.max_scroll());
                return match sui.job_id {
                    Some(job_id) => vec![Effect::CancelSearch { job_id }],
                    None => vec![],
                };
            }
            p.status_msg = None;
            p.clear_selection();
            vec![]
        }
        PaneMsg::BlankPressed => {
            p.status_msg = None;
            p.clear_selection();
            vec![]
        }
        PaneMsg::GoBack => {
            if locked {
                p.status_msg = Some(MSG_TAB_LOCKED.into());
                return vec![];
            }
            let Some(prev) = p.history_back.pop() else {
                return vec![];
            };
            p.history_fwd.push(p.cur_path.clone());
            set_path_and_load(p, id, prev)
        }
        PaneMsg::GoForward => {
            if locked {
                p.status_msg = Some(MSG_TAB_LOCKED.into());
                return vec![];
            }
            let Some(next) = p.history_fwd.pop() else {
                return vec![];
            };
            p.history_back.push(p.cur_path.clone());
            set_path_and_load(p, id, next)
        }
        PaneMsg::Reload => reload(p, id),
        PaneMsg::OpenPathEdit => {
            p.overlay = Some(Overlay::PathEdit {
                value: p.cur_path.to_string_lossy().to_string(),
            });
            vec![]
        }
        // overlay なし状態で届いた入力系は無視 (update_overlay が正規経路)
        PaneMsg::PathEditInput(_) | PaneMsg::PathEditCommit | PaneMsg::PathEditCancel => vec![],
        PaneMsg::Domain(ev) => update_domain_event(p, id, ev),
        PaneMsg::ReloadTick(seq) => {
            if seq == p.reload_seq {
                reload(p, id)
            } else {
                vec![] // より新しい変化が控えている (デバウンス継続)
            }
        }
        PaneMsg::OpenRename => {
            if p.showing_search() {
                return vec![];
            }
            let Some(name) = p
                .cursor
                .and_then(|i| p.entries.get(i))
                .map(|e| e.name.to_string())
            else {
                return vec![];
            };
            p.overlay = Some(Overlay::Modal {
                kind: ModalKind::Rename {
                    original: name.clone(),
                },
                value: name,
            });
            vec![]
        }
        PaneMsg::OpenNewFolder => {
            p.overlay = Some(Overlay::Modal {
                kind: ModalKind::NewFolder,
                value: String::new(),
            });
            vec![]
        }
        PaneMsg::OpenNewFile => {
            p.overlay = Some(Overlay::Modal {
                kind: ModalKind::NewFile,
                value: String::new(),
            });
            vec![]
        }
        // overlay なし状態で届いたモーダル系は無視
        PaneMsg::ModalInput(_) | PaneMsg::ModalCommit | PaneMsg::ModalCancel => vec![],
        PaneMsg::Conflict(_) => vec![],
        PaneMsg::RequestCopy | PaneMsg::RequestCut if p.showing_search() => {
            p.status_msg = Some(MSG_NO_OPS_IN_SEARCH.into());
            vec![]
        }
        PaneMsg::RequestCopy => clipboard_write(p, "copy"),
        PaneMsg::RequestCut => clipboard_write(p, "cut"),
        PaneMsg::RequestPaste => vec![Effect::ClipboardRead { pane: id }],
        PaneMsg::PasteRead { paths, op } => {
            let op = if op == "cut" {
                TransferOp::Move
            } else {
                TransferOp::Copy
            };
            let sources: Vec<std::path::PathBuf> =
                paths.into_iter().map(std::path::PathBuf::from).collect();
            let existing = existing_names(p);
            let plan = transfer::plan_transfer(op, &sources, &p.cur_path, &existing);
            if plan.items.is_empty() {
                return vec![];
            }
            if plan.conflicts.is_empty() {
                let items =
                    transfer::resolve_conflicts(&plan, ConflictChoice::Overwrite, &existing);
                vec![Effect::SpawnJob {
                    pane: id,
                    op,
                    items,
                }]
            } else {
                p.overlay = Some(Overlay::Conflict { plan });
                vec![]
            }
        }
        PaneMsg::PasteVirtualRead { entries } => {
            // RDP/Outlook の仮想ファイル (ソースパスなし)。常にコピー動作 —
            // rdpclip は切り取りの移動 (ソース削除) を越境できないため。
            let existing = existing_names(p);
            let plan = transfer::plan_virtual_paste(entries, &p.cur_path, &existing);
            if plan.entries.is_empty() {
                return vec![];
            }
            if plan.conflicts.is_empty() {
                vec![Effect::PasteVirtual {
                    pane: id,
                    dest: plan.dest,
                    entries: plan.entries,
                }]
            } else {
                p.overlay = Some(Overlay::VirtualConflict { plan });
                vec![]
            }
        }
        PaneMsg::RequestDelete => {
            // 検索結果リストの index は entries と別空間 — 破壊的操作は禁止 (安全側)
            if p.showing_search() {
                p.status_msg = Some(MSG_NO_OPS_IN_SEARCH.into());
                return vec![];
            }
            let paths = p.selected_paths();
            if paths.is_empty() {
                return vec![];
            }
            vec![Effect::DeleteToTrash { paths }]
        }
        PaneMsg::Undo => vec![Effect::PerformUndo],
        PaneMsg::StatusMsg(s) => {
            p.status_msg = Some(s);
            vec![]
        }
        PaneMsg::NavigateTo(path) => {
            if path == p.cur_path {
                return vec![];
            }
            navigate_or_new_tab(p, id, locked, path)
        }
        PaneMsg::OpenMenu {
            at,
            row,
            templates,
            commands,
            templates_dir,
            can_paste,
        } => {
            // 選択に含まれない行での右クリックは単一選択にしてから開く
            if let Some(ix) = row {
                if !p.selected.contains(&ix) {
                    p.click_row(ix, false, false);
                }
            }
            let row_ctx = row.and_then(|ix| {
                let list = if p.showing_search() {
                    return None; // 検索結果ではメニューを出さない (操作対象が別空間)
                } else {
                    &p.entries
                };
                list.get(ix).map(|e| (e.is_dir, e.ext.clone()))
            });
            if row.is_some() && row_ctx.is_none() && !p.showing_search() {
                return vec![];
            }
            let ctx = menu::MenuContext {
                row: row_ctx.as_ref().map(|(d, e)| (*d, e.as_deref())),
                can_paste,
                templates: &templates,
                commands: &commands,
            };
            let items = menu::build_menu(&ctx);
            p.overlay = Some(Overlay::ContextMenu {
                items,
                at,
                open_path: vec![],
                target_row: row,
                templates_dir,
            });
            vec![]
        }
        PaneMsg::BandSelect { a, b } => {
            p.band_select(a, b);
            vec![]
        }
        PaneMsg::RightPressed { ix } => {
            if !p.showing_search() && !p.selected.contains(&ix) && ix < p.entries.len() {
                p.click_row(ix, false, false);
            }
            vec![]
        }
        PaneMsg::ShellMenuRequest { row } => {
            if p.showing_search() {
                return vec![];
            }
            if let Some(ix) = row {
                if !p.selected.contains(&ix) {
                    p.click_row(ix, false, false);
                }
            }
            let paths = p.selected_paths();
            if paths.is_empty() {
                return vec![];
            }
            vec![Effect::ShowShellMenu { paths }]
        }
        // overlay なし状態で届いたメニュー操作は無視
        PaneMsg::MenuClicked(_) | PaneMsg::MenuClose => vec![],
        PaneMsg::CancelJobRequest => match &p.job {
            Some(job) => vec![Effect::CancelJob { id: job.id }],
            None => vec![],
        },
    }
}

/// 選択パスをクリップボードへ (選択が空なら何もしない)。
fn clipboard_write(p: &PaneState, op: &str) -> Vec<Effect> {
    let paths = p.selected_paths();
    if paths.is_empty() {
        return vec![];
    }
    vec![Effect::ClipboardWrite {
        paths,
        op: op.to_string(),
    }]
}

fn existing_names(p: &PaneState) -> std::collections::BTreeSet<String> {
    p.entries.iter().map(|e| e.name.to_string()).collect()
}

/// オーバーレイ表示中のメッセージ処理。`None` を返すと通常処理へフォールスルー。
fn update_overlay(
    p: &mut PaneState,
    id: PaneId,
    locked: bool,
    msg: &PaneMsg,
) -> Option<Vec<Effect>> {
    // どのオーバーレイでも通すもの (読み込み結果・幾何・domain イベント)
    if matches!(
        msg,
        PaneMsg::Loaded { .. }
            | PaneMsg::LoadFailed { .. }
            | PaneMsg::Scrolled(_)
            | PaneMsg::ViewportChanged { .. }
            | PaneMsg::Domain(_)
            | PaneMsg::ReloadTick(_)
            | PaneMsg::StatusMsg(_)
            | PaneMsg::CancelJobRequest
    ) {
        return None;
    }
    match p.overlay.as_mut()? {
        Overlay::PathEdit { value } => match msg {
            PaneMsg::PathEditInput(v) => {
                *value = v.clone();
                Some(vec![])
            }
            PaneMsg::PathEditCommit => {
                let target = std::path::PathBuf::from(value.trim());
                p.overlay = None;
                if target.as_os_str().is_empty() || target == p.cur_path {
                    return Some(vec![]);
                }
                Some(navigate_or_new_tab(p, id, locked, target))
            }
            PaneMsg::PathEditCancel | PaneMsg::ClearSelection => {
                p.overlay = None;
                Some(vec![])
            }
            // マウス操作 (一覧・ツリー・ボタンのクリック) は編集を破棄して
            // その操作をそのまま通す (エクスプローラ準拠: パスバー編集中でも
            // 1 クリック目からクリック先が効く)。キー操作はオーバーレイ表示中
            // GUI 層で止まるため、これらがここへ届くのはマウス由来のときだけ
            PaneMsg::RowPressed { .. }
            | PaneMsg::RowReleased { .. }
            | PaneMsg::RowDoubleClicked { .. }
            | PaneMsg::BlankPressed
            | PaneMsg::BandSelect { .. }
            | PaneMsg::RightPressed { .. }
            | PaneMsg::OpenMenu { .. }
            | PaneMsg::ShellMenuRequest { .. }
            | PaneMsg::HeaderClicked(_)
            | PaneMsg::ColResized { .. }
            | PaneMsg::NavigateTo(_)
            | PaneMsg::GoParent
            | PaneMsg::GoBack
            | PaneMsg::GoForward
            | PaneMsg::Reload => {
                p.overlay = None;
                None
            }
            _ => Some(vec![]),
        },
        Overlay::Modal { kind, value } => match msg {
            PaneMsg::ModalInput(v) => {
                *value = v.clone();
                Some(vec![])
            }
            // 確定は kind ごとに検証と Effect 発行を持つ (経路は一本)。
            // 検証 NG はモーダルを開いたまま。作成/リネーム後は watcher reload で
            // その名前へカーソルが乗るよう pending_cursor_name を仕込む
            PaneMsg::ModalCommit => match kind {
                // F7 (is_multiline): 1 行 1 フォルダの一括作成。既存名との衝突は
                // create_dir_all が冪等に吸収するため検査しない
                ModalKind::NewFolder => match parse_folder_names(value) {
                    Err(bad) => {
                        // 不正な行は理由を通知してモーダルを開いたまま
                        // (無反応だとどの行が悪いか分からない)
                        p.status_msg = Some(invalid_name_message(&bad));
                        Some(vec![])
                    }
                    // 有効な行なし (実質未入力) はモーダルを開いたまま
                    Ok(names) if names.is_empty() => Some(vec![]),
                    Ok(names) => {
                        p.overlay = None;
                        // カーソルは reload で先頭の名前へ
                        p.pending_cursor_name = names.first().cloned();
                        Some(vec![Effect::CreateDirs {
                            paths: names.iter().map(|name| p.cur_path.join(name)).collect(),
                        }])
                    }
                },
                ModalKind::Rename { original } => {
                    let name = match check_name(value) {
                        NameCheck::Empty => return Some(vec![]),
                        NameCheck::Invalid(bad) => {
                            p.status_msg = Some(invalid_name_message(&bad));
                            return Some(vec![]);
                        }
                        NameCheck::Ok(name) => name,
                    };
                    // 同名は何もせず閉じるだけ
                    if name == *original {
                        p.overlay = None;
                        return Some(vec![]);
                    }
                    // 既存名は確定拒否 (GPUI 版は no_overwrite でエラーにする —
                    // 事前検査で同じ結果に)
                    if p.entries.iter().any(|e| *e.name == *name) {
                        p.status_msg = Some(clash_message(&name));
                        return Some(vec![]);
                    }
                    let original = original.clone();
                    p.overlay = None;
                    p.pending_cursor_name = Some(name.clone());
                    Some(vec![Effect::Rename {
                        from: p.cur_path.join(original),
                        to: p.cur_path.join(name),
                    }])
                }
                ModalKind::NewFile => {
                    let name = match check_name(value) {
                        NameCheck::Empty => return Some(vec![]),
                        NameCheck::Invalid(bad) => {
                            p.status_msg = Some(invalid_name_message(&bad));
                            return Some(vec![]);
                        }
                        NameCheck::Ok(name) => name,
                    };
                    // 既存名は確定拒否 (GPUI 版は create_new でエラーにする)
                    if p.entries.iter().any(|e| *e.name == *name) {
                        p.status_msg = Some(clash_message(&name));
                        return Some(vec![]);
                    }
                    p.overlay = None;
                    p.pending_cursor_name = Some(name.clone());
                    Some(vec![Effect::CreateFile {
                        dir: p.cur_path.clone(),
                        name,
                    }])
                }
            },
            PaneMsg::ModalCancel | PaneMsg::ClearSelection => {
                p.overlay = None;
                Some(vec![])
            }
            _ => Some(vec![]),
        },
        Overlay::DropMenu {
            items, paths, dest, ..
        } => match msg {
            PaneMsg::MenuClicked(path) => {
                let Some(&ix) = path.first() else {
                    return Some(vec![]);
                };
                let Some(item) = items.get(ix) else {
                    return Some(vec![]);
                };
                let action = item.action.clone();
                let paths = paths.clone();
                let dest = dest.clone();
                p.overlay = None;
                // DropMenu の Copy/Cut は「ここにコピー/ここに移動」(F-605)。
                // UserCommand は {paths}=ドラッグ項目 / {cwd}=ドロップ先 (COMMANDS.md)
                Some(match action {
                    MenuAction::Copy => vec![Effect::DropTransfer {
                        pane: id,
                        op: crate::transfer::TransferOp::Copy,
                        paths,
                        dest,
                    }],
                    MenuAction::Cut => vec![Effect::DropTransfer {
                        pane: id,
                        op: crate::transfer::TransferOp::Move,
                        paths,
                        dest,
                    }],
                    MenuAction::UserCommand(cmd_id) => vec![Effect::RunUserCommand {
                        id: cmd_id,
                        paths,
                        cwd: dest,
                    }],
                    _ => vec![], // キャンセル
                })
            }
            PaneMsg::MenuClose | PaneMsg::ClearSelection | PaneMsg::BlankPressed => {
                p.overlay = None;
                Some(vec![])
            }
            _ => Some(vec![]),
        },
        Overlay::ContextMenu {
            items,
            open_path,
            target_row,
            templates_dir,
            ..
        } => match msg {
            PaneMsg::MenuClicked(path) => {
                // index チェーンで項目を解決
                let mut cur: &[MenuItem] = items;
                let mut found: Option<&MenuItem> = None;
                for &ix in path {
                    let Some(item) = cur.get(ix) else {
                        return Some(vec![]);
                    };
                    found = Some(item);
                    cur = &item.children;
                }
                let Some(item) = found else {
                    return Some(vec![]);
                };
                if !item.enabled {
                    return Some(vec![]);
                }
                if !item.children.is_empty() {
                    // サブメニューのクリック開閉 (同じなら閉じ、別なら切替 — USAGE §2)
                    if open_path.as_slice() == path.as_slice() {
                        open_path.clear();
                    } else {
                        *open_path = path.clone();
                    }
                    return Some(vec![]);
                }
                let action = item.action.clone();
                let target = *target_row;
                let tdir = templates_dir.clone();
                p.overlay = None;
                Some(run_menu_action(p, id, locked, action, target, tdir))
            }
            PaneMsg::MenuClose | PaneMsg::ClearSelection | PaneMsg::BlankPressed => {
                p.overlay = None;
                Some(vec![])
            }
            _ => Some(vec![]),
        },
        Overlay::Conflict { plan } => match msg {
            PaneMsg::Conflict(choice) => {
                let plan = plan.clone();
                p.overlay = None;
                if *choice == ConflictChoice::Cancel {
                    return Some(vec![]);
                }
                let existing = existing_names(p);
                let items = transfer::resolve_conflicts(&plan, *choice, &existing);
                if items.is_empty() {
                    return Some(vec![]);
                }
                Some(vec![Effect::SpawnJob {
                    pane: id,
                    op: plan.op,
                    items,
                }])
            }
            PaneMsg::ClearSelection => {
                p.overlay = None;
                Some(vec![])
            }
            _ => Some(vec![]),
        },
        Overlay::VirtualConflict { plan } => match msg {
            PaneMsg::Conflict(choice) => {
                let plan = plan.clone();
                p.overlay = None;
                if *choice == ConflictChoice::Cancel {
                    return Some(vec![]);
                }
                let existing = existing_names(p);
                let entries = transfer::resolve_virtual_conflicts(&plan, *choice, &existing);
                if entries.is_empty() {
                    return Some(vec![]);
                }
                Some(vec![Effect::PasteVirtual {
                    pane: id,
                    dest: plan.dest,
                    entries,
                }])
            }
            PaneMsg::ClearSelection => {
                p.overlay = None;
                Some(vec![])
            }
            _ => Some(vec![]),
        },
    }
}

/// Windows のファイル/フォルダ名に使えない文字 (パス区切り含む)。
const INVALID_NAME_CHARS: [char; 9] = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

/// 名前 1 件の検証結果 (F2/F8 の単一行と F7 の行ごとで共通)。
enum NameCheck {
    /// 空 (実質未入力) — 確定させない (無通知)。
    Empty,
    /// 使えない文字入り — 確定させず invalid_name_message で通知する。
    Invalid(String),
    Ok(String),
}

/// trim + Windows の名前規則で 1 件検証する。区切り文字を許すと
/// 「D:notes」のようなドライブ相対名が cur_path.join で別ドライブへ
/// すり替わる (F7 レビューの知見 — F2/F8 では ADS 誤作成も防ぐ)。
fn check_name(value: &str) -> NameCheck {
    let name = value.trim();
    if name.is_empty() {
        NameCheck::Empty
    } else if name.contains(INVALID_NAME_CHARS) || name.chars().any(char::is_control) {
        NameCheck::Invalid(name.to_string())
    } else {
        NameCheck::Ok(name.to_string())
    }
}

/// 使えない文字を含む名前の通知文言 (F2/F7/F8 共通)。
fn invalid_name_message(name: &str) -> String {
    format!("「{name}」は名前に使えません (\\ / : * ? \" < > | と制御文字は不可)")
}

/// F7 の複数行入力を行ごとのフォルダ名に分解する (1 行 1 フォルダ)。
/// 前後空白は除去・空行は無視・重複行は先勝ちで 1 つに畳む (NTFS は大文字
/// 小文字を区別しないため、大小違いだけの行も同一視 — 2 行受理して 1 つしか
/// できない「黙った半分成功」を防ぐ)。
/// エディタ (cosmic-text) は孤立 \r も改行として表示するため、分割は
/// \n / \r の両方で行う (str::lines は孤立 \r を割らず、表示と作成結果が
/// 食い違う)。使えない文字を含む行は Err(その行) — 呼び出し側で通知する。
fn parse_folder_names(value: &str) -> Result<Vec<String>, String> {
    let mut seen = std::collections::HashSet::new();
    let mut names: Vec<String> = Vec::new();
    for line in value.split(['\n', '\r']) {
        match check_name(line) {
            NameCheck::Empty => continue,
            NameCheck::Invalid(bad) => return Err(bad),
            NameCheck::Ok(name) => {
                if seen.insert(name.to_lowercase()) {
                    names.push(name);
                }
            }
        }
    }
    Ok(names)
}

/// 既存名と衝突したときの通知文言 (F2/F8 共通)。
fn clash_message(name: &str) -> String {
    format!("「{name}」は既に存在します (Windows では同名のファイルとフォルダは共存できません)")
}

/// メニュー項目の実行 (メニューは閉じた後に呼ばれる)。
fn run_menu_action(
    p: &mut PaneState,
    id: PaneId,
    locked: bool,
    action: MenuAction,
    target_row: Option<usize>,
    templates_dir: String,
) -> Vec<Effect> {
    match action {
        MenuAction::Open => match target_row.or(p.cursor) {
            Some(ix) => activate(p, id, locked, ix),
            None => vec![],
        },
        MenuAction::Copy => clipboard_write(p, "copy"),
        MenuAction::Cut => clipboard_write(p, "cut"),
        MenuAction::Paste => vec![Effect::ClipboardRead { pane: id }],
        MenuAction::Rename => update_pane(p, id, locked, PaneMsg::OpenRename),
        MenuAction::Delete => update_pane(p, id, locked, PaneMsg::RequestDelete),
        MenuAction::Refresh => reload(p, id),
        MenuAction::NewFolder => update_pane(p, id, locked, PaneMsg::OpenNewFolder),
        MenuAction::NewFileEmpty => update_pane(p, id, locked, PaneMsg::OpenNewFile),
        MenuAction::NewFileTemplate(template) => vec![Effect::CreateFromTemplate {
            dir: p.cur_path.clone(),
            template,
        }],
        MenuAction::OpenTemplatesDir => {
            navigate_or_new_tab(p, id, locked, std::path::PathBuf::from(templates_dir))
        }
        MenuAction::UserCommand(cmd_id) => {
            let paths = p.selected_paths();
            vec![Effect::RunUserCommand {
                id: cmd_id,
                paths,
                cwd: p.cur_path.clone(),
            }]
        }
        MenuAction::Properties => {
            // 行の上 = その行のプロパティ / 背景 = 現在フォルダのプロパティ
            // (背景では p.cursor へフォールバックしない — エクスプローラ準拠)。
            let path = match target_row.and_then(|ix| p.entries.get(ix)) {
                Some(e) => p.cur_path.join(&*e.name),
                None => p.cur_path.clone(),
            };
            vec![Effect::ShowProperties { path }]
        }
        MenuAction::Submenu => vec![], // キャンセル等 (閉じるだけ)
    }
}

fn self_gen(p: &PaneState) -> u64 {
    p.load_gen
}

/// domain イベント (watcher / ジョブ進捗 / 検索結果) の反映。
/// update_pane の match から分離 (イベント追加時の変更範囲をここに閉じる)。
fn update_domain_event(p: &mut PaneState, id: PaneId, ev: DomainEvent) -> Vec<Effect> {
    match ev {
        DomainEvent::FsChange { path } => {
            // 監視は現在フォルダのみだが、遅れて届く旧パスのイベントは無視
            if path != p.cur_path.to_string_lossy() {
                return vec![];
            }
            p.reload_seq += 1;
            vec![Effect::Debounce {
                pane: id,
                seq: p.reload_seq,
                millis: RELOAD_DEBOUNCE_MS,
            }]
        }
        DomainEvent::JobProgress {
            job_id,
            kind,
            done_files,
            total_files,
            done_bytes,
            total_bytes,
            current,
        } => {
            p.job = Some(JobStatus {
                id: job_id,
                kind,
                done_files,
                total_files,
                done_bytes,
                total_bytes,
                current,
            });
            vec![]
        }
        DomainEvent::JobDone {
            ok,
            canceled,
            error,
            ..
        } => {
            p.job = None;
            p.status_msg = if canceled {
                Some("キャンセルしました".into())
            } else if let Some(e) = error {
                Some(format!("エラー: {e}"))
            } else if !ok {
                Some("一部の項目を処理できませんでした".into())
            } else {
                None // 成功は無言
            };
            // watcher が効かないフォルダ (ネットワークドライブ等) でも結果を
            // 反映させるため明示 reload (GPUI 版パリティ。watcher と二重に
            // なっても世代キャンセルで最後の 1 回だけが反映される)
            reload(p, id)
        }
        DomainEvent::SearchHit {
            job_id,
            path,
            name,
            is_dir,
        } => {
            if let Some(sui) = &mut p.search {
                // 旧検索の遅延ヒットを捨てる (SearchDone と同じガード)。
                // job_id は StartSearch 実行側が検索スレッド起動前に
                // SearchStarted で同期確定させているため、新検索のヒットを
                // 誤って落とすことはない (app.rs:1144 の先例パターン)
                if sui.job_id != Some(job_id) {
                    return vec![];
                }
                if sui.showing {
                    let pb = std::path::PathBuf::from(&path);
                    let ext = (!is_dir)
                        .then(|| name.rsplit_once('.').map(|(_, e)| e.to_string()))
                        .flatten();
                    // 検索結果行: 名前列にフルパスの親を出す代わりに名前のみ
                    // (GPUI 版と同じ見た目は Phase 7 で照合)
                    sui.hits.push(Entry::new(name, is_dir, 0, 0, ext, false));
                    sui.hit_paths.push((pb, is_dir));
                }
            }
            vec![]
        }
        DomainEvent::SearchDone {
            job_id,
            total,
            canceled,
            fallback,
            error,
        } => {
            if let Some(sui) = &mut p.search {
                if sui.job_id != Some(job_id) {
                    return vec![]; // 旧検索の完了通知は無視
                }
                sui.running = false;
                sui.summary = Some(if let Some(e) = error {
                    format!("検索エラー: {e}")
                } else if canceled {
                    "検索をキャンセルしました".into()
                } else if fallback {
                    format!("{total} 件 (内蔵検索)")
                } else {
                    format!("{total} 件")
                });
            }
            vec![]
        }
        DomainEvent::Unknown { .. } => vec![],
    }
}

/// 検索バー系メッセージ (F-701/F-702) の処理。update_pane の match から分離。
fn update_search(p: &mut PaneState, id: PaneId, msg: PaneMsg) -> Vec<Effect> {
    match msg {
        PaneMsg::OpenSearch => {
            if p.search.is_none() {
                p.search = Some(SearchUi::default());
            }
            vec![]
        }
        PaneMsg::SearchInput(v) => {
            if let Some(s) = &mut p.search {
                s.query = v;
            }
            vec![]
        }
        PaneMsg::SearchCommit => {
            let Some(sui) = &mut p.search else {
                return vec![];
            };
            let query = sui.query.trim().to_string();
            if query.is_empty() {
                return vec![];
            }
            sui.running = true;
            sui.showing = true;
            sui.hits.clear();
            sui.hit_paths.clear();
            sui.summary = None;
            sui.job_id = None; // StartSearch 実行側が SearchStarted 相当で確定する
            p.cursor = None;
            p.selected.clear();
            // 表示リストが hits (空) に切り替わる — 深スクロールを持ち越すと
            // ヒットテストと描画がずれる (row_at のクランプと対の対策)
            p.scroll_offset = 0.0;
            vec![Effect::StartSearch {
                pane: id,
                root: p.cur_path.clone(),
                query,
            }]
        }
        PaneMsg::SearchClose => {
            let job = p.search.take().and_then(|s| s.job_id);
            p.cursor = None;
            p.selected.clear();
            // 結果リストの深スクロールが entries の範囲外に残らないように
            p.scroll_offset = p.scroll_offset.clamp(0.0, p.max_scroll());
            // 走り続けている内蔵検索 (ドライブ全走査等) をバーと一緒に止める
            match job {
                Some(job_id) => vec![Effect::CancelSearch { job_id }],
                None => vec![],
            }
        }
        PaneMsg::SearchStarted(job_id) => {
            if let Some(sui) = &mut p.search {
                sui.job_id = Some(job_id);
            }
            vec![]
        }
        _ => vec![], // dispatcher が検索系のみ渡す
    }
}

/// 行の活性化: フォルダ = 移動 / ファイル = 既定アプリで開く (F-301)。
/// 検索結果リスト表示中は結果パス基準 (フォルダ = 開く / ファイル = 親を開いて選択 — F-701)。
/// ロックタブではフォルダ進入を新タブへ逃がす (F-104)。
fn activate(p: &mut PaneState, id: PaneId, locked: bool, ix: usize) -> Vec<Effect> {
    if let Some(sui) = &p.search {
        if sui.showing {
            let Some((path, is_dir)) = sui.hit_paths.get(ix).cloned() else {
                return vec![];
            };
            // ヒットを開いたら検索バーごと閉じる — 実行中の検索も止める
            let cancel = p
                .search
                .take()
                .and_then(|s| s.job_id)
                .map(|job_id| Effect::CancelSearch { job_id });
            let push_cancel = |mut fx: Vec<Effect>| {
                fx.extend(cancel);
                fx
            };
            return if is_dir {
                push_cancel(navigate_or_new_tab(p, id, locked, path))
            } else {
                let parent = path
                    .parent()
                    .map(|x| x.to_path_buf())
                    .unwrap_or_else(|| p.cur_path.clone());
                let name = path.file_name().map(|s| s.to_string_lossy().to_string());
                let fx = navigate_or_new_tab(p, id, locked, parent);
                // ロックタブは新タブへ逃げて自ペインは動かない — pending を
                // 設定すると無関係な後続 reload でカーソルが飛ぶ (GoParent と同じ判定)
                if !locked {
                    p.pending_cursor_name = name;
                }
                push_cancel(fx)
            };
        }
    }
    let Some(entry) = p.entries.get(ix) else {
        return vec![];
    };
    let target = p.cur_path.join(&*entry.name);
    if entry.is_dir {
        navigate_or_new_tab(p, id, locked, target)
    } else {
        vec![Effect::OpenFile { path: target }]
    }
}

/// ロックタブなら移動せず `OpenTabFor` を返す (F-104)。
fn navigate_or_new_tab(
    p: &mut PaneState,
    id: PaneId,
    locked: bool,
    path: std::path::PathBuf,
) -> Vec<Effect> {
    if locked {
        vec![Effect::OpenTabFor { path }]
    } else {
        navigate(p, id, path)
    }
}

/// フォルダ移動: 履歴に現在地を積んでから移動する (F-303)。
pub fn navigate(p: &mut PaneState, id: PaneId, path: std::path::PathBuf) -> Vec<Effect> {
    if path != p.cur_path {
        p.history_back.push(p.cur_path.clone());
        p.history_fwd.clear();
    }
    set_path_and_load(p, id, path)
}

/// 履歴操作用: 履歴を触らずにパスを差し替えて読み込む。
fn set_path_and_load(p: &mut PaneState, id: PaneId, path: std::path::PathBuf) -> Vec<Effect> {
    // ツリー/入力/セッション由来の表記ゆれ (連続 \ ・末尾 \ ・/) をここで正す
    let path = crate::model::normalize_path(&path);
    p.cur_path = path.clone();
    p.cursor = None;
    p.anchor = None;
    p.selected.clear();
    p.pending_cursor_name = None;
    p.pending_click = None;
    p.scroll_offset = 0.0;
    start_load(p, id, path)
}

/// F5 / watcher の再読み込み: 選択・カーソル・スクロールを維持したまま読み直す
/// (選択は Loaded 側で名前復元される — USAGE.md §2)。
pub fn reload(p: &mut PaneState, id: PaneId) -> Vec<Effect> {
    // これから取る一覧が最新になるので、係留中の watcher デバウンス tick は
    // 不要 — seq を進めて無効化する (明示 reload + watcher で listing が
    // 二重に走るのを防ぐ。reload 開始後の FsChange は新しい seq で再デバウンス)
    p.reload_seq += 1;
    start_load(p, id, p.cur_path.clone())
}

/// 並べ替え (列見出しクリック): 置換ベクタで entries を並べ替え、選択・カーソル・
/// アンカーの index を直接再マップする。旧実装の「名前スナップショット + 名前復元」は
/// 選択行数ぶんの String clone を伴い、内容が変わらない並べ替えには過剰だった。
/// (entries が差し替わる Loaded は従来どおり名前復元 — restore_selection)
fn resort_keeping_selection(p: &mut PaneState) {
    let n = p.entries.len();
    if n == 0 {
        return;
    }
    let sort = p.sort;
    let mut order: Vec<u32> = (0..n as u32).collect();
    // sort_by は安定ソートなので sort_entries と同一の並びになる (entry_cmp 共用)
    order.sort_by(|&a, &b| {
        crate::model::entry_cmp(&p.entries[a as usize], &p.entries[b as usize], sort)
    });
    let old = std::mem::take(&mut p.entries);
    let mut slots: Vec<Option<Entry>> = old.into_iter().map(Some).collect();
    p.entries = order
        .iter()
        .map(|&i| slots[i as usize].take().expect("順列は重複しない"))
        .collect();
    // 旧 index → 新 index の対応表で選択系を写す
    let mut new_pos = vec![0u32; n];
    for (new_ix, &old_ix) in order.iter().enumerate() {
        new_pos[old_ix as usize] = new_ix as u32;
    }
    // 範囲外 index (検索結果リスト表示中の選択 = hits 空間) は旧実装と同じく黙って落とす
    let remap = |i: usize| new_pos.get(i).map(|&x| x as usize);
    p.selected = p.selected.iter().filter_map(|&i| remap(i)).collect();
    p.cursor = p.cursor.and_then(remap);
    p.anchor = p.anchor.and_then(remap);
}

fn start_load(p: &mut PaneState, id: PaneId, path: std::path::PathBuf) -> Vec<Effect> {
    p.load_gen += 1;
    p.loading = true;
    p.load_error = None;
    vec![Effect::LoadDir {
        pane: id,
        generation: p.load_gen,
        path,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Entry, SortState};
    use std::path::PathBuf;

    fn entry(name: &str, is_dir: bool) -> Entry {
        let ext = (!is_dir)
            .then(|| name.rsplit_once('.').map(|(_, e)| e.to_string()))
            .flatten();
        Entry::new(name.to_string(), is_dir, 1, 100, ext, false)
    }

    fn pane_with(names: &[(&str, bool)]) -> PaneState {
        let mut p = PaneState::new(PathBuf::from("C:\\root"));
        p.entries = names.iter().map(|(n, d)| entry(n, *d)).collect();
        p.viewport_h = 240.0;
        p
    }

    #[test]
    fn double_click_dir_navigates_and_bumps_generation() {
        let mut p = pane_with(&[("sub", true), ("a.txt", false)]);
        let fx = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::RowDoubleClicked { ix: 0 },
        );
        assert_eq!(p.cur_path, PathBuf::from("C:\\root\\sub"));
        assert!(p.loading);
        assert_eq!(
            fx,
            vec![Effect::LoadDir {
                pane: PaneId::default(),
                generation: 1,
                path: PathBuf::from("C:\\root\\sub")
            }]
        );
    }

    #[test]
    fn double_click_file_opens() {
        let mut p = pane_with(&[("a.txt", false)]);
        let fx = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::RowDoubleClicked { ix: 0 },
        );
        assert_eq!(
            fx,
            vec![Effect::OpenFile {
                path: PathBuf::from("C:\\root\\a.txt")
            }]
        );
        assert!(!p.loading);
    }

    #[test]
    fn stale_generation_result_is_dropped() {
        let mut p = pane_with(&[]);
        // 2 回ナビゲート → gen=2。gen=1 の結果は捨てる
        navigate(&mut p, PaneId::default(), PathBuf::from("C:\\a"));
        navigate(&mut p, PaneId::default(), PathBuf::from("C:\\b"));
        let fx = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Loaded {
                generation: 1,
                entries: vec![entry("stale.txt", false)],
            },
        );
        assert!(fx.is_empty());
        assert!(p.entries.is_empty());
        assert!(p.loading);
        // 現世代は反映される
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Loaded {
                generation: 2,
                entries: vec![entry("fresh.txt", false)],
            },
        );
        assert_eq!(&*p.entries[0].name, "fresh.txt");
        assert!(!p.loading);
    }

    #[test]
    fn loaded_sorts_and_keeps_selection_by_name() {
        let mut p = pane_with(&[("b.txt", false), ("a.txt", false)]);
        p.click_row(0, false, false); // b.txt を選択
        let fx = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Loaded {
                generation: 0,
                entries: vec![
                    entry("b.txt", false),
                    entry("dir", true),
                    entry("a.txt", false),
                ],
            },
        );
        assert!(fx.is_empty());
        let names: Vec<_> = p.entries.iter().map(|e| &*e.name).collect();
        assert_eq!(names, ["dir", "a.txt", "b.txt"]); // dir 先頭 + 名前順
        let sel: Vec<_> = p.selected.iter().map(|&i| &*p.entries[i].name).collect();
        assert_eq!(sel, ["b.txt"]); // 名前で維持
    }

    #[test]
    fn closing_search_cancels_running_job() {
        // バーを閉じる / Esc / ヒットを開く、のどの経路でも実行中検索を止める (P-2)
        let close_paths: [fn(&mut PaneState) -> Vec<Effect>; 2] = [
            |p| update_pane(p, PaneId::default(), false, PaneMsg::SearchClose),
            |p| update_pane(p, PaneId::default(), false, PaneMsg::ClearSelection),
        ];
        for close in close_paths {
            let mut p = pane_with(&[]);
            update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenSearch);
            update_pane(
                &mut p,
                PaneId::default(),
                false,
                PaneMsg::SearchInput("q".into()),
            );
            update_pane(&mut p, PaneId::default(), false, PaneMsg::SearchCommit);
            update_pane(&mut p, PaneId::default(), false, PaneMsg::SearchStarted(9));
            let fx = close(&mut p);
            assert!(fx.contains(&Effect::CancelSearch { job_id: 9 }));
        }
        // ヒット活性化でも止まる
        let mut p = pane_with(&[]);
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenSearch);
        update_pane(&mut p, PaneId::default(), false, PaneMsg::SearchStarted(9));
        let sui = p.search.as_mut().unwrap();
        sui.showing = true;
        sui.hits.push(entry("dir", true));
        sui.hit_paths.push((PathBuf::from("C:\\dir"), true));
        let fx = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::RowDoubleClicked { ix: 0 },
        );
        assert!(fx.contains(&Effect::CancelSearch { job_id: 9 }));
        // 未起動 (job_id 未確定) なら Cancel は出ない
        let mut p = pane_with(&[]);
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenSearch);
        let fx = update_pane(&mut p, PaneId::default(), false, PaneMsg::SearchClose);
        assert!(fx.is_empty());
    }

    #[test]
    fn header_sort_remaps_selection_indices() {
        // 並べ替えで選択・カーソルが同じ「行 (中身)」に付いていく (M-6 の index 再マップ)
        let mut p = pane_with(&[("b.txt", false), ("a.txt", false), ("c.txt", false)]);
        p.click_row(0, false, false); // b.txt
        p.click_row(2, true, false); // + c.txt
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::HeaderClicked(Column::Name), // 既定 Name/asc → 降順トグル
        );
        let names: Vec<_> = p.entries.iter().map(|e| &*e.name).collect();
        assert_eq!(names, ["c.txt", "b.txt", "a.txt"]);
        let sel: Vec<_> = p.selected.iter().map(|&i| &*p.entries[i].name).collect();
        assert_eq!(sel, ["c.txt", "b.txt"]);
        assert_eq!(p.cursor.map(|i| &*p.entries[i].name), Some("c.txt"));
    }

    #[test]
    fn stale_search_hits_are_dropped() {
        // 検索を打ち直したとき、旧 job のヒットが新結果に混入しない (B-1)
        let mut p = pane_with(&[]);
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenSearch);
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::SearchInput("a".into()),
        );
        update_pane(&mut p, PaneId::default(), false, PaneMsg::SearchCommit);
        update_pane(&mut p, PaneId::default(), false, PaneMsg::SearchStarted(2));
        let hit = |job_id: u64| {
            PaneMsg::Domain(DomainEvent::SearchHit {
                job_id,
                path: "C:\\x\\old.txt".into(),
                name: "old.txt".into(),
                is_dir: false,
            })
        };
        // 旧 job (1) のヒットは捨てられ、現 job (2) のヒットだけ載る
        update_pane(&mut p, PaneId::default(), false, hit(1));
        assert_eq!(p.search.as_ref().unwrap().hits.len(), 0);
        update_pane(&mut p, PaneId::default(), false, hit(2));
        assert_eq!(p.search.as_ref().unwrap().hits.len(), 1);
    }

    #[test]
    fn locked_tab_search_activate_does_not_leak_pending_cursor() {
        // ロックタブで検索ヒット (ファイル) を開いても、自ペインに
        // pending_cursor_name が残らない (B-2 — 後続 reload でカーソルが飛ぶ)
        let mut p = pane_with(&[]);
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenSearch);
        let sui = p.search.as_mut().unwrap();
        sui.showing = true;
        sui.hits.push(entry("hit.txt", false));
        sui.hit_paths
            .push((PathBuf::from("C:\\elsewhere\\hit.txt"), false));
        let fx = update_pane(
            &mut p,
            PaneId::default(),
            true, // locked
            PaneMsg::RowDoubleClicked { ix: 0 },
        );
        assert!(matches!(&fx[0], Effect::OpenTabFor { .. }));
        assert_eq!(p.pending_cursor_name, None);
        // 非ロックなら従来どおり pending が立つ
        let mut p = pane_with(&[]);
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenSearch);
        let sui = p.search.as_mut().unwrap();
        sui.showing = true;
        sui.hits.push(entry("hit.txt", false));
        sui.hit_paths
            .push((PathBuf::from("C:\\elsewhere\\hit.txt"), false));
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::RowDoubleClicked { ix: 0 },
        );
        assert_eq!(p.pending_cursor_name.as_deref(), Some("hit.txt"));
    }

    #[test]
    fn closing_search_clamps_scroll_into_entries_range() {
        // 結果リストで深くスクロール → 閉じたとき entries の範囲へ戻る (B-6)
        let mut p = pane_with(&[("a.txt", false), ("b.txt", false)]);
        p.viewport_h = 240.0;
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenSearch);
        p.scroll_offset = 5000.0; // 結果リストでの深スクロールを模擬
        update_pane(&mut p, PaneId::default(), false, PaneMsg::SearchClose);
        assert!(p.scroll_offset <= p.max_scroll());
        // Esc (ClearSelection) 経由でも同様
        let mut p = pane_with(&[("a.txt", false)]);
        p.viewport_h = 240.0;
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenSearch);
        p.scroll_offset = 5000.0;
        update_pane(&mut p, PaneId::default(), false, PaneMsg::ClearSelection);
        assert!(p.search.is_none());
        assert!(p.scroll_offset <= p.max_scroll());
    }

    #[test]
    fn press_on_selected_row_defers_collapse_until_release() {
        let mut p = pane_with(&[("a.txt", false), ("b.txt", false), ("c.txt", false)]);
        p.click_row(0, false, false);
        p.click_row(2, true, false); // {0, 2}

        // 選択済み行の修飾なし押下では選択を崩さない (左ドラッグで選択全体を運ぶため)
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::RowPressed {
                ix: 2,
                ctrl: false,
                shift: false,
            },
        );
        assert!(p.selected.iter().eq(&[0, 2]));
        assert_eq!(p.pending_click, Some(2));
        // ドラッグに至らず離した → 単一選択へ確定
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::RowReleased { ix: 2 },
        );
        assert!(p.selected.iter().eq(&[2]));
        assert_eq!(p.cursor, Some(2));
        assert_eq!(p.pending_click, None);
    }

    #[test]
    fn press_on_unselected_row_collapses_immediately() {
        let mut p = pane_with(&[("a.txt", false), ("b.txt", false), ("c.txt", false)]);
        p.click_row(0, false, false);
        p.click_row(2, true, false); // {0, 2}
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::RowPressed {
                ix: 1,
                ctrl: false,
                shift: false,
            },
        );
        assert!(p.selected.iter().eq(&[1]));
        // 対応する押下保留が無い Release は何もしない
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::RowReleased { ix: 0 },
        );
        assert!(p.selected.iter().eq(&[1]));
    }

    #[test]
    fn pending_click_is_dropped_on_reload() {
        // ドラッグ開始時は RowReleased が来ない — 選択は維持されたまま。
        // その後の Loaded で保留は破棄され、遅れて届いた Release で誤発火しない
        let mut p = pane_with(&[("a.txt", false), ("b.txt", false)]);
        p.click_row(0, false, false);
        p.click_row(1, true, false); // {0, 1}
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::RowPressed {
                ix: 0,
                ctrl: false,
                shift: false,
            },
        );
        assert!(p.selected.iter().eq(&[0, 1])); // ドラッグはこの選択全体を運べる
        let generation = p.load_gen;
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Loaded {
                generation,
                entries: vec![entry("a.txt", false), entry("b.txt", false)],
            },
        );
        assert_eq!(p.pending_click, None);
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::RowReleased { ix: 0 },
        );
        assert_eq!(p.selected.len(), 2); // 破棄済みなので選択は崩れない
    }

    #[test]
    fn navigate_normalizes_double_separators() {
        // ツリーのドライブ行由来の "D:\\AI" (二重区切り) も、移動時に正規化される
        let mut p = pane_with(&[]);
        let fx = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::NavigateTo(PathBuf::from("D:\\\\AI\\comfy")),
        );
        assert_eq!(p.cur_path.to_string_lossy(), "D:\\AI\\comfy");
        // 読み込み効果へ渡るパスも正規化済み
        assert!(
            matches!(&fx[0], Effect::LoadDir { path, .. } if path.to_string_lossy() == "D:\\AI\\comfy")
        );
    }

    #[test]
    fn load_failed_after_navigate_reverts_to_shown_folder() {
        let mut p = pane_with(&[("sub", true)]);
        let generation = p.load_gen;
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Loaded {
                generation,
                entries: vec![entry("sub", true)],
            },
        );
        // 存在しないパスへ移動を試みる → 失敗
        navigate(&mut p, PaneId::default(), PathBuf::from("C:\\nope"));
        assert_eq!(p.history_back.last(), Some(&PathBuf::from("C:\\root")));
        let generation = p.load_gen;
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::LoadFailed {
                generation,
                error: "not found: C:\\nope".into(),
            },
        );
        // cur_path は表示中の一覧のパスへ戻り、積んだ履歴も掃除される
        assert_eq!(p.cur_path, PathBuf::from("C:\\root"));
        assert!(p.history_back.is_empty());
        assert!(p.status_msg.as_deref().unwrap().contains("not found"));
        assert_eq!(p.load_error, None);
        // 一覧は旧フォルダのまま → ダブルクリックは正しいパスへ解決する
        let fx = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::RowDoubleClicked { ix: 0 },
        );
        assert!(
            matches!(&fx[0], Effect::LoadDir { path, .. } if path == &PathBuf::from("C:\\root\\sub"))
        );
    }

    #[test]
    fn load_failed_on_reload_keeps_current_path() {
        let mut p = pane_with(&[("a.txt", false)]);
        let generation = p.load_gen;
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Loaded {
                generation,
                entries: vec![entry("a.txt", false)],
            },
        );
        reload(&mut p, PaneId::default());
        let generation = p.load_gen;
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::LoadFailed {
                generation,
                error: "denied".into(),
            },
        );
        // その場の reload 失敗は移動を伴わない → 従来どおり load_error 表示
        assert_eq!(p.cur_path, PathBuf::from("C:\\root"));
        assert_eq!(p.load_error.as_deref(), Some("denied"));
    }

    #[test]
    fn load_failed_go_back_reverts_and_prunes_forward() {
        // C:\a → C:\b と移動後、C:\a が消えた状態で戻る → C:\b に留まる
        let mut p = pane_with(&[]);
        for dir in ["C:\\a", "C:\\b"] {
            navigate(&mut p, PaneId::default(), PathBuf::from(dir));
            let generation = p.load_gen;
            update_pane(
                &mut p,
                PaneId::default(),
                false,
                PaneMsg::Loaded {
                    generation,
                    entries: vec![],
                },
            );
        }
        update_pane(&mut p, PaneId::default(), false, PaneMsg::GoBack);
        assert_eq!(p.cur_path, PathBuf::from("C:\\a"));
        let generation = p.load_gen;
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::LoadFailed {
                generation,
                error: "not found".into(),
            },
        );
        assert_eq!(p.cur_path, PathBuf::from("C:\\b"));
        assert!(p.history_fwd.is_empty()); // GoBack が積んだ fwd を掃除
        assert_eq!(p.history_back.last(), Some(&PathBuf::from("C:\\root"))); // back は保持
    }

    #[test]
    fn go_parent_sets_pending_cursor_to_source_folder() {
        let mut p = pane_with(&[]);
        let fx = update_pane(&mut p, PaneId::default(), false, PaneMsg::GoParent);
        assert_eq!(p.cur_path, PathBuf::from("C:\\"));
        assert_eq!(p.pending_cursor_name.as_deref(), Some("root"));
        assert_eq!(fx.len(), 1);
        // Loaded で root にカーソルが乗る
        let generation = p.load_gen;
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Loaded {
                generation,
                entries: vec![entry("other", true), entry("root", true)],
            },
        );
        assert_eq!(p.cursor.map(|i| &*p.entries[i].name), Some("root"));
    }

    #[test]
    fn header_click_toggles_and_switches_column() {
        let mut p = pane_with(&[("a.txt", false), ("b.txt", false)]);
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::HeaderClicked(Column::Name),
        );
        assert_eq!(
            p.sort,
            SortState {
                col: Column::Name,
                asc: false
            }
        );
        assert_eq!(&*p.entries[0].name, "b.txt");
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::HeaderClicked(Column::Size),
        );
        assert_eq!(
            p.sort,
            SortState {
                col: Column::Size,
                asc: true
            }
        );
    }

    #[test]
    fn col_resize_clamps_and_ignores_name() {
        let mut p = pane_with(&[]);
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::ColResized {
                col: Column::Size,
                width: 10.0,
            },
        );
        assert_eq!(p.col_widths[1], COL_W_MIN);
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::ColResized {
                col: Column::Modified,
                width: 9999.0,
            },
        );
        assert_eq!(p.col_widths[0], COL_W_MAX);
        let before = p.col_widths;
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::ColResized {
                col: Column::Name,
                width: 100.0,
            },
        );
        assert_eq!(p.col_widths, before);
    }

    #[test]
    fn history_back_and_forward_roundtrip() {
        let mut p = pane_with(&[("sub", true)]);
        // C:\root → C:\root\sub → 戻る → 進む
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::RowDoubleClicked { ix: 0 },
        );
        assert_eq!(p.history_back, vec![PathBuf::from("C:\\root")]);
        let fx = update_pane(&mut p, PaneId::default(), false, PaneMsg::GoBack);
        assert_eq!(p.cur_path, PathBuf::from("C:\\root"));
        assert_eq!(p.history_fwd, vec![PathBuf::from("C:\\root\\sub")]);
        assert_eq!(fx.len(), 1);
        update_pane(&mut p, PaneId::default(), false, PaneMsg::GoForward);
        assert_eq!(p.cur_path, PathBuf::from("C:\\root\\sub"));
        assert!(p.history_fwd.is_empty());
        // 履歴が空なら何もしない
        let mut q = pane_with(&[]);
        assert!(update_pane(&mut q, PaneId::default(), false, PaneMsg::GoBack).is_empty());
        assert!(update_pane(&mut q, PaneId::default(), false, PaneMsg::GoForward).is_empty());
    }

    #[test]
    fn navigate_clears_forward_history() {
        let mut p = pane_with(&[("a", true), ("b", true)]);
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::RowDoubleClicked { ix: 0 },
        ); // → a
        update_pane(&mut p, PaneId::default(), false, PaneMsg::GoBack); // → root (fwd=[a])
        assert_eq!(p.history_fwd.len(), 1);
        // ここで別フォルダへ移動すると進む履歴は消える
        p.entries = vec![entry("b", true)];
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::RowDoubleClicked { ix: 0 },
        ); // → b
        assert!(p.history_fwd.is_empty());
    }

    #[test]
    fn reload_preserves_selection_and_scroll() {
        let mut p = pane_with(&[("a.txt", false), ("b.txt", false)]);
        p.click_row(1, false, false);
        p.scroll_offset = 12.0;
        p.viewport_h = 24.0;
        let fx = update_pane(&mut p, PaneId::default(), false, PaneMsg::Reload);
        assert!(matches!(fx[0], Effect::LoadDir { generation: 1, .. }));
        assert_eq!(p.scroll_offset, 12.0); // reload はスクロールを保つ
        let generation = p.load_gen;
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Loaded {
                generation,
                entries: vec![entry("b.txt", false), entry("a.txt", false)],
            },
        );
        // 選択は名前で維持される
        let sel: Vec<_> = p.selected.iter().map(|&i| &*p.entries[i].name).collect();
        assert_eq!(sel, ["b.txt"]);
    }

    #[test]
    fn fs_change_debounces_and_only_latest_tick_reloads() {
        use crate::domain_event::DomainEvent;
        let mut p = pane_with(&[]);
        let cur = p.cur_path.to_string_lossy().to_string();
        let fx1 = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Domain(DomainEvent::FsChange { path: cur.clone() }),
        );
        assert_eq!(
            fx1,
            vec![Effect::Debounce {
                pane: PaneId::default(),
                seq: 1,
                millis: 150
            }]
        );
        let fx2 = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Domain(DomainEvent::FsChange { path: cur }),
        );
        assert_eq!(
            fx2,
            vec![Effect::Debounce {
                pane: PaneId::default(),
                seq: 2,
                millis: 150
            }]
        );
        // 古い tick は無視、新しい tick で reload
        assert!(update_pane(&mut p, PaneId::default(), false, PaneMsg::ReloadTick(1)).is_empty());
        let fx3 = update_pane(&mut p, PaneId::default(), false, PaneMsg::ReloadTick(2));
        assert!(matches!(fx3[0], Effect::LoadDir { .. }));
        // 監視外パスの変化は無視
        let fx4 = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Domain(DomainEvent::FsChange {
                path: "D:\\other".into(),
            }),
        );
        assert!(fx4.is_empty());
    }

    #[test]
    fn explicit_reload_absorbs_pending_watcher_debounce() {
        // 自分の操作の明示 reload (OpDone 経路) が走ったら、係留中の watcher
        // デバウンス tick は不要 — listing が二重に走らない
        use crate::domain_event::DomainEvent;
        let mut p = pane_with(&[]);
        let cur = p.cur_path.to_string_lossy().to_string();
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Domain(DomainEvent::FsChange { path: cur.clone() }),
        );
        let pending_seq = p.reload_seq;
        // 明示 reload → 係留中の tick は stale になる
        let fx = update_pane(&mut p, PaneId::default(), false, PaneMsg::Reload);
        assert!(matches!(fx[0], Effect::LoadDir { .. }));
        assert!(
            update_pane(
                &mut p,
                PaneId::default(),
                false,
                PaneMsg::ReloadTick(pending_seq)
            )
            .is_empty(),
            "明示 reload 後の watcher tick が二重 reload している"
        );
        // reload 後の新しい変化は改めてデバウンス → tick で reload される
        let fx = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Domain(DomainEvent::FsChange { path: cur }),
        );
        let Effect::Debounce { seq, .. } = fx[0] else {
            panic!("FsChange が Debounce を出さない: {fx:?}");
        };
        let fx = update_pane(&mut p, PaneId::default(), false, PaneMsg::ReloadTick(seq));
        assert!(matches!(fx[0], Effect::LoadDir { .. }));
    }

    #[test]
    fn path_edit_overlay_flow() {
        let mut p = pane_with(&[("x.txt", false)]);
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenPathEdit);
        assert!(matches!(p.overlay, Some(Overlay::PathEdit { .. })));
        // オーバーレイ中は一覧キー操作が無効
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Nav(crate::NavKey::Down, false),
        );
        assert_eq!(p.cursor, None);
        // 入力 → 確定で navigate
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::PathEditInput("D:\\data".into()),
        );
        let fx = update_pane(&mut p, PaneId::default(), false, PaneMsg::PathEditCommit);
        assert!(p.overlay.is_none());
        assert_eq!(p.cur_path, PathBuf::from("D:\\data"));
        assert_eq!(fx.len(), 1);
        assert_eq!(p.history_back.last(), Some(&PathBuf::from("C:\\root")));
        // Esc (ClearSelection) はオーバーレイを閉じるだけ
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenPathEdit);
        update_pane(&mut p, PaneId::default(), false, PaneMsg::ClearSelection);
        assert!(p.overlay.is_none());
        assert_eq!(p.cur_path, PathBuf::from("D:\\data"));
        // 同一パスで確定 → 移動しない
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenPathEdit);
        let fx = update_pane(&mut p, PaneId::default(), false, PaneMsg::PathEditCommit);
        assert!(fx.is_empty());
    }

    #[test]
    fn path_edit_cancelled_by_mouse_and_click_applies() {
        // 行クリック: 編集を破棄しつつ 1 クリック目からそのまま選択が効く
        let mut p = pane_with(&[("a.txt", false), ("b.txt", false)]);
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenPathEdit);
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::PathEditInput("D:\\typed".into()),
        );
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::RowPressed {
                ix: 1,
                ctrl: false,
                shift: false,
            },
        );
        assert!(p.overlay.is_none());
        assert!(p.selected.contains(&1));
        // 入力途中の値は破棄される (typed へは移動しない)
        assert_ne!(p.cur_path, PathBuf::from("D:\\typed"));

        // ツリークリック (NavigateTo): 編集を破棄してそのまま移動する
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenPathEdit);
        let fx = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::NavigateTo(PathBuf::from("D:\\data")),
        );
        assert!(p.overlay.is_none());
        assert_eq!(p.cur_path, PathBuf::from("D:\\data"));
        assert!(fx.iter().any(|e| matches!(e, Effect::LoadDir { .. })));

        // 空白クリック: 編集を破棄して選択解除
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenPathEdit);
        update_pane(&mut p, PaneId::default(), false, PaneMsg::BlankPressed);
        assert!(p.overlay.is_none());
        assert!(p.selected.is_empty());
    }

    #[test]
    fn rename_modal_commit_emits_effect_and_pending_cursor() {
        let mut p = pane_with(&[("old.txt", false)]);
        p.click_row(0, false, false);
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenRename);
        assert!(matches!(p.overlay, Some(Overlay::Modal { .. })));
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::ModalInput("new.txt".into()),
        );
        let fx = update_pane(&mut p, PaneId::default(), false, PaneMsg::ModalCommit);
        assert_eq!(
            fx,
            vec![Effect::Rename {
                from: PathBuf::from("C:\\root\\old.txt"),
                to: PathBuf::from("C:\\root\\new.txt"),
            }]
        );
        assert_eq!(p.pending_cursor_name.as_deref(), Some("new.txt"));
        assert!(p.overlay.is_none());
        // 不正な名前 (区切り文字) は確定されずモーダルが残る
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenNewFolder);
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::ModalInput("a\\b".into()),
        );
        assert!(update_pane(&mut p, PaneId::default(), false, PaneMsg::ModalCommit).is_empty());
        assert!(p.overlay.is_some());
        update_pane(&mut p, PaneId::default(), false, PaneMsg::ModalCancel);

        // F2/F8 も F7 と同じ名前規則 — 使えない文字は拒否 + 理由通知
        // (「a:b」を許すとリネームは ADS、作成はドライブ相対の誤爆になる)
        p.status_msg = None;
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenRename);
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::ModalInput("a:b".into()),
        );
        assert!(update_pane(&mut p, PaneId::default(), false, PaneMsg::ModalCommit).is_empty());
        assert!(p.overlay.is_some(), "F2 の不正名でモーダルが閉じた");
        assert!(p.status_msg.is_some(), "F2 の不正名が通知されない");
        update_pane(&mut p, PaneId::default(), false, PaneMsg::ModalCancel);
        p.status_msg = None;
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenNewFile);
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::ModalInput("a*b".into()),
        );
        assert!(update_pane(&mut p, PaneId::default(), false, PaneMsg::ModalCommit).is_empty());
        assert!(p.overlay.is_some(), "F8 の不正名でモーダルが閉じた");
        assert!(p.status_msg.is_some(), "F8 の不正名が通知されない");
    }

    #[test]
    fn new_folder_multiline_creates_one_dir_per_line() {
        let mut p = pane_with(&[]);
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenNewFolder);
        // 空行・前後空白・重複行は畳まれ、残った行ごとに CreateDir
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::ModalInput("alpha\n\n  beta  \nalpha\n".into()),
        );
        let fx = update_pane(&mut p, PaneId::default(), false, PaneMsg::ModalCommit);
        assert_eq!(
            fx,
            vec![Effect::CreateDirs {
                paths: vec![
                    PathBuf::from("C:\\root\\alpha"),
                    PathBuf::from("C:\\root\\beta"),
                ],
            }]
        );
        // カーソルは先頭の名前へ
        assert_eq!(p.pending_cursor_name.as_deref(), Some("alpha"));
        assert!(p.overlay.is_none());

        // 単一行は従来どおり 1 件 (回帰)
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenNewFolder);
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::ModalInput("solo".into()),
        );
        let fx = update_pane(&mut p, PaneId::default(), false, PaneMsg::ModalCommit);
        assert_eq!(
            fx,
            vec![Effect::CreateDirs {
                paths: vec![PathBuf::from("C:\\root\\solo")],
            }]
        );

        // 空行だけ (実質未入力) は確定されずモーダルが残る
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenNewFolder);
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::ModalInput("  \n\n".into()),
        );
        assert!(update_pane(&mut p, PaneId::default(), false, PaneMsg::ModalCommit).is_empty());
        assert!(p.overlay.is_some());

        // 使えない文字を含む行が 1 つでもあれば全体を確定させず理由を通知
        for bad in ["ok\nng/name", "a:b", "a*b\nok", "tab\tname"] {
            p.status_msg = None;
            update_pane(
                &mut p,
                PaneId::default(),
                false,
                PaneMsg::ModalInput(bad.into()),
            );
            assert!(
                update_pane(&mut p, PaneId::default(), false, PaneMsg::ModalCommit).is_empty(),
                "{bad:?} が確定されてしまう"
            );
            assert!(p.overlay.is_some());
            assert!(p.status_msg.is_some(), "{bad:?} の拒否理由が通知されない");
        }

        // 孤立 \r もエディタ表示と同じく行区切りとして扱う (cosmic-text 準拠)
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::ModalInput("cr1\rcr2".into()),
        );
        let fx = update_pane(&mut p, PaneId::default(), false, PaneMsg::ModalCommit);
        assert_eq!(
            fx,
            vec![Effect::CreateDirs {
                paths: vec![
                    PathBuf::from("C:\\root\\cr1"),
                    PathBuf::from("C:\\root\\cr2"),
                ],
            }]
        );
    }

    #[test]
    fn new_folder_folds_case_insensitive_duplicates() {
        // NTFS は大文字小文字を区別しない — 大小違いだけの行は先勝ちで 1 つに
        // 畳む (2 行受理して 1 フォルダしかできない「黙った半分成功」を防ぐ)
        let mut p = pane_with(&[]);
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenNewFolder);
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::ModalInput("Docs\ndocs\nDOCS\nother".into()),
        );
        let fx = update_pane(&mut p, PaneId::default(), false, PaneMsg::ModalCommit);
        assert_eq!(
            fx,
            vec![Effect::CreateDirs {
                paths: vec![
                    PathBuf::from("C:\\root\\Docs"),
                    PathBuf::from("C:\\root\\other"),
                ],
            }]
        );
    }

    #[test]
    fn loaded_without_pending_match_consumes_pending_gracefully() {
        // 一括作成で先頭行の作成が失敗した場合など、reload に pending の名前が
        // 無くてもカーソルは誤った行に飛ばず、pending は消費されて次回の
        // reload に持ち越さない (Loaded は take で必ず消費する)
        let mut p = pane_with(&[]);
        update_pane(&mut p, PaneId::default(), false, PaneMsg::OpenNewFolder);
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::ModalInput("ghost\nreal".into()),
        );
        update_pane(&mut p, PaneId::default(), false, PaneMsg::ModalCommit);
        assert_eq!(p.pending_cursor_name.as_deref(), Some("ghost"));
        let generation = p.load_gen;
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Loaded {
                generation,
                entries: vec![entry("real", true)],
            },
        );
        assert_eq!(p.pending_cursor_name, None);
        assert_eq!(p.cursor, None);
    }

    #[test]
    fn paste_without_conflict_spawns_job_directly() {
        use crate::transfer::TransferOp;
        let mut p = pane_with(&[("existing.txt", false)]);
        let fx = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::PasteRead {
                paths: vec!["D:\\src\\new.txt".into()],
                op: "copy".into(),
            },
        );
        assert_eq!(
            fx,
            vec![Effect::SpawnJob {
                pane: PaneId::default(),
                op: TransferOp::Copy,
                items: vec![(
                    PathBuf::from("D:\\src\\new.txt"),
                    PathBuf::from("C:\\root\\new.txt")
                )],
            }]
        );
        assert!(p.overlay.is_none());
    }

    #[test]
    fn paste_with_conflict_opens_dialog_then_rename_both() {
        use crate::transfer::{ConflictChoice, TransferOp};
        let mut p = pane_with(&[("a.txt", false)]);
        let fx = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::PasteRead {
                paths: vec!["D:\\src\\a.txt".into()],
                op: "cut".into(),
            },
        );
        assert!(fx.is_empty());
        assert!(matches!(p.overlay, Some(Overlay::Conflict { .. })));
        let fx = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Conflict(ConflictChoice::RenameBoth),
        );
        assert_eq!(
            fx,
            vec![Effect::SpawnJob {
                pane: PaneId::default(),
                op: TransferOp::Move,
                items: vec![(
                    PathBuf::from("D:\\src\\a.txt"),
                    PathBuf::from("C:\\root\\a (2).txt")
                )],
            }]
        );
        assert!(p.overlay.is_none());
    }

    #[test]
    fn properties_menu_targets_row_or_current_folder() {
        // 行の上 → その行のプロパティ (最下部固定 — ADR 0007 追記)
        let mut p = pane_with(&[("a.txt", false)]);
        let open = |row| PaneMsg::OpenMenu {
            at: (0.0, 0.0),
            row,
            templates: vec![],
            commands: vec![],
            templates_dir: String::new(),
            can_paste: false,
        };
        update_pane(&mut p, PaneId::default(), false, open(Some(0)));
        let last = match &p.overlay {
            Some(Overlay::ContextMenu { items, .. }) => {
                assert_eq!(items.last().unwrap().label, "プロパティ");
                items.len() - 1
            }
            _ => panic!("メニューが開いていない"),
        };
        let fx = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::MenuClicked(vec![last]),
        );
        assert_eq!(
            fx,
            vec![Effect::ShowProperties {
                path: PathBuf::from("C:\\root\\a.txt"),
            }]
        );
        // 背景 → 現在フォルダのプロパティ (カーソルへフォールバックしない)
        p.click_row(0, false, false); // カーソルを乗せた状態でも
        update_pane(&mut p, PaneId::default(), false, open(None));
        let last = match &p.overlay {
            Some(Overlay::ContextMenu { items, .. }) => items.len() - 1,
            _ => panic!("メニューが開いていない"),
        };
        let fx = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::MenuClicked(vec![last]),
        );
        assert_eq!(
            fx,
            vec![Effect::ShowProperties {
                path: PathBuf::from("C:\\root"),
            }]
        );
    }

    #[test]
    fn virtual_paste_without_conflict_spawns_extract_job() {
        use crate::transfer::VirtualEntry;
        let mut p = pane_with(&[("existing.txt", false)]);
        let entries = vec![VirtualEntry {
            index: 0,
            rel_path: "new.txt".into(),
            is_dir: false,
            size: Some(10),
        }];
        let fx = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::PasteVirtualRead {
                entries: entries.clone(),
            },
        );
        assert_eq!(
            fx,
            vec![Effect::PasteVirtual {
                pane: PaneId::default(),
                dest: PathBuf::from("C:\\root"),
                entries,
            }]
        );
        assert!(p.overlay.is_none());
    }

    #[test]
    fn virtual_paste_with_conflict_opens_dialog_then_rename_both() {
        use crate::transfer::{ConflictChoice, VirtualEntry};
        let mut p = pane_with(&[("a.txt", false)]);
        let fx = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::PasteVirtualRead {
                entries: vec![VirtualEntry {
                    index: 0,
                    rel_path: "a.txt".into(),
                    is_dir: false,
                    size: None,
                }],
            },
        );
        assert!(fx.is_empty());
        assert!(matches!(p.overlay, Some(Overlay::VirtualConflict { .. })));
        let fx = update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Conflict(ConflictChoice::RenameBoth),
        );
        assert_eq!(
            fx,
            vec![Effect::PasteVirtual {
                pane: PaneId::default(),
                dest: PathBuf::from("C:\\root"),
                entries: vec![VirtualEntry {
                    index: 0,
                    rel_path: "a (2).txt".into(),
                    is_dir: false,
                    size: None,
                }],
            }]
        );
        assert!(p.overlay.is_none());
    }

    #[test]
    fn esc_cancels_running_job_before_clearing_selection() {
        use crate::domain_event::DomainEvent;
        let mut p = pane_with(&[("a.txt", false)]);
        p.click_row(0, false, false);
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Domain(DomainEvent::JobProgress {
                job_id: 9,
                kind: "copy".into(),
                done_files: 1,
                total_files: 5,
                done_bytes: 0,
                total_bytes: 0,
                current: "x".into(),
            }),
        );
        assert!(p.job.is_some());
        // 1 回目の Esc = ジョブキャンセル (選択は残る)
        let fx = update_pane(&mut p, PaneId::default(), false, PaneMsg::ClearSelection);
        assert_eq!(fx, vec![Effect::CancelJob { id: 9 }]);
        assert!(!p.selected.is_empty());
        // JobDone (キャンセル) で job が消え、次の Esc は選択解除
        update_pane(
            &mut p,
            PaneId::default(),
            false,
            PaneMsg::Domain(DomainEvent::JobDone {
                job_id: 9,
                kind: "copy".into(),
                ok: false,
                canceled: true,
                error: None,
                done_files: 1,
                total_files: 5,
            }),
        );
        assert!(p.job.is_none());
        assert_eq!(p.status_msg.as_deref(), Some("キャンセルしました"));
        assert!(update_pane(&mut p, PaneId::default(), false, PaneMsg::ClearSelection).is_empty());
        assert!(p.selected.is_empty());
    }

    #[test]
    fn copy_cut_delete_use_selection() {
        let mut p = pane_with(&[("a.txt", false), ("b.txt", false)]);
        p.click_row(0, false, false);
        p.click_row(1, true, false);
        let fx = update_pane(&mut p, PaneId::default(), false, PaneMsg::RequestCopy);
        assert_eq!(
            fx,
            vec![Effect::ClipboardWrite {
                paths: vec![
                    PathBuf::from("C:\\root\\a.txt"),
                    PathBuf::from("C:\\root\\b.txt")
                ],
                op: "copy".into(),
            }]
        );
        let fx = update_pane(&mut p, PaneId::default(), false, PaneMsg::RequestDelete);
        assert_eq!(
            fx,
            vec![Effect::DeleteToTrash {
                paths: vec![
                    PathBuf::from("C:\\root\\a.txt"),
                    PathBuf::from("C:\\root\\b.txt")
                ],
            }]
        );
        // 選択なしなら何も出ない
        p.clear_selection();
        assert!(update_pane(&mut p, PaneId::default(), false, PaneMsg::RequestCut).is_empty());
        assert!(update_pane(&mut p, PaneId::default(), false, PaneMsg::RequestDelete).is_empty());
    }

    #[test]
    fn scroll_clamps_to_content() {
        let mut p = pane_with(&[("a.txt", false), ("b.txt", false)]);
        p.viewport_h = 24.0; // 1 行分
        update_pane(&mut p, PaneId::default(), false, PaneMsg::Scrolled(9999.0));
        assert_eq!(p.scroll_offset, 24.0); // 2 行 - 1 行分
        update_pane(&mut p, PaneId::default(), false, PaneMsg::Scrolled(-5.0));
        assert_eq!(p.scroll_offset, 0.0);
    }
}
