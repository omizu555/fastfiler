# 第99章: 未確定事項

本章は Question Bank のうち最終的に解決できなかった項目（`status: abandoned`）を集約する。

## 1. 未解決（abandoned）項目: 0 件

`abandoned`（恒久的に解決不能として打ち切った）項目は **0 件**。Phase 5 の対話で全 73 件に回答を付与した。

## 2. 推論で回答したが SME 確認が望ましい項目: 44 件

以下は Phase 5 で **コード根拠の推論により回答済み**（`status: answered`）だが、設計意図・運用方針・脅威モデルに関わるため、保守担当者による SME 確認を推奨する項目である。回答本文は `questions.json` に記録されている。

| ID | 重要度 | カテゴリ | 要旨 |
|---|---|---|---|
| Q-001 | nice-to-have | architecture_decision | fastfiler-domain は edition 2021 / rust-version 1… |
| Q-004 | important | operational_requirement | 設定・セッションの保存先 (%APPDATA%\FastFiler 配下と推測) と、persi… |
| Q-006 | nice-to-have | data_model | `lib.rs` のモジュールドキュメント (12-15行の不採用一覧やモジュール列挙) に `… |
| Q-007 | nice-to-have | data_model | `fastfiler-domain` の Cargo.toml description にある「… |
| Q-008 | nice-to-have | external_integration | domain は windows クレート 0.58、gpui は 0.61 と分かれているが、… |
| Q-009 | nice-to-have | operational_requirement | CONTEXT.md は share ノードの永続化先を `settings.ron` と記すが… |
| Q-010 | nice-to-have | data_model | `fastfiler-domain` に publish 指定がない (gpui は publi… |
| Q-011 | important | data_model | `events.rs` のモジュールドキュメントは「Tauri アダプタ (tauri_sink… |
| Q-012 | nice-to-have | data_model | ドメインの windows 0.58 と GUI の 0.61 のバージョン差は意図的な分離か、… |
| Q-014 | nice-to-have | architecture_decision | ドメイン edition 2021 / GUI edition 2024 を敢えて揃えていないの… |
| Q-015 | nice-to-have | architecture_decision | EventSink → ChannelSink の継ぎ目で、ドメイン側が「どのイベント名 / J… |
| Q-016 | nice-to-have | architecture_decision | build.rs がアイコン埋め込み以外のビルド時責務 (バージョン埋め込み・マニフェスト・コー… |
| Q-017 | nice-to-have | architecture_decision | on_domain_event が分岐する文字列イベント名は、ドメイン側 (file_jobs.… |
| Q-019 | nice-to-have | performance | fs-change は 150ms、schedule_save は 800ms とデバウンス値が… |
| Q-022 | important | architecture_decision | run_job は body 内で panic した場合、unregister が呼ばれず Jo… |
| Q-026 | important | architecture_decision | restore_from_trash の複数一致時に deleted_at 最近傍を選ぶロジック… |
| Q-027 | important | architecture_decision | delete_path（file_ops）はゴミ箱を経由しない物理削除である。GUI の通常削除… |
| Q-028 | nice-to-have | architecture_decision | shell.rs の show_shell_context_menu は IContextMen… |
| Q-030 | nice-to-have | architecture_decision | CDropTarget の assert_thread は debug_assert のためリリ… |
| Q-031 | nice-to-have | external_integration | shell_assoc.rs の Folder/Directory 関連付けは HKCU の S… |
| Q-032 | nice-to-have | operational_requirement | icons.rs のアルファ全 0 判定でマスクを適用するヒューリスティックは、正規の完全透明ア… |
| Q-035 | nice-to-have | external_integration | Everything バックエンド失敗時の builtin 自動フォールバックは仕様として保証さ… |
| Q-039 | nice-to-have | architecture_decision | path_util.volume_key は junction/subst を区別できないと明記… |
| Q-040 | nice-to-have | external_integration | シェルの操作キー (分割・F6・Ctrl+Tab 等) はすべて PaneView 側のキーハン… |
| Q-042 | nice-to-have | operational_requirement | ウィンドウ位置保存で `window.window_bounds()` (GetWindowPl… |
| Q-044 | nice-to-have | architecture_decision | 設定オーバーレイはモーダルだが、背景クリックで閉じる一方 Esc/Enter も受ける。ポート未… |
| Q-049 | nice-to-have | architecture_decision | Undo は移動ジョブが全件成功したときだけ履歴へ push し、部分成功は記録しない (on_… |
| Q-050 | nice-to-have | architecture_decision | row_at_y と update_rubber と render_rubber が Unifo… |
| Q-051 | nice-to-have | architecture_decision | tree.rs の children_of が呼ぶ fs::list_dirs の第2引数 So… |
| Q-054 | nice-to-have | architecture_decision | UNC サーバノードはローカルドライブと違い「展開トグルを持たない見出し」だが、サーバ配下に複数… |
| Q-056 | nice-to-have | external_integration | IME 未確定 (marked_range) の確定タイミングと、確定後に selected_r… |
| Q-057 | important | operational_requirement | テーマ再読み込み時の Box::leak によるメモリリークは、設計上「許容」と明記されているが… |
| Q-059 | nice-to-have | architecture_decision | ホットキー combo の normalize は ctrl/alt/shift のみ修飾子とし… |
| Q-061 | nice-to-have | performance | 設定変更は settings_store::update で即時保存されるが、連続変更 (例: … |
| Q-062 | nice-to-have | operational_requirement | `write_atomic` は同一ボリューム内 rename のアトミック性を前提とするが、`… |
| Q-063 | important | performance | セッション保存のデバウンスが 800ms である根拠 (体感とデータ保全のトレードオフ) は仕様… |
| Q-064 | nice-to-have | architecture_decision | `node_data` のパス直列化が `to_string_lossy` を使うため、非 UT… |
| Q-065 | nice-to-have | architecture_decision | `activate_existing_window` がウィンドウクラス名ではなくタイトル文字列… |
| Q-067 | nice-to-have | operational_requirement | `.bak` も本体も壊れた場合、現状は「既定状態で起動」へ縮退する。利用者へ「前回状態を復元で… |
| Q-068 | nice-to-have | operational_requirement | ロギングは仕様として「永続ログを持たない (status バー通知 + debug eprint… |
| Q-070 | important | security_compliance | OLE D&D の受信側 (extract_hdrop_paths / parse_paths_… |
| Q-071 | nice-to-have | performance | アイコンキャッシュは reload ごとに作り直されプロセス常駐しない。大規模フォルダの往復で体… |
| Q-072 | nice-to-have | performance | fs-change デバウンスは 150ms 固定・リーディング窓型で、長大コピー中は窓ごとに … |
| Q-073 | nice-to-have | operational_requirement | 多重起動防止はウィンドウ「タイトル」一致 (FindWindowW(NULL, "FastFil… |
