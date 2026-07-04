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

/// 回帰: 非表示タブの古いペイン矩形が OLE ドロップのヒットテストに残り、
/// 「別タブのペインへドロップが解決される」バグ (ユーザー実機ログで発見)。
#[test]
fn ole_hit_test_ignores_hidden_tabs() {
    unsafe {
        std::env::set_var("FASTFILER_OPEN", std::env::temp_dir());
    }
    let (mut app, _task) = app::boot();
    let first_pane = app::focused_pane_for_test(&app);

    // タブ 1 のペインが (0,0)-(800,600) を占めていたと通知
    let _ = app::update(
        &mut app,
        Msg::List(
            first_pane,
            fastfiler_iced::widgets::file_list::ListEvent::BoundsChanged {
                x: 0.0,
                y: 0.0,
                w: 800.0,
                h: 600.0,
            },
        ),
    );
    // タブを追加 (アクティブが変わり、新ペインが同じ領域を占める)
    let _ = app::update(
        &mut app,
        Msg::Core(AppMsg::Tab(fastfiler_core::update_app::TabMsg::Add)),
    );
    let second_pane = app::focused_pane_for_test(&app);
    assert_ne!(first_pane, second_pane);
    let _ = app::update(
        &mut app,
        Msg::List(
            second_pane,
            fastfiler_iced::widgets::file_list::ListEvent::BoundsChanged {
                x: 0.0,
                y: 0.0,
                w: 800.0,
                h: 600.0,
            },
        ),
    );
    // 同じ座標のヒットは「アクティブタブのペイン」に解決されるべき
    let hit = app::ole_hit_for_test(&app, 400.0, 300.0);
    assert_eq!(
        hit,
        Some(second_pane),
        "非表示タブの古い矩形に解決されている (ドロップ先取り違えバグ)"
    );
}
