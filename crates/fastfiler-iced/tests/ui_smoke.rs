//! UI 操作レベルのスモークテスト (iced_test のヘッドレス Simulator)。
//!
//! 「ボタンが押せて、正しいメッセージが飛び、画面が遷移する」ことを
//! 実描画なしで検証する。ファイル操作そのものは対象外 (core の単体テスト側)。
//!
//! 制約: 直描きのカスタム widget (一覧・ツリー・タブ) の文字はセレクタで
//! 探せないため、標準 widget (ボタン等) はテキスト、カスタム widget は
//! 座標クリックで叩く。

use fastfiler_core::update_app::{AppMsg, DragMsg};
use fastfiler_iced::app::{self, App, Msg};
use fastfiler_iced::settings_view::SettingsMsg;
use iced_test::simulator;

/// テスト用に単一ペイン (temp フォルダ) で起動する。
/// FASTFILER_OPEN はセッションの読み込みも保存も抑止する (検証モード)。
fn boot_app() -> App {
    // SAFETY: テストは 1 プロセスで直列に env を読む前に設定する
    unsafe {
        std::env::set_var("FASTFILER_OPEN", std::env::temp_dir());
    }
    let (app, _task) = app::boot();
    app
}

/// キー押下イベントの組み立て (iced の KeyPressed は 7 フィールド —
/// 使う 2 つ以外は Unidentified/Standard/None の定型)。
fn key_pressed(key: iced::keyboard::Key, modifiers: iced::keyboard::Modifiers) -> iced::Event {
    iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: iced::keyboard::Location::Standard,
        repeat: false,
        modifiers,
        text: None,
    })
}

/// view からボタンをクリックし、発行されたメッセージを update へ流す。
fn click_and_apply(app: &mut App, label: &str) -> usize {
    let mut ui = simulator(app::view(app));
    ui.click(label)
        .unwrap_or_else(|e| panic!("「{label}」がクリックできない: {e:?}"));
    let msgs: Vec<Msg> = ui.into_messages().collect();
    let n = msgs.len();
    for m in msgs {
        let _ = app::update(app, m);
    }
    n
}

#[test]
fn ui_smoke_buttons_and_screens() {
    let mut app = boot_app();

    // 1. 設定ボタン → 設定画面が開く
    assert!(
        click_and_apply(&mut app, "設定") > 0,
        "設定ボタンが反応しない"
    );
    {
        // 設定画面のボタンが存在しクリックできる
        let mut ui = simulator(app::view(&app));
        ui.find("テーマを再読込").expect("設定画面が開いていない");
        ui.click("× 閉じる").expect("閉じるボタンが押せない");
        let msgs: Vec<Msg> = ui.into_messages().collect();
        assert!(
            msgs.iter()
                .any(|m| matches!(m, Msg::Settings(SettingsMsg::Close))),
            "閉じるが Close を発行しない"
        );
        for m in msgs {
            let _ = app::update(&mut app, m);
        }
    }
    // 閉じた後は通常画面 (設定ボタンが再び見える)
    {
        let mut ui = simulator(app::view(&app));
        ui.find("設定").expect("設定画面から戻れていない");
    }

    // 2. タブ追加 (＋) → パスバー等の通常 UI が維持される
    assert!(click_and_apply(&mut app, "＋") > 0, "＋ボタンが反応しない");
    {
        let mut ui = simulator(app::view(&app));
        ui.find("＋").expect("タブ追加後に画面が壊れた");
    }

    // 3. ツリートグル → 押しても view が構築できる (パニックしない)
    assert!(
        click_and_apply(&mut app, "ツリー") > 0,
        "ツリーボタンが反応しない"
    );
    let _ = simulator(app::view(&app));

    // 4. ペインヘッダの ↑ (親へ) ボタン
    assert!(click_and_apply(&mut app, "↑") > 0, "↑ボタンが反応しない");
    let _ = simulator(app::view(&app));
}

#[test]
fn ui_smoke_settings_controls() {
    let mut app = boot_app();
    click_and_apply(&mut app, "設定");

    let mut ui = simulator(app::view(&app));
    // 設定画面の主要ボタン一式が存在する (欠落の回帰防止 —
    // 「設定ボタンが無い」事故の再発防止と同型のチェック)
    for label in [
        "テーマを再読込",
        "ホットキー設定を開く",
        "再読み込み",
        "テンプレートフォルダを開く",
        "ユーザーコマンドのフォルダを開く",
        "× 閉じる",
    ] {
        ui.find(label)
            .unwrap_or_else(|e| panic!("設定画面に「{label}」が無い: {e:?}"));
    }
}

/// 外部右ボタンドロップ → チューザーメニュー → 「ここにコピー」→ 実ファイルコピー
/// までの通し検証 (ユーザー報告「メニューは出るが実際はコピーされない」の再現)。
#[test]
fn external_right_drop_copies_real_file() {
    // 送り元ファイルと送り先フォルダを用意
    let base = std::env::temp_dir().join(format!("ff_ui_dnd_{}", std::process::id()));
    let src_dir = base.join("src");
    let dest_dir = base.join("dest");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&dest_dir).unwrap();
    let src_file = src_dir.join("dropped.txt");
    std::fs::write(&src_file, b"payload").unwrap();

    // SAFETY: テストプロセス内で最初に設定
    unsafe {
        std::env::set_var("FASTFILER_OPEN", &dest_dir);
    }
    let (mut app, _task) = app::boot();

    // 外部右ドロップ相当を投入 → DropMenu が開く
    let pane = {
        // focused_pane は公開されていないため、External はフォーカスペイン宛に
        // 届いた前提で view から確かめる。App 経由で pane id を得る手段として
        // core の AppMsg::Drag は pane を要求する — boot 直後は単一ペイン。
        // fastfiler_core::AppModel へ直接アクセスできないので、
        // FooterRightClicked と同じ経路は使わず、External をフォーカス宛にする
        // ための専用ヘルパを app に置かない代わりに、単一ペイン前提で
        // Msg::List の BoundsChanged 等は不要 — DragMsg::External は
        // 存在する PaneId が必要。ここでは app::focused_pane_for_test() を使う。
        app::focused_pane_for_test(&app)
    };
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Drag(DragMsg::External {
            pane,
            paths: vec![src_file.clone()],
            effect: 1,
            right_button: true,
            at: (20.0, 20.0),
            commands: vec![],
        })),
    );

    // メニュー項目「ここにコピー」(1 項目目) を座標クリック
    // パネルは at=(20,20)、項目高 26px、パディング 4px → 項目 0 の中心 ≈ (130, 37)
    let mut ui = simulator(app::view(&app));
    ui.point_at(iced::Point::new(130.0, 37.0));
    let _ = ui.simulate(iced_test::simulator::click());
    let msgs: Vec<Msg> = ui.into_messages().collect();
    assert!(
        !msgs.is_empty(),
        "メニュークリックがメッセージを発行しない (widget にイベントが届いていない)"
    );
    for m in msgs {
        let _ = app::update(&mut app, m);
    }

    // ジョブスレッドがコピーするのを待つ (最大 5 秒)
    let expected = dest_dir.join("dropped.txt");
    let mut ok = false;
    for _ in 0..50 {
        if expected.exists() {
            ok = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let _ = std::fs::remove_dir_all(&base);
    assert!(ok, "「ここにコピー」を選んでもファイルがコピーされない");
}

/// 回帰 (ユーザー操作想定): タブを切り替えた後にエクスプローラからドロップすると
/// 「別タブの古いペイン」へ解決されるバグ (実機ログで発見)。
/// 実際のマウスイベントを simulator で流し、widget が発行する矩形通知だけで
/// ヒットテスト表が正しく更新されることを検証する (手動注入なし)。
#[test]
fn ole_hit_test_follows_active_tab() {
    unsafe {
        std::env::set_var("FASTFILER_OPEN", std::env::temp_dir());
    }
    let (mut app, _task) = app::boot();
    let first_pane = app::focused_pane_for_test(&app);

    // 起動直後、マウスを一度も動かさずに即ドロップされても解決できること
    // (OLE ドラッグ中は通常のマウスイベントが届かないため、widget 通知に
    //  依存すると表が空になる — 実機で「再起動直後の D&D が全拒否」を確認)
    assert_eq!(
        app::ole_hit_for_test(&app, 600.0, 300.0),
        Some(first_pane),
        "起動直後 (マウス操作なし) のドロップが拒否される"
    );

    // タブ追加ボタンをクリック → 直後 (マウス移動なし) のドロップは新タブのペインへ
    {
        let mut ui = simulator(app::view(&app));
        ui.click("＋").expect("タブ追加");
        let msgs: Vec<Msg> = ui.into_messages().collect();
        for m in msgs {
            let _ = app::update(&mut app, m);
        }
    }
    let second_pane = app::focused_pane_for_test(&app);
    assert_ne!(first_pane, second_pane);
    assert_eq!(
        app::ole_hit_for_test(&app, 600.0, 300.0),
        Some(second_pane),
        "タブ切替直後のドロップが古いタブのペインへ解決される"
    );

    // ペイン分割直後: 分割した両ペインがそれぞれの領域に解決されること
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Tab(
            fastfiler_core::update_app::TabMsg::SplitFocused(fastfiler_core::bsp::SplitDir::Row),
        )),
    );
    let right_pane = app::focused_pane_for_test(&app);
    assert_ne!(second_pane, right_pane);
    // 左半分 → 元ペイン / 右半分 → 新ペイン (960 幅、ペイン領域はタブ+ツリーの右)
    let left_hit = app::ole_hit_for_test(&app, 500.0, 300.0);
    let right_hit = app::ole_hit_for_test(&app, 900.0, 300.0);
    assert_eq!(left_hit, Some(second_pane), "分割左ペインの解決が誤り");
    assert_eq!(right_hit, Some(right_pane), "分割右ペインの解決が誤り");
}

/// 回帰 (ユーザー操作想定): 分割ペイン間の右ボタンドラッグ&ドロップ。
/// 右ドラッグの Released が「発生元ペインの押下状態」でしか判定されず、
/// 分割相手のペインで離すと無反応になるバグ (実機報告) の再現と検証。
#[test]
fn internal_right_drag_across_panes_opens_chooser() {
    // 行が必要なので実ファイルを持つフォルダを開く
    let base = std::env::temp_dir().join(format!("ff_rdrag_{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(base.join("a.txt"), b"x").unwrap();
    unsafe {
        std::env::set_var("FASTFILER_OPEN", &base);
    }
    let (mut app, _task) = app::boot();
    // 読み込み完了を待つ代わりに直接 Loaded を作れないため、実読み込みを促す:
    // boot の LoadDir はタスク実行されないので、エントリを同期投入する
    let pane = app::focused_pane_for_test(&app);
    let entries = vec![fastfiler_core::Entry::new(
        "a.txt".into(),
        false,
        1,
        0,
        Some("txt".into()),
        false,
    )];
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Pane(
            pane,
            fastfiler_core::PaneMsg::Loaded {
                generation: 1,
                entries,
            },
        )),
    );
    // 左右分割 → フォーカスは新 (右) ペイン。ドラッグ元は左ペイン
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Tab(
            fastfiler_core::update_app::TabMsg::SplitFocused(fastfiler_core::bsp::SplitDir::Row),
        )),
    );

    // 段階 0: 行 0 の実座標をプローブ (左クリックで RowPressed が出る y を探す —
    // ヘッダ高はフォント等で変わるため決め打ちしない)
    let row_y = (40..160)
        .step_by(6)
        .find(|&y| {
            let mut ui = simulator(app::view(&app));
            let p = iced::Point::new(500.0, y as f32);
            ui.point_at(p);
            let _ = ui.simulate(vec![
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position: p }),
                iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                    iced::mouse::Button::Left,
                )),
            ]);
            ui.into_messages().any(|m| {
                matches!(
                    m,
                    Msg::List(
                        _,
                        fastfiler_iced::widgets::file_list::ListEvent::RowPressed { .. }
                    )
                )
            })
        })
        .expect("行 0 の座標が見つからない (entries が表示されていない?)");

    // 段階 1: 左ペインの行を右ボタンで掴んで 12px 動かす → DragStarted
    let row_pos = iced::Point::new(500.0, row_y as f32);
    {
        let mut ui = simulator(app::view(&app));
        ui.point_at(row_pos);
        let _ = ui.simulate(vec![
            iced::Event::Mouse(iced::mouse::Event::CursorMoved { position: row_pos }),
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
                iced::mouse::Button::Right,
            )),
        ]);
        ui.point_at(iced::Point::new(row_pos.x + 12.0, row_pos.y));
        let _ = ui.simulate(std::iter::once(iced::Event::Mouse(
            iced::mouse::Event::CursorMoved {
                position: iced::Point::new(row_pos.x + 12.0, row_pos.y),
            },
        )));
        let msgs: Vec<Msg> = ui.into_messages().collect();
        let msgs_dbg: Vec<String> = msgs.iter().map(|m| format!("{m:?}")).collect();
        let started = msgs.iter().any(|m| {
            matches!(
                m,
                Msg::List(
                    _,
                    fastfiler_iced::widgets::file_list::ListEvent::DragStarted { .. }
                )
            )
        });
        for m in msgs {
            let _ = app::update(&mut app, m);
        }
        assert!(
            started,
            "右ドラッグの DragStarted が発行されない: {msgs_dbg:?}"
        );
    }

    // 段階 2: 分割相手 (右ペイン) の上で右ボタンを離す → チューザーが開くべき
    let drop_pos = iced::Point::new(880.0, 300.0);
    {
        let mut ui = simulator(app::view(&app));
        ui.point_at(drop_pos);
        let _ = ui.simulate(vec![
            iced::Event::Mouse(iced::mouse::Event::CursorMoved { position: drop_pos }),
            iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                iced::mouse::Button::Right,
            )),
        ]);
        let msgs: Vec<Msg> = ui.into_messages().collect();
        assert!(
            !msgs.is_empty(),
            "分割相手ペインでの右リリースが無反応 (実機バグの再現)"
        );
        for m in msgs {
            let _ = app::update(&mut app, m);
        }
    }
    let _ = std::fs::remove_dir_all(&base);
    assert!(
        app::drop_menu_open_for_test(&app),
        "右ドラッグ&ドロップでチューザーメニューが開かない"
    );
}

/// 3 行 (a/b/c.txt) を同期投入して全選択した状態のアプリを作る。
/// 行 0 の実座標もプローブして返す (ヘッダ高は決め打ちしない)。
fn boot_with_selected_rows() -> (App, iced::Point) {
    let mut app = boot_app();
    let pane = app::focused_pane_for_test(&app);
    let entries: Vec<_> = ["a.txt", "b.txt", "c.txt"]
        .iter()
        .map(|n| fastfiler_core::Entry::new((*n).into(), false, 1, 0, Some("txt".into()), false))
        .collect();
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Pane(
            pane,
            fastfiler_core::PaneMsg::Loaded {
                generation: 1,
                entries,
            },
        )),
    );
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Pane(pane, fastfiler_core::PaneMsg::SelectAll)),
    );
    assert_eq!(app::selection_for_test(&app), vec![0, 1, 2]);
    // プローブは view の複製に対して行い、メッセージは捨てる (状態は変えない)
    let row_y = (40..160)
        .step_by(6)
        .find(|&y| {
            let mut ui = simulator(app::view(&app));
            let p = iced::Point::new(500.0, y as f32);
            ui.point_at(p);
            let _ = ui.simulate(vec![
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position: p }),
                iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                    iced::mouse::Button::Left,
                )),
            ]);
            ui.into_messages().any(|m| {
                matches!(
                    m,
                    Msg::List(
                        _,
                        fastfiler_iced::widgets::file_list::ListEvent::RowPressed { ix: 0, .. }
                    )
                )
            })
        })
        .expect("行 0 の座標が見つからない (entries が表示されていない?)");
    (app, iced::Point::new(500.0, row_y as f32))
}

/// 実機報告「複数選択後の左ドラッグ移動ができない (押下で 1 個の選択に潰れる)」。
/// 選択済み行の修飾なし押下は選択を維持し、ドラッグは選択全体を運ぶ。
#[test]
fn left_drag_carries_whole_selection() {
    let (mut app, row_pos) = boot_with_selected_rows();
    let mut ui = simulator(app::view(&app));
    ui.point_at(row_pos);
    let _ = ui.simulate(vec![
        iced::Event::Mouse(iced::mouse::Event::CursorMoved { position: row_pos }),
        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
    ]);
    let moved = iced::Point::new(row_pos.x + 12.0, row_pos.y);
    ui.point_at(moved);
    let _ = ui.simulate(std::iter::once(iced::Event::Mouse(
        iced::mouse::Event::CursorMoved { position: moved },
    )));
    let msgs: Vec<Msg> = ui.into_messages().collect();
    let started = msgs.iter().any(|m| {
        matches!(
            m,
            Msg::List(
                _,
                fastfiler_iced::widgets::file_list::ListEvent::DragStarted { .. }
            )
        )
    });
    for m in msgs {
        let _ = app::update(&mut app, m);
    }
    assert!(started, "左ドラッグの DragStarted が発行されない");
    assert_eq!(
        app::selection_for_test(&app),
        vec![0, 1, 2],
        "押下の瞬間に選択が潰れている (実機バグの再現)"
    );
    assert_eq!(
        app::drag_paths_for_test(&app),
        Some(3),
        "ドラッグが選択全体を運んでいない"
    );
}

/// 実機報告「パスバーをクリックした直後、ファイル/フォルダをクリックしても
/// 選択できない (Enter で編集を閉じるまで一覧が無反応)」。
/// パスバー編集中でも一覧クリックは 1 クリック目からそのまま効くこと
/// (編集はその時点で破棄される — エクスプローラ準拠)。
#[test]
fn list_click_works_while_path_edit_is_open() {
    let (mut app, row_pos) = boot_with_selected_rows();

    // パスバー (ボタン表示、ラベル=現在パス) をクリック → 編集モードへ
    let path_label = app::cur_path_for_test(&app);
    assert!(
        click_and_apply(&mut app, &path_label) > 0,
        "パスバーが反応しない"
    );
    assert!(
        app::path_edit_open_for_test(&app),
        "パスバークリックで編集が開かない"
    );

    // 編集中に行 0 を普通にクリック (press + release)
    let mut ui = simulator(app::view(&app));
    ui.point_at(row_pos);
    let _ = ui.simulate(vec![
        iced::Event::Mouse(iced::mouse::Event::CursorMoved { position: row_pos }),
        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
        iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
            iced::mouse::Button::Left,
        )),
    ]);
    let msgs: Vec<Msg> = ui.into_messages().collect();
    assert!(!msgs.is_empty(), "編集中の一覧クリックが無反応");
    for m in msgs {
        let _ = app::update(&mut app, m);
    }

    // 編集は破棄され、クリックは 1 回目からそのまま効いている
    assert!(
        !app::path_edit_open_for_test(&app),
        "一覧クリックでパスバー編集が閉じない"
    );
    assert_eq!(
        app::selection_for_test(&app),
        vec![0],
        "編集中の一覧クリックが選択に反映されない (実機バグの再現)"
    );
}

/// 上と対: ドラッグに至らない単純クリックは、離した時点で単一選択へ確定する。
#[test]
fn plain_click_on_selection_collapses_on_release() {
    let (mut app, row_pos) = boot_with_selected_rows();
    let mut ui = simulator(app::view(&app));
    ui.point_at(row_pos);
    let _ = ui.simulate(vec![
        iced::Event::Mouse(iced::mouse::Event::CursorMoved { position: row_pos }),
        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
        iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
            iced::mouse::Button::Left,
        )),
    ]);
    let msgs: Vec<Msg> = ui.into_messages().collect();
    let released = msgs.iter().any(|m| {
        matches!(
            m,
            Msg::List(
                _,
                fastfiler_iced::widgets::file_list::ListEvent::RowReleased { ix: 0 }
            )
        )
    });
    for m in msgs {
        let _ = app::update(&mut app, m);
    }
    assert!(released, "非ドラッグの左リリースで RowReleased が出ない");
    assert_eq!(
        app::selection_for_test(&app),
        vec![0],
        "クリック確定で単一選択にならない"
    );
    assert_eq!(app::drag_paths_for_test(&app), None);
}

/// ツリーのドライブ行クリック → フォーカスペインのパスが「二重区切りなし」で
/// 遷移する統合テスト (実機報告 `D:\\AI\…` の回帰防止)。
/// ドライブは domain の生形式 ("Q:\" — 末尾 \ 付き) のまま注入する。
#[test]
fn tree_drive_click_navigates_with_clean_path() {
    let mut app = boot_app();
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Tree(fastfiler_core::tree::TreeMsg::DrivesLoaded(
            vec![("Q:\\".into(), String::new())],
        ))),
    );
    // ドライブ行 = ツリー先頭行。中心座標はレイアウト式のフックから得る
    // (矢印域より右なので Open が出る)
    let (x, y) = app::tree_row0_center_for_test(&app);
    let pos = iced::Point::new(x, y);
    let mut ui = simulator(app::view(&app));
    ui.point_at(pos);
    let _ = ui.simulate(vec![
        iced::Event::Mouse(iced::mouse::Event::CursorMoved { position: pos }),
        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
        iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
            iced::mouse::Button::Left,
        )),
    ]);
    let msgs: Vec<Msg> = ui.into_messages().collect();
    let navigated = msgs.iter().any(|m| {
        matches!(
            m,
            Msg::Core(AppMsg::Focused(fastfiler_core::PaneMsg::NavigateTo(_)))
        )
    });
    for m in msgs {
        let _ = app::update(&mut app, m);
    }
    assert!(navigated, "ドライブ行クリックで NavigateTo が出ない");
    // "Q:\\" (二重) や "Q:" (ルート欠落) ではなく "Q:\" ちょうど
    assert_eq!(app::cur_path_for_test(&app), "Q:\\");
}

/// F7 の新規フォルダモーダルは複数行エディタ (1 行 1 フォルダの一括作成)。
/// UI 経路の検証: 開く → 貼り付けが core に同期 → OK で閉じる / Enter は改行の
/// まま閉じない / Esc はエディタにフォーカスがあっても 1 回で閉じる。
/// 行分割と CreateDir 発行そのものは core の単体テスト側。
#[test]
fn new_folder_modal_is_multiline_editor() {
    use iced::widget::text_editor::{Action, Edit};
    let hint = app::MULTILINE_HINT;
    let open = |app: &mut App| {
        let _ = app::update(
            app,
            Msg::Core(AppMsg::Focused(fastfiler_core::PaneMsg::OpenNewFolder)),
        );
    };
    let modal_open = |app: &App| simulator(app::view(app)).find(hint).is_ok();
    // モーダル中央のエディタへのクリック + 任意キーの流し込み (メッセージを返す)
    let click_editor_then_key = |app: &App, key: iced::keyboard::key::Named| -> Vec<Msg> {
        let pos = iced::Point::new(480.0, 300.0);
        let mut ui = simulator(app::view(app));
        ui.point_at(pos);
        let _ = ui.simulate(vec![
            iced::Event::Mouse(iced::mouse::Event::CursorMoved { position: pos }),
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
            iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                iced::mouse::Button::Left,
            )),
            key_pressed(
                iced::keyboard::Key::Named(key),
                iced::keyboard::Modifiers::default(),
            ),
        ]);
        ui.into_messages().collect()
    };

    let mut app = boot_app();
    open(&mut app);
    assert!(modal_open(&app), "F7 相当で複数行エディタが開かない");

    // 複数行の貼り付け → OK で閉じる (確定成功 = 行分割が通った)
    let _ = app::update(
        &mut app,
        Msg::ModalEditor(Action::Edit(Edit::Paste(std::sync::Arc::new(
            "d1\nd2".into(),
        )))),
    );
    assert!(click_and_apply(&mut app, "OK") > 0, "OK が押せない");
    assert!(!modal_open(&app), "複数行の確定でモーダルが閉じない");

    // 不正な行 (区切り文字) が混じると OK でも閉じない (core の検証が効く)
    open(&mut app);
    let _ = app::update(
        &mut app,
        Msg::ModalEditor(Action::Edit(Edit::Paste(std::sync::Arc::new(
            "ok\nng/name".into(),
        )))),
    );
    click_and_apply(&mut app, "OK");
    assert!(modal_open(&app), "不正な行を含む確定が素通りしている");
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Focused(fastfiler_core::PaneMsg::ModalCancel)),
    );

    // Enter は改行 (メモ帳のノリ) — モーダルは閉じない
    open(&mut app);
    for m in click_editor_then_key(&app, iced::keyboard::key::Named::Enter) {
        let _ = app::update(&mut app, m);
    }
    assert!(
        modal_open(&app),
        "Enter でモーダルが閉じてしまう (改行のはず)"
    );

    // Esc はエディタにフォーカスがあっても 1 回で閉じる (カスタム key_binding)
    let msgs = click_editor_then_key(&app, iced::keyboard::key::Named::Escape);
    assert!(
        msgs.iter().any(|m| matches!(
            m,
            Msg::Core(AppMsg::Focused(fastfiler_core::PaneMsg::ModalCancel))
        )),
        "エディタ内の Esc が ModalCancel を発行しない"
    );
    for m in msgs {
        let _ = app::update(&mut app, m);
    }
    assert!(!modal_open(&app), "Esc でモーダルが閉じない");
}

/// タブ切替のように「モーダルを破棄せずフォーカスだけ動かす」経路では、
/// エディタの入力は破棄前に core へ書き戻され (sync_modal_editor のフラッシュ)、
/// 戻ってきたときに復元される — 編集中は毎キー同期しない設計の安全網。
#[test]
fn multiline_editor_input_survives_tab_switch() {
    use iced::widget::text_editor::{Action, Edit};
    let hint = app::MULTILINE_HINT;
    let mut app = boot_app();
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Focused(fastfiler_core::PaneMsg::OpenNewFolder)),
    );
    let _ = app::update(
        &mut app,
        Msg::ModalEditor(Action::Edit(Edit::Paste(std::sync::Arc::new(
            "keep1\nkeep2".into(),
        )))),
    );
    // タブ追加 → フォーカスは新タブへ (モーダルは元ペインに生存、Content は破棄)
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Tab(fastfiler_core::update_app::TabMsg::Add)),
    );
    assert!(
        simulator(app::view(&app)).find(hint).is_err(),
        "新タブに元ペインのモーダルが見えている"
    );
    // 元タブへ戻る → モーダル再表示、書き戻した全文から Content 復元
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Tab(fastfiler_core::update_app::TabMsg::Select(0))),
    );
    assert!(
        simulator(app::view(&app)).find(hint).is_ok(),
        "元タブに戻ってもモーダルが再表示されない"
    );
    // 確定が通る = 入力 (keep1/keep2) が失われていない (空なら開いたまま)
    assert!(click_and_apply(&mut app, "OK") > 0, "OK が押せない");
    assert!(
        simulator(app::view(&app)).find(hint).is_err(),
        "確定でモーダルが閉じない (タブ切替で入力が失われた?)"
    );
}

/// フォーカスだけ動かす経路のもう 1 系統 — OLE 右ドロップ (DropMenu が開き
/// focused が直接差し替わる) でも、エディタ入力はフラッシュ → 復元される。
#[test]
fn multiline_editor_input_survives_ole_focus_move() {
    use iced::widget::text_editor::{Action, Edit};
    let hint = app::MULTILINE_HINT;
    let mut app = boot_app();
    let first = app::focused_pane_for_test(&app);
    // 左右分割 → フォーカスは新 (右) ペイン。そこで F7 相当 + 貼り付け
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Tab(
            fastfiler_core::update_app::TabMsg::SplitFocused(fastfiler_core::bsp::SplitDir::Row),
        )),
    );
    let second = app::focused_pane_for_test(&app);
    assert_ne!(first, second);
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Focused(fastfiler_core::PaneMsg::OpenNewFolder)),
    );
    let _ = app::update(
        &mut app,
        Msg::ModalEditor(Action::Edit(Edit::Paste(std::sync::Arc::new(
            "keep1\nkeep2".into(),
        )))),
    );
    // 外部右ドロップが左ペインへ → DropMenu が開き、フォーカスだけ左へ移る
    // (クリック経路と違いモーダルは破棄されない)。ドロップ元は表示中フォルダの
    // 外にする — 同一フォルダからのドロップは無視される (update_app の仕様)
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Drag(DragMsg::External {
            pane: first,
            paths: vec![std::env::temp_dir().join("elsewhere").join("dropped.txt")],
            effect: 1,
            right_button: true,
            at: (20.0, 20.0),
            commands: vec![],
        })),
    );
    assert!(
        simulator(app::view(&app)).find(hint).is_err(),
        "フォーカス移動後も右ペインのモーダルが見えている"
    );
    // メニューを閉じ、右ペインをクリックで戻る → モーダル再表示 + 入力復元
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Pane(first, fastfiler_core::PaneMsg::MenuClose)),
    );
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Pane(second, fastfiler_core::PaneMsg::BlankPressed)),
    );
    assert!(
        simulator(app::view(&app)).find(hint).is_ok(),
        "右ペインに戻ってもモーダルが再表示されない"
    );
    // 確定が通る = 入力 (keep1/keep2) が失われていない (空なら開いたまま)
    assert!(click_and_apply(&mut app, "OK") > 0, "OK が押せない");
    assert!(
        simulator(app::view(&app)).find(hint).is_err(),
        "確定でモーダルが閉じない (OLE 経由で入力が失われた?)"
    );
}

/// 操作完了 (OpDone) の明示 reload が、係留中の watcher デバウンス tick を
/// 吸収する通し (iced 層) — listing が二重に走らない。
#[test]
fn opdone_reload_absorbs_watcher_debounce() {
    use fastfiler_core::domain_event::DomainEvent;
    use fastfiler_iced::effects::OpOutcome;
    let mut app = boot_app();
    let pane = app::focused_pane_for_test(&app);
    let cur = app::cur_path_for_test(&app);
    // watcher 変化 → デバウンス係留 (seq 1)
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Pane(
            pane,
            fastfiler_core::PaneMsg::Domain(DomainEvent::FsChange { path: cur }),
        )),
    );
    let gen_before = app::load_gen_for_test(&app);
    // 操作完了 → 明示 reload が 1 回走る
    let _ = app::update(
        &mut app,
        Msg::OpDone(OpOutcome::Done {
            record: None,
            message: None,
        }),
    );
    let gen_after = app::load_gen_for_test(&app);
    assert_eq!(gen_after, gen_before + 1, "OpDone の明示 reload が走らない");
    // 係留していた tick (seq 1) は吸収済み — 二重 reload しない
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Pane(pane, fastfiler_core::PaneMsg::ReloadTick(1))),
    );
    assert_eq!(
        app::load_gen_for_test(&app),
        gen_after,
        "明示 reload 後の watcher tick が二重 reload している"
    );
}

/// 検索ショートカットの実証 (実機報告「Ctrl+F で検索ボックスが出ない」の切り分け)。
/// hotkeys.json のカスタム (search: ctrl+k) が生きている環境では Ctrl+F は
/// 割り当てが無いのが正しい — 既定とカスタムの両方をヘッドレスで検証する。
#[test]
fn search_shortcut_opens_search_bar() {
    // APPDATA をテスト用に隔離 (実ユーザーの hotkeys.json に依存しない)
    let base = std::env::temp_dir().join(format!("ff_hk_{}", std::process::id()));
    std::fs::create_dir_all(base.join("FastFiler")).unwrap();
    unsafe {
        std::env::set_var("APPDATA", &base);
        std::env::set_var("FASTFILER_OPEN", std::env::temp_dir());
    }

    let press = |app: &mut App, ch: &str| {
        let iced::Event::Keyboard(ev) = key_pressed(
            iced::keyboard::Key::Character(ch.into()),
            iced::keyboard::Modifiers::CTRL,
        ) else {
            unreachable!()
        };
        let _ = app::update(app, Msg::Key(ev));
    };
    let search_bar_visible = |app: &App| {
        let mut ui = simulator(app::view(app));
        ui.find("検索 (Everything / 内蔵)…").is_ok()
    };

    // 既定 (ファイルなし → 自動生成): Ctrl+F で検索バーが開く
    fastfiler_iced::hotkeys::reload();
    let (mut app, _task) = app::boot();
    assert!(!search_bar_visible(&app), "初期状態で検索バーが出ている");
    press(&mut app, "f");
    assert!(
        search_bar_visible(&app),
        "既定の Ctrl+F で検索バーが開かない"
    );

    // カスタム (search: ctrl+k): Ctrl+K で開き、Ctrl+F では開かない
    std::fs::write(
        base.join("FastFiler").join("hotkeys.json"),
        r#"{ "search": "ctrl+k" }"#,
    )
    .unwrap();
    fastfiler_iced::hotkeys::reload();
    let (mut app2, _task) = app::boot();
    press(&mut app2, "f");
    assert!(
        !search_bar_visible(&app2),
        "カスタム設定下で Ctrl+F が効いてしまう"
    );
    press(&mut app2, "k");
    assert!(
        search_bar_visible(&app2),
        "カスタムの Ctrl+K で検索バーが開かない"
    );
    let _ = std::fs::remove_dir_all(&base);
}
