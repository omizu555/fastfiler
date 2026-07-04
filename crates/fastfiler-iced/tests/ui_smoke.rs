//! UI 操作レベルのスモークテスト (iced_test のヘッドレス Simulator)。
//!
//! 「ボタンが押せて、正しいメッセージが飛び、画面が遷移する」ことを
//! 実描画なしで検証する。ファイル操作そのものは対象外 (core の単体テスト側)。
//!
//! 制約: 直描きのカスタム widget (一覧・ツリー・タブ) の文字はセレクタで
//! 探せないため、標準 widget (ボタン等) はテキスト、カスタム widget は
//! 座標クリックで叩く。

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
