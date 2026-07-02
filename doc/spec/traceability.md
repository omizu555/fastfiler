# トレーサビリティ表

本表は各章・節が参照したソースコード単位（`source-map.json` の SRC 単位）への対応を示す。
`[REF: path:line]` 引用を `build-trace.py` が SRC 単位へ解決した結果に基づく。

| 章 | 節 | 参照ソース（path:lines） |
|---|---|---|
| 01-overview.md | 1.11 本章のまとめ | `crates/fastfiler-domain/src/lib.rs:1-34` |
| 01-overview.md | 1.2 中核アイデンティティと非目標 | `crates/fastfiler-domain/src/lib.rs:1-34` |
| 01-overview.md | 1.4 ドメイン層 — fastfiler-domain クレート | `crates/fastfiler-domain/src/lib.rs:1-34` |
| 01-overview.md | 1.6 起動シーケンス — main の流れ | `crates/fastfiler-gpui/src/main.rs:28-108` |
| 01-overview.md | 1.8 ドメイン層と GUI 層の橋渡し | `crates/fastfiler-domain/src/events.rs:10-13` ; `crates/fastfiler-domain/src/events.rs:14-23` ; `crates/fastfiler-domain/src/events.rs:24-31` ; `crates/fastfiler-domain/src/events.rs:32-32` ; `crates/fastfiler-domain/src/events.rs:33-35` ; `crates/fastfiler-gpui/src/sink.rs:13-16` ; `crates/fastfiler-gpui/src/sink.rs:17-20` ; `crates/fastfiler-gpui/src/sink.rs:21-27` …(+1) |
| 02-architecture.md | 2.10 GPUI アプリのブートストラップ | `crates/fastfiler-gpui/src/main.rs:28-108` ; `crates/fastfiler-gpui/src/win32_single_instance.rs:19-29` ; `crates/fastfiler-gpui/src/win32_single_instance.rs:30-47` |
| 02-architecture.md | 2.11 永続化という第二の継ぎ目 | `crates/fastfiler-gpui/src/persist.rs:28-61` ; `crates/fastfiler-gpui/src/session.rs:16-45` ; `crates/fastfiler-gpui/src/settings_store.rs:13-36` |
| 02-architecture.md | 2.12 アーキテクチャ図 | `crates/fastfiler-domain/src/events.rs:10-13` ; `crates/fastfiler-gpui/src/sink.rs:28-33` |
| 02-architecture.md | 2.13 確実性と未解決点 | `crates/fastfiler-domain/src/events.rs:10-13` ; `crates/fastfiler-domain/src/lib.rs:1-34` ; `crates/fastfiler-gpui/src/sink.rs:13-16` ; `crates/fastfiler-gpui/src/sink.rs:17-20` ; `crates/fastfiler-gpui/src/sink.rs:21-27` ; `crates/fastfiler-gpui/src/sink.rs:28-33` ; `crates/fastfiler-gpui/build.rs:1-10` |
| 02-architecture.md | 2.5 GUI とドメインを橋渡しする依存 | `crates/fastfiler-domain/src/events.rs:10-13` ; `crates/fastfiler-domain/src/events.rs:14-23` ; `crates/fastfiler-domain/src/events.rs:24-31` ; `crates/fastfiler-domain/src/events.rs:32-32` ; `crates/fastfiler-domain/src/events.rs:33-35` ; `crates/fastfiler-gpui/src/sink.rs:13-16` ; `crates/fastfiler-gpui/src/sink.rs:17-20` ; `crates/fastfiler-gpui/src/sink.rs:28-33` |
| 02-architecture.md | 2.6 ドメイン層のモジュール組織 | `crates/fastfiler-domain/src/lib.rs:1-34` |
| 02-architecture.md | 2.9 build.rs の責務 | `crates/fastfiler-gpui/build.rs:1-10` |
| 03-state-model.md | on_domain_event: 文字列イベントの分岐 | `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 03-state-model.md | コピー/移動ジョブの進捗 | `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 03-state-model.md | タブとペインツリー | `crates/fastfiler-gpui/src/app.rs:72-84` ; `crates/fastfiler-gpui/src/app.rs:85-94` ; `crates/fastfiler-gpui/src/app.rs:95-115` |
| 03-state-model.md | トレース1: ツリーのクリックからフォルダ移動まで | `crates/fastfiler-gpui/src/app.rs:153-1383` ; `crates/fastfiler-gpui/src/pane.rs:341-3319` ; `crates/fastfiler-gpui/src/tree.rs:57-326` |
| 03-state-model.md | トレース2: ファイル監視からの自動更新 | `crates/fastfiler-domain/src/watcher.rs:16-22` ; `crates/fastfiler-domain/src/watcher.rs:27-61` ; `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 03-state-model.md | ドメインイベントのブリッジ: EventSink とチャネル | `crates/fastfiler-domain/src/events.rs:10-13` ; `crates/fastfiler-domain/src/events.rs:14-23` ; `crates/fastfiler-domain/src/events.rs:24-31` ; `crates/fastfiler-domain/src/events.rs:32-32` ; `crates/fastfiler-domain/src/events.rs:33-35` ; `crates/fastfiler-gpui/src/sink.rs:13-16` ; `crates/fastfiler-gpui/src/sink.rs:17-20` ; `crates/fastfiler-gpui/src/sink.rs:21-27` …(+1) |
| 03-state-model.md | ペイン状態: PaneView | `crates/fastfiler-gpui/src/pane.rs:250-253` ; `crates/fastfiler-gpui/src/pane.rs:262-335` |
| 03-state-model.md | ライフサイクルとリーク防止 | `crates/fastfiler-gpui/src/app.rs:153-1383` ; `crates/fastfiler-gpui/src/pane.rs:3320-3327` |
| 03-state-model.md | ルート状態: FastFilerApp | `crates/fastfiler-gpui/src/app.rs:116-152` |
| 03-state-model.md | 再描画の起点 | `crates/fastfiler-gpui/src/pane.rs:3328-3705` |
| 03-state-model.md | 受信ループ: チャネルから UI スレッドへ | `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 03-state-model.md | 変化観測 (observe) による波及 | `crates/fastfiler-gpui/src/app.rs:153-1383` |
| 03-state-model.md | 親子間の型付きイベント: EventEmitter | `crates/fastfiler-gpui/src/app.rs:153-1383` ; `crates/fastfiler-gpui/src/pane.rs:230-244` ; `crates/fastfiler-gpui/src/pane.rs:245-249` ; `crates/fastfiler-gpui/src/pane.rs:341-3319` ; `crates/fastfiler-gpui/src/tree.rs:23-29` ; `crates/fastfiler-gpui/src/tree.rs:30-32` ; `crates/fastfiler-gpui/src/tree.rs:57-326` |
| 04-domain-fs.md | 4.0 この章の位置づけ | `crates/fastfiler-domain/src/error.rs:5-27` ; `crates/fastfiler-domain/src/error.rs:28-47` ; `crates/fastfiler-domain/src/error.rs:48-53` ; `crates/fastfiler-domain/src/events.rs:10-13` ; `crates/fastfiler-domain/src/events.rs:14-23` ; `crates/fastfiler-domain/src/events.rs:24-31` |
| 04-domain-fs.md | 4.1.1 データ型: FileEntry / DriveInfo / DiskInfo | `crates/fastfiler-domain/src/fs.rs:13-24` ; `crates/fastfiler-domain/src/fs.rs:25-33` ; `crates/fastfiler-domain/src/fs.rs:34-39` ; `crates/fastfiler-domain/src/fs.rs:40-46` ; `crates/fastfiler-domain/src/fs.rs:47-52` ; `crates/fastfiler-domain/src/fs.rs:53-56` |
| 04-domain-fs.md | 4.1.2 list_dir — 全エントリ列挙 | `crates/fastfiler-domain/src/fs.rs:57-95` |
| 04-domain-fs.md | 4.1.3 stat_path / list_dirs | `crates/fastfiler-domain/src/fs.rs:96-117` ; `crates/fastfiler-domain/src/fs.rs:118-149` |
| 04-domain-fs.md | 4.1.4 home_dir / list_drives / disk_free | `crates/fastfiler-domain/src/fs.rs:150-156` ; `crates/fastfiler-domain/src/fs.rs:157-272` ; `crates/fastfiler-domain/src/fs.rs:273-305` |
| 04-domain-fs.md | 4.2.1 基本 4 操作 | `crates/fastfiler-domain/src/file_ops.rs:12-16` ; `crates/fastfiler-domain/src/file_ops.rs:17-21` ; `crates/fastfiler-domain/src/file_ops.rs:22-35` ; `crates/fastfiler-domain/src/file_ops.rs:36-48` ; `crates/fastfiler-domain/src/file_ops.rs:49-68` ; `crates/fastfiler-domain/src/file_ops.rs:69-85` |
| 04-domain-fs.md | 4.2.2 delete_to_trash — Windows ゴミ箱送り | `crates/fastfiler-domain/src/file_ops.rs:86-105` |
| 04-domain-fs.md | 4.2.3 Undo 経路用の上書き禁止操作 | `crates/fastfiler-domain/src/file_ops.rs:106-118` ; `crates/fastfiler-domain/src/file_ops.rs:119-144` ; `crates/fastfiler-domain/src/file_ops.rs:145-169` |
| 04-domain-fs.md | 4.2.4 restore_from_trash — ゴミ箱からの復元 | `crates/fastfiler-domain/src/file_ops.rs:170-184` ; `crates/fastfiler-domain/src/undo.rs:21-25` ; `crates/fastfiler-domain/src/undo.rs:26-36` |
| 04-domain-fs.md | 4.3.1 JobRegistry — キャンセルフラグの管理 | `crates/fastfiler-domain/src/file_jobs.rs:21-24` ; `crates/fastfiler-domain/src/file_jobs.rs:25-129` |
| 04-domain-fs.md | 4.3.2 ジョブのペイロード型 | `crates/fastfiler-domain/src/file_jobs.rs:25-129` ; `crates/fastfiler-domain/src/file_jobs.rs:130-135` ; `crates/fastfiler-domain/src/file_jobs.rs:136-147` ; `crates/fastfiler-domain/src/file_jobs.rs:148-159` |
| 04-domain-fs.md | 4.3.3 サイズスキャンと進捗スロットリング | `crates/fastfiler-domain/src/file_jobs.rs:160-174` ; `crates/fastfiler-domain/src/file_jobs.rs:175-178` ; `crates/fastfiler-domain/src/file_jobs.rs:179-186` ; `crates/fastfiler-domain/src/file_jobs.rs:187-213` |
| 04-domain-fs.md | 4.3.4 進捗付きコピー / 再帰コピー / 再帰削除 | `crates/fastfiler-domain/src/file_jobs.rs:214-246` ; `crates/fastfiler-domain/src/file_jobs.rs:247-273` ; `crates/fastfiler-domain/src/file_jobs.rs:274-307` |
| 04-domain-fs.md | 4.3.5 run_job — ジョブのライフサイクル | `crates/fastfiler-domain/src/file_jobs.rs:308-375` |
| 04-domain-fs.md | 4.3.6 run_copy / run_move / run_delete | `crates/fastfiler-domain/src/file_jobs.rs:25-129` |
| 04-domain-fs.md | 4.3.7 スレッディング: ジョブはどう起動されるか | `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 04-domain-fs.md | 4.4 `watcher.rs` — ディレクトリ監視 | `crates/fastfiler-domain/src/watcher.rs:16-22` ; `crates/fastfiler-domain/src/watcher.rs:23-26` ; `crates/fastfiler-domain/src/watcher.rs:27-61` ; `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 04-domain-fs.md | 4.5 横断的な観察: エラー処理・上書きポリシー・リンク扱い | `crates/fastfiler-domain/src/error.rs:5-27` ; `crates/fastfiler-domain/src/file_jobs.rs:160-174` ; `crates/fastfiler-domain/src/file_ops.rs:106-118` ; `crates/fastfiler-domain/src/fs.rs:57-95` ; `crates/fastfiler-domain/src/fs.rs:96-117` |
| 05-domain-shell.md | 5.2 ShellExecuteW による既定アプリ起動とエクスプローラ表示 | `crates/fastfiler-domain/src/shell.rs:121-170` ; `crates/fastfiler-domain/src/shell.rs:207-220` |
| 05-domain-shell.md | 5.3 シェルコンテキストメニュー（IContextMenu）と PIDL のライフサイクル | `crates/fastfiler-domain/src/shell.rs:19-31` ; `crates/fastfiler-domain/src/shell.rs:32-120` |
| 05-domain-shell.md | 5.4 ファイルタイプ関連付け（shell_assoc.rs） | `crates/fastfiler-domain/src/shell_assoc.rs:22-26` ; `crates/fastfiler-domain/src/shell_assoc.rs:27-77` ; `crates/fastfiler-domain/src/shell_assoc.rs:78-96` ; `crates/fastfiler-domain/src/shell_assoc.rs:97-101` ; `crates/fastfiler-domain/src/shell_assoc.rs:102-106` ; `crates/fastfiler-domain/src/shell_assoc.rs:107-111` ; `crates/fastfiler-domain/src/shell_assoc.rs:112-130` ; `crates/fastfiler-domain/src/shell_assoc.rs:131-163` |
| 05-domain-shell.md | 5.6 アイコン抽出（icons.rs） | `crates/fastfiler-domain/src/icons.rs:250-258` ; `crates/fastfiler-domain/src/icons.rs:259-267` ; `crates/fastfiler-domain/src/icons.rs:268-272` ; `crates/fastfiler-domain/src/icons.rs:273-275` |
| 05-domain-shell.md | 5.9 この章の安全性に関するまとめ | `crates/fastfiler-domain/src/ole_dnd.rs:90-122` ; `crates/fastfiler-domain/src/ole_dnd.rs:123-138` ; `crates/fastfiler-domain/src/ole_dnd.rs:143-168` |
| 05-domain-shell.md | CDataObject（IDataObject 実装） | `crates/fastfiler-domain/src/ole_dnd.rs:266-269` ; `crates/fastfiler-domain/src/ole_dnd.rs:270-273` ; `crates/fastfiler-domain/src/ole_dnd.rs:274-281` ; `crates/fastfiler-domain/src/ole_dnd.rs:282-291` ; `crates/fastfiler-domain/src/ole_dnd.rs:292-298` ; `crates/fastfiler-domain/src/ole_dnd.rs:299-333` ; `crates/fastfiler-domain/src/ole_dnd.rs:334-451` |
| 05-domain-shell.md | CDropSource（IDropSource 実装） | `crates/fastfiler-domain/src/ole_dnd.rs:57-73` ; `crates/fastfiler-domain/src/ole_dnd.rs:74-78` ; `crates/fastfiler-domain/src/ole_dnd.rs:334-451` ; `crates/fastfiler-domain/src/ole_dnd.rs:452-456` ; `crates/fastfiler-domain/src/ole_dnd.rs:457-483` |
| 05-domain-shell.md | CF_HDROP の組み立てと HGLOBAL 確保 | `crates/fastfiler-domain/src/win_clipboard.rs:31-133` |
| 05-domain-shell.md | OLE 初期化とドロップ時の修飾キー読み取り | `crates/fastfiler-domain/src/ole_dnd.rs:580-588` ; `crates/fastfiler-domain/src/ole_dnd.rs:589-603` ; `crates/fastfiler-domain/src/ole_dnd.rs:604-618` ; `crates/fastfiler-domain/src/ole_dnd.rs:619-624` ; `crates/fastfiler-domain/src/ole_dnd.rs:625-633` |
| 05-domain-shell.md | STA スレッドへの隔離（UI スレッド再入の回避） | `crates/fastfiler-domain/src/shell.rs:121-170` ; `crates/fastfiler-domain/src/shell.rs:171-188` ; `crates/fastfiler-domain/src/shell.rs:189-206` |
| 05-domain-shell.md | STGMEDIUM と登録の RAII | `crates/fastfiler-domain/src/ole_dnd.rs:169-170` ; `crates/fastfiler-domain/src/ole_dnd.rs:171-181` ; `crates/fastfiler-domain/src/ole_dnd.rs:182-220` ; `crates/fastfiler-domain/src/ole_dnd.rs:221-241` ; `crates/fastfiler-domain/src/ole_dnd.rs:242-265` ; `crates/fastfiler-domain/src/ole_dnd.rs:801-806` ; `crates/fastfiler-domain/src/ole_dnd.rs:807-812` ; `crates/fastfiler-domain/src/ole_dnd.rs:813-831` …(+1) |
| 05-domain-shell.md | start_drag と DoDragDrop、そして「移動なら元を消す」の二条件 | `crates/fastfiler-domain/src/ole_dnd.rs:50-56` ; `crates/fastfiler-domain/src/ole_dnd.rs:57-73` ; `crates/fastfiler-domain/src/ole_dnd.rs:457-483` ; `crates/fastfiler-domain/src/ole_dnd.rs:484-484` ; `crates/fastfiler-domain/src/ole_dnd.rs:485-486` ; `crates/fastfiler-domain/src/ole_dnd.rs:487-579` |
| 05-domain-shell.md | 受信側：CDropTarget（IDropTarget 実装） | `crates/fastfiler-domain/src/ole_dnd.rs:644-657` ; `crates/fastfiler-domain/src/ole_dnd.rs:658-663` ; `crates/fastfiler-domain/src/ole_dnd.rs:664-682` ; `crates/fastfiler-domain/src/ole_dnd.rs:683-800` |
| 05-domain-shell.md | 貼り付け側の読み出し | `crates/fastfiler-domain/src/win_clipboard.rs:31-133` ; `crates/fastfiler-domain/src/win_clipboard.rs:134-139` ; `crates/fastfiler-domain/src/win_clipboard.rs:152-225` ; `crates/fastfiler-domain/src/win_clipboard.rs:239-278` |
| 05-domain-shell.md | 送信側のデータ構造と HGLOBAL ヘルパ | `crates/fastfiler-domain/src/ole_dnd.rs:90-122` ; `crates/fastfiler-domain/src/ole_dnd.rs:123-138` ; `crates/fastfiler-domain/src/ole_dnd.rs:143-168` |
| 06-domain-services.md | 6.0 この章の位置づけ | `crates/fastfiler-domain/src/search.rs:53-90` |
| 06-domain-services.md | 6.1 エラー型 — `error.rs` | `crates/fastfiler-domain/src/error.rs:5-27` ; `crates/fastfiler-domain/src/error.rs:28-47` ; `crates/fastfiler-domain/src/error.rs:48-53` ; `crates/fastfiler-domain/src/error.rs:54-54` |
| 06-domain-services.md | 6.11 横断的な観察 | `crates/fastfiler-domain/src/ascii_tree.rs:61-83` ; `crates/fastfiler-domain/src/templates.rs:22-30` ; `crates/fastfiler-domain/src/templates.rs:36-66` ; `crates/fastfiler-domain/src/user_commands.rs:49-61` |
| 06-domain-services.md | 6.12 確信度と要確認事項 | `crates/fastfiler-domain/src/search.rs:53-90` |
| 06-domain-services.md | 6.2 パスのボリューム判定 — `path_util.rs` | `crates/fastfiler-domain/src/path_util.rs:18-49` ; `crates/fastfiler-domain/src/path_util.rs:50-61` |
| 06-domain-services.md | 6.3 ASCII ツリー描画 — `ascii_tree.rs` | `crates/fastfiler-domain/src/ascii_tree.rs:12-12` ; `crates/fastfiler-domain/src/ascii_tree.rs:13-13` ; `crates/fastfiler-domain/src/ascii_tree.rs:14-14` ; `crates/fastfiler-domain/src/ascii_tree.rs:15-18` ; `crates/fastfiler-domain/src/ascii_tree.rs:19-48` ; `crates/fastfiler-domain/src/ascii_tree.rs:49-60` ; `crates/fastfiler-domain/src/ascii_tree.rs:61-83` ; `crates/fastfiler-domain/src/ascii_tree.rs:84-105` |
| 06-domain-services.md | 6.4.1 データ型 | `crates/fastfiler-domain/src/search.rs:19-26` ; `crates/fastfiler-domain/src/search.rs:27-36` ; `crates/fastfiler-domain/src/search.rs:37-42` ; `crates/fastfiler-domain/src/search.rs:43-52` |
| 06-domain-services.md | 6.4.2 ジョブ起動とキャンセル | `crates/fastfiler-domain/src/search.rs:53-90` |
| 06-domain-services.md | 6.4.3 バックエンド分岐とフォールバック | `crates/fastfiler-domain/src/search.rs:91-179` |
| 06-domain-services.md | 6.4.4 組み込みウォーカーとマッチャ | `crates/fastfiler-domain/src/search.rs:180-228` ; `crates/fastfiler-domain/src/search.rs:229-251` |
| 06-domain-services.md | 6.5 Everything HTTP クライアント — `everything.rs` | `crates/fastfiler-domain/src/everything.rs:15-22` ; `crates/fastfiler-domain/src/everything.rs:23-34` ; `crates/fastfiler-domain/src/everything.rs:35-41` ; `crates/fastfiler-domain/src/everything.rs:42-43` ; `crates/fastfiler-domain/src/everything.rs:44-57` ; `crates/fastfiler-domain/src/everything.rs:58-131` ; `crates/fastfiler-domain/src/everything.rs:132-139` |
| 06-domain-services.md | 6.6 ファイルテンプレート — `templates.rs` | `crates/fastfiler-domain/src/templates.rs:16-21` ; `crates/fastfiler-domain/src/templates.rs:22-30` ; `crates/fastfiler-domain/src/templates.rs:36-66` ; `crates/fastfiler-domain/src/templates.rs:67-94` ; `crates/fastfiler-domain/src/templates.rs:95-103` ; `crates/fastfiler-domain/src/templates.rs:104-126` ; `crates/fastfiler-domain/src/templates.rs:127-155` |
| 06-domain-services.md | 6.7.1 コマンド定義とプレースホルダ | `crates/fastfiler-domain/src/user_commands.rs:23-44` ; `crates/fastfiler-domain/src/user_commands.rs:273-310` |
| 06-domain-services.md | 6.7.2 ディレクトリ初期化と読み込み | `crates/fastfiler-domain/src/user_commands.rs:49-61` ; `crates/fastfiler-domain/src/user_commands.rs:67-79` ; `crates/fastfiler-domain/src/user_commands.rs:322-411` |
| 06-domain-services.md | 6.7.3 実行フローとセキュリティ対策 | `crates/fastfiler-domain/src/user_commands.rs:67-79` ; `crates/fastfiler-domain/src/user_commands.rs:80-84` ; `crates/fastfiler-domain/src/user_commands.rs:85-182` ; `crates/fastfiler-domain/src/user_commands.rs:183-217` ; `crates/fastfiler-domain/src/user_commands.rs:229-268` ; `crates/fastfiler-domain/src/user_commands.rs:269-272` ; `crates/fastfiler-domain/src/user_commands.rs:311-321` |
| 06-domain-services.md | 6.8 Undo モデル — `undo.rs` | `crates/fastfiler-domain/src/undo.rs:21-25` ; `crates/fastfiler-domain/src/undo.rs:26-36` ; `crates/fastfiler-domain/src/undo.rs:37-49` ; `crates/fastfiler-domain/src/undo.rs:50-54` ; `crates/fastfiler-domain/src/undo.rs:55-85` ; `crates/fastfiler-domain/src/undo.rs:86-89` ; `crates/fastfiler-domain/src/undo.rs:90-125` |
| 07-gui-app.md | 7.10 設定オーバーレイ (モーダル) のレイアウト | `crates/fastfiler-gpui/src/app.rs:153-1383` ; `crates/fastfiler-gpui/src/app.rs:1384-1701` |
| 07-gui-app.md | 7.11 フッターとウィンドウ位置の保存 | `crates/fastfiler-gpui/src/app.rs:153-1383` ; `crates/fastfiler-gpui/src/app.rs:1384-1701` |
| 07-gui-app.md | 7.2 プロセス起動とウィンドウブートストラップ (`main.rs`) | `crates/fastfiler-gpui/src/app.rs:1893-1896` ; `crates/fastfiler-gpui/src/main.rs:28-108` |
| 07-gui-app.md | 7.3 シェルの状態: `FastFilerApp` 構造体 | `crates/fastfiler-gpui/src/app.rs:72-84` ; `crates/fastfiler-gpui/src/app.rs:85-94` ; `crates/fastfiler-gpui/src/app.rs:116-152` |
| 07-gui-app.md | 7.4 最上位レイアウトツリー (`Render for FastFilerApp`) | `crates/fastfiler-gpui/src/app.rs:1384-1701` |
| 07-gui-app.md | 7.5 縦タブバーのレイアウトと描画 | `crates/fastfiler-gpui/src/app.rs:55-56` ; `crates/fastfiler-gpui/src/app.rs:57-71` ; `crates/fastfiler-gpui/src/app.rs:153-1383` ; `crates/fastfiler-gpui/src/app.rs:1384-1701` |
| 07-gui-app.md | 7.6 リサイズハンドルとドラッグ機構 | `crates/fastfiler-gpui/src/app.rs:34-37` ; `crates/fastfiler-gpui/src/app.rs:38-43` ; `crates/fastfiler-gpui/src/app.rs:153-1383` ; `crates/fastfiler-gpui/src/app.rs:1384-1701` ; `crates/fastfiler-gpui/src/app.rs:1702-1718` |
| 07-gui-app.md | 7.7 ペインツリーの再帰描画 (`render_node`) | `crates/fastfiler-gpui/src/app.rs:153-1383` ; `crates/fastfiler-gpui/src/app.rs:1384-1701` ; `crates/fastfiler-gpui/src/app.rs:1702-1718` ; `crates/fastfiler-gpui/src/app.rs:1719-1746` ; `crates/fastfiler-gpui/src/app.rs:1747-1753` ; `crates/fastfiler-gpui/src/app.rs:1754-1760` ; `crates/fastfiler-gpui/src/app.rs:1761-1768` ; `crates/fastfiler-gpui/src/app.rs:1769-1780` …(+4) |
| 07-gui-app.md | 7.8 グローバルアクションとイベントルーティング | `crates/fastfiler-gpui/src/app.rs:153-1383` |
| 07-gui-app.md | 7.9 キーバインドとフォーカス | `crates/fastfiler-gpui/src/app.rs:153-1383` ; `crates/fastfiler-gpui/src/app.rs:1384-1701` ; `crates/fastfiler-gpui/src/main.rs:28-108` |
| 08-gui-pane.md | PaneView が保持する状態 | `crates/fastfiler-gpui/src/pane.rs:262-335` ; `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 08-gui-pane.md | この章で扱うもの | `crates/fastfiler-gpui/src/pane.rs:262-335` |
| 08-gui-pane.md | まだ確認しきれていない点 | `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 08-gui-pane.md | アドレスバーと検索 | `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 08-gui-pane.md | キーボード入力の経路 | `crates/fastfiler-gpui/src/hotkeys.rs:39-59` ; `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 08-gui-pane.md | コンテキストメニュー | `crates/fastfiler-gpui/src/pane.rs:190-203` ; `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 08-gui-pane.md | ソートと列幅 | `crates/fastfiler-gpui/src/pane.rs:341-3319` ; `crates/fastfiler-gpui/src/pane.rs:3328-3705` |
| 08-gui-pane.md | ドメインイベントとライフサイクル | `crates/fastfiler-gpui/src/pane.rs:341-3319` ; `crates/fastfiler-gpui/src/pane.rs:3320-3327` |
| 08-gui-pane.md | ドラッグ&ドロップの完了処理 | `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 08-gui-pane.md | ドラッグ&ドロップの開始 | `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 08-gui-pane.md | ドロップを受け取る側 | `crates/fastfiler-gpui/src/pane.rs:341-3319` ; `crates/fastfiler-gpui/src/pane.rs:3744-3755` ; `crates/fastfiler-gpui/src/pane.rs:3756-3790` |
| 08-gui-pane.md | マウスのクリックと行の活性化 | `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 08-gui-pane.md | ラバーバンド (矩形) 選択 | `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 08-gui-pane.md | 描画の全体構成 | `crates/fastfiler-gpui/src/pane.rs:341-3319` ; `crates/fastfiler-gpui/src/pane.rs:3328-3705` |
| 08-gui-pane.md | 選択モデル | `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 09-gui-tree-input.md | 9.10 TextInput の状態モデルとキーバインド | `crates/fastfiler-gpui/src/main.rs:28-108` ; `crates/fastfiler-gpui/src/text_input.rs:44-61` ; `crates/fastfiler-gpui/src/text_input.rs:62-73` ; `crates/fastfiler-gpui/src/text_input.rs:634-675` |
| 09-gui-tree-input.md | 9.11 カーソル移動と選択範囲 | `crates/fastfiler-gpui/src/text_input.rs:74-322` |
| 09-gui-tree-input.md | 9.12 文字の挿入と削除 | `crates/fastfiler-gpui/src/text_input.rs:74-322` ; `crates/fastfiler-gpui/src/text_input.rs:323-449` |
| 09-gui-tree-input.md | 9.13 クリップボード操作 | `crates/fastfiler-gpui/src/text_input.rs:74-322` |
| 09-gui-tree-input.md | 9.14 マウスによるカーソル配置と範囲選択 | `crates/fastfiler-gpui/src/text_input.rs:74-322` |
| 09-gui-tree-input.md | 9.15 IME 対応 — EntityInputHandler | `crates/fastfiler-gpui/src/text_input.rs:74-322` ; `crates/fastfiler-gpui/src/text_input.rs:323-449` |
| 09-gui-tree-input.md | 9.16 カスタム Element による低レベル描画 | `crates/fastfiler-gpui/src/text_input.rs:468-633` ; `crates/fastfiler-gpui/src/text_input.rs:634-675` ; `crates/fastfiler-gpui/src/text_input.rs:676-680` |
| 09-gui-tree-input.md | 9.17 TextInput の呼び出し側 — 1 つの実装を 4 用途で共有 | `crates/fastfiler-gpui/src/app.rs:153-1383` ; `crates/fastfiler-gpui/src/pane.rs:44-50` ; `crates/fastfiler-gpui/src/pane.rs:51-57` ; `crates/fastfiler-gpui/src/pane.rs:84-95` ; `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 09-gui-tree-input.md | 9.2 TreeView の状態モデル | `crates/fastfiler-gpui/src/tree.rs:33-41` ; `crates/fastfiler-gpui/src/tree.rs:42-56` ; `crates/fastfiler-gpui/src/tree.rs:402-417` |
| 09-gui-tree-input.md | 9.3 表示リストの再構築 — rebuild と push_item | `crates/fastfiler-gpui/src/tree.rs:57-326` |
| 09-gui-tree-input.md | 9.4 遅延読み込みとキャッシュ — children_of と toggle | `crates/fastfiler-gpui/src/tree.rs:57-326` |
| 09-gui-tree-input.md | 9.5 reveal — ペインへの追従 | `crates/fastfiler-gpui/src/app.rs:153-1383` ; `crates/fastfiler-gpui/src/tree.rs:57-326` |
| 09-gui-tree-input.md | 9.6 UNC share の登録と解除 | `crates/fastfiler-gpui/src/app.rs:153-1383` ; `crates/fastfiler-gpui/src/tree.rs:57-326` ; `crates/fastfiler-gpui/src/tree.rs:377-388` ; `crates/fastfiler-gpui/src/tree.rs:389-401` |
| 09-gui-tree-input.md | 9.7 行の描画 — render_item | `crates/fastfiler-gpui/src/tree.rs:57-326` |
| 09-gui-tree-input.md | 9.8 仮想化描画とイベント発行 | `crates/fastfiler-gpui/src/app.rs:153-1383` ; `crates/fastfiler-gpui/src/tree.rs:23-29` ; `crates/fastfiler-gpui/src/tree.rs:30-32` ; `crates/fastfiler-gpui/src/tree.rs:57-326` ; `crates/fastfiler-gpui/src/tree.rs:327-376` |
| 09-gui-tree-input.md | 9.9 ツリーへのファイルドロップに関する事実確認 | `crates/fastfiler-gpui/src/app.rs:38-43` ; `crates/fastfiler-gpui/src/app.rs:44-46` ; `crates/fastfiler-gpui/src/tree.rs:57-326` |
| 10-theme-settings.md | 10.1.2 スキーマと既定値 | `crates/fastfiler-gpui/src/settings_store.rs:13-36` ; `crates/fastfiler-gpui/src/settings_store.rs:37-40` ; `crates/fastfiler-gpui/src/settings_store.rs:41-44` ; `crates/fastfiler-gpui/src/settings_store.rs:45-48` ; `crates/fastfiler-gpui/src/settings_store.rs:49-52` ; `crates/fastfiler-gpui/src/settings_store.rs:53-66` |
| 10-theme-settings.md | 10.1.3 static ストアとアクセサ | `crates/fastfiler-gpui/src/pane.rs:341-3319` ; `crates/fastfiler-gpui/src/settings_store.rs:67-75` ; `crates/fastfiler-gpui/src/settings_store.rs:76-81` ; `crates/fastfiler-gpui/src/settings_store.rs:82-93` ; `crates/fastfiler-gpui/src/settings_store.rs:94-98` ; `crates/fastfiler-gpui/src/settings_store.rs:99-111` |
| 10-theme-settings.md | 10.1.4 ロード処理 — クラッシュ安全な読み込み | `crates/fastfiler-gpui/src/persist.rs:62-73` |
| 10-theme-settings.md | 10.1.5 セーブ処理 — read-modify-write とアトミック書き込み | `crates/fastfiler-gpui/src/persist.rs:28-61` ; `crates/fastfiler-gpui/src/settings_store.rs:99-111` |
| 10-theme-settings.md | 10.2.3 組み込みプリセット 3 種 | `crates/fastfiler-gpui/src/theme.rs:77-119` ; `crates/fastfiler-gpui/src/theme.rs:206-207` ; `crates/fastfiler-gpui/src/theme.rs:208-216` |
| 10-theme-settings.md | 10.2.4 現在テーマの保持と解決 — th() / CURRENT / set_by_name | `crates/fastfiler-gpui/src/theme.rs:217-219` ; `crates/fastfiler-gpui/src/theme.rs:220-231` ; `crates/fastfiler-gpui/src/theme.rs:232-253` ; `crates/fastfiler-gpui/src/theme.rs:254-263` |
| 10-theme-settings.md | 10.2.5 ユーザーテーマ — JSON ファイルからの読み込み | `crates/fastfiler-gpui/src/theme.rs:254-263` ; `crates/fastfiler-gpui/src/theme.rs:264-274` ; `crates/fastfiler-gpui/src/theme.rs:289-361` ; `crates/fastfiler-gpui/src/theme.rs:362-405` |
| 10-theme-settings.md | 10.2.6 スタイル (形状プリセット) | `crates/fastfiler-gpui/src/theme.rs:494-543` ; `crates/fastfiler-gpui/src/theme.rs:544-551` ; `crates/fastfiler-gpui/src/theme.rs:552-552` ; `crates/fastfiler-gpui/src/theme.rs:553-554` ; `crates/fastfiler-gpui/src/theme.rs:555-578` ; `crates/fastfiler-gpui/src/theme.rs:579-585` ; `crates/fastfiler-gpui/src/theme.rs:586-595` |
| 10-theme-settings.md | 10.2.7 UI フォントサイズのキャッシュ | `crates/fastfiler-gpui/src/theme.rs:601-611` ; `crates/fastfiler-gpui/src/theme.rs:612-612` ; `crates/fastfiler-gpui/src/theme.rs:613-614` ; `crates/fastfiler-gpui/src/theme.rs:615-617` ; `crates/fastfiler-gpui/src/theme.rs:618-622` ; `crates/fastfiler-gpui/src/theme.rs:623-632` ; `crates/fastfiler-gpui/src/theme.rs:633-637` ; `crates/fastfiler-gpui/src/theme.rs:638-642` …(+1) |
| 10-theme-settings.md | 10.3.1 カスタマイズ可能なアクション | `crates/fastfiler-gpui/src/hotkeys.rs:17-38` |
| 10-theme-settings.md | 10.3.2 既定割り当てテーブル | `crates/fastfiler-gpui/src/hotkeys.rs:39-59` |
| 10-theme-settings.md | 10.3.3 combo 文字列の正規化 | `crates/fastfiler-gpui/src/hotkeys.rs:76-101` ; `crates/fastfiler-gpui/src/hotkeys.rs:102-118` |
| 10-theme-settings.md | 10.3.4 読み込みとフォールバック生成 | `crates/fastfiler-gpui/src/hotkeys.rs:60-61` ; `crates/fastfiler-gpui/src/hotkeys.rs:62-65` ; `crates/fastfiler-gpui/src/hotkeys.rs:66-75` ; `crates/fastfiler-gpui/src/hotkeys.rs:119-164` |
| 10-theme-settings.md | 10.3.5 キーストロークの解決と消費 | `crates/fastfiler-gpui/src/hotkeys.rs:165-175` ; `crates/fastfiler-gpui/src/hotkeys.rs:176-182` ; `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 10-theme-settings.md | 10.4.1 起動シーケンス (main.rs) | `crates/fastfiler-gpui/src/main.rs:28-108` |
| 10-theme-settings.md | 10.4.2 変更伝播 (app.rs) | `crates/fastfiler-gpui/src/app.rs:153-1383` ; `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 11-persistence-session.md | 11.1.2 補助関数 `with_suffix` | `crates/fastfiler-gpui/src/persist.rs:19-27` |
| 11-persistence-session.md | 11.1.3 中核処理 `write_atomic` — tmp + fsync + rename | `crates/fastfiler-gpui/src/persist.rs:28-61` |
| 11-persistence-session.md | 11.1.4 読み込み側 `load_with_backup` — 本体 → `.bak` フォールバック | `crates/fastfiler-gpui/src/persist.rs:62-73` |
| 11-persistence-session.md | 11.2.1 セッションデータの形式 `SessionData` | `crates/fastfiler-gpui/src/session.rs:16-45` ; `crates/fastfiler-gpui/src/session.rs:46-49` ; `crates/fastfiler-gpui/src/session.rs:50-53` ; `crates/fastfiler-gpui/src/session.rs:54-60` |
| 11-persistence-session.md | 11.2.2 ペインツリーの直列化表現 `NodeData` | `crates/fastfiler-gpui/src/session.rs:54-60` ; `crates/fastfiler-gpui/src/session.rs:61-77` |
| 11-persistence-session.md | 11.2.3 保存先パスと load / save | `crates/fastfiler-gpui/src/persist.rs:19-27` ; `crates/fastfiler-gpui/src/session.rs:78-86` ; `crates/fastfiler-gpui/src/session.rs:87-92` ; `crates/fastfiler-gpui/src/session.rs:93-101` |
| 11-persistence-session.md | 11.2.4 保存タイミング — デバウンスと終了時フック | `crates/fastfiler-gpui/src/app.rs:153-1383` |
| 11-persistence-session.md | 11.2.5 メモリ表現 ⇄ 直列化表現の変換 | `crates/fastfiler-gpui/src/app.rs:153-1383` ; `crates/fastfiler-gpui/src/app.rs:1719-1746` |
| 11-persistence-session.md | 11.2.6 起動シーケンスでのセッション復元 | `crates/fastfiler-gpui/src/main.rs:28-108` |
| 11-persistence-session.md | 11.3 設定の永続化も同じ基盤に載る (`settings_store.rs`) | `crates/fastfiler-gpui/src/settings_store.rs:67-75` ; `crates/fastfiler-gpui/src/settings_store.rs:82-93` ; `crates/fastfiler-gpui/src/settings_store.rs:94-98` ; `crates/fastfiler-gpui/src/settings_store.rs:99-111` |
| 11-persistence-session.md | 11.4.1 名前付き Mutex による多重起動判定 | `crates/fastfiler-gpui/src/win32_single_instance.rs:19-29` ; `crates/fastfiler-gpui/src/win32_single_instance.rs:30-47` |
| 11-persistence-session.md | 11.4.2 既存ウィンドウの前面化 | `crates/fastfiler-gpui/src/main.rs:28-108` ; `crates/fastfiler-gpui/src/win32_single_instance.rs:30-47` ; `crates/fastfiler-gpui/src/win32_single_instance.rs:48-60` |
| 11-persistence-session.md | 11.4.3 起動シーケンスへの組み込み | `crates/fastfiler-gpui/src/main.rs:28-108` |
| 12-cross-cutting.md | 12.1.1 一覧描画の仮想化 | `crates/fastfiler-gpui/src/pane.rs:3328-3705` |
| 12-cross-cutting.md | 12.1.2 アイコン取得のコスト削減 | `crates/fastfiler-gpui/src/pane.rs:341-3319` ; `crates/fastfiler-gpui/src/pane.rs:3328-3705` ; `crates/fastfiler-gpui/src/pane.rs:3706-3724` ; `crates/fastfiler-gpui/src/pane.rs:3725-3743` |
| 12-cross-cutting.md | 12.1.3 監視イベントのバースト対策 (デバウンス) | `crates/fastfiler-domain/src/watcher.rs:27-61` ; `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 12-cross-cutting.md | 12.1.4 ブロッキング処理の追い出し (非同期ジョブ) | `crates/fastfiler-domain/src/shell.rs:189-206` ; `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 12-cross-cutting.md | 12.1.5 入力サイズの上限 (OOM / フリーズ防止) | `crates/fastfiler-domain/src/ole_dnd.rs:484-484` ; `crates/fastfiler-domain/src/ole_dnd.rs:485-486` ; `crates/fastfiler-domain/src/ole_dnd.rs:487-579` |
| 12-cross-cutting.md | 12.2.2 `unsafe` / COM の境界面 | `crates/fastfiler-domain/src/ole_dnd.rs:143-168` ; `crates/fastfiler-domain/src/ole_dnd.rs:169-170` ; `crates/fastfiler-domain/src/ole_dnd.rs:171-181` ; `crates/fastfiler-domain/src/ole_dnd.rs:487-579` ; `crates/fastfiler-domain/src/shell.rs:19-31` ; `crates/fastfiler-domain/src/shell.rs:32-120` |
| 12-cross-cutting.md | 12.2.3 信頼できない入力 (HDROP) の防御的解析 | `crates/fastfiler-domain/src/ole_dnd.rs:143-168` ; `crates/fastfiler-domain/src/ole_dnd.rs:221-241` ; `crates/fastfiler-domain/src/ole_dnd.rs:242-265` |
| 12-cross-cutting.md | 12.2.4 パス取り扱いの安全性 | `crates/fastfiler-gpui/src/pane.rs:3725-3743` ; `crates/fastfiler-gpui/src/pane.rs:3744-3755` ; `crates/fastfiler-gpui/src/pane.rs:3756-3790` |
| 12-cross-cutting.md | 12.2.5 COM アパートメントとスレッディングの一貫性 | `crates/fastfiler-domain/src/ole_dnd.rs:604-618` ; `crates/fastfiler-gpui/src/main.rs:28-108` |
| 12-cross-cutting.md | 12.3.1 クラッシュ安全な永続化 | `crates/fastfiler-gpui/src/persist.rs:28-61` ; `crates/fastfiler-gpui/src/persist.rs:62-73` ; `crates/fastfiler-gpui/src/session.rs:93-101` ; `crates/fastfiler-gpui/src/settings_store.rs:99-111` |
| 12-cross-cutting.md | 12.3.2 多重起動防止と既存ウィンドウ前面化 | `crates/fastfiler-gpui/src/main.rs:28-108` ; `crates/fastfiler-gpui/src/win32_single_instance.rs:30-47` ; `crates/fastfiler-gpui/src/win32_single_instance.rs:48-60` |
| 12-cross-cutting.md | 12.3.3 エラーの可視化とロギングの不在 | `crates/fastfiler-domain/src/ole_dnd.rs:487-579` ; `crates/fastfiler-gpui/src/pane.rs:341-3319` |
| 12-cross-cutting.md | 12.3.4 リソースのライフサイクルと自動解放 | `crates/fastfiler-domain/src/watcher.rs:27-61` ; `crates/fastfiler-gpui/src/pane.rs:341-3319` ; `crates/fastfiler-gpui/src/pane.rs:3320-3327` |
| 12-cross-cutting.md | 12.3.5 グレースフルデグラデーション (段階的縮退) | `crates/fastfiler-domain/src/user_commands.rs:85-182` ; `crates/fastfiler-gpui/src/persist.rs:62-73` ; `crates/fastfiler-gpui/src/win32_single_instance.rs:30-47` |
| 12-cross-cutting.md | コマンドインジェクション対策 (cmd_quote / build_shell_command) | `crates/fastfiler-domain/src/user_commands.rs:183-217` ; `crates/fastfiler-domain/src/user_commands.rs:229-268` ; `crates/fastfiler-domain/src/user_commands.rs:269-272` |
| 12-cross-cutting.md | バイナリプランティング対策 (resolve_in_path) | `crates/fastfiler-domain/src/user_commands.rs:85-182` ; `crates/fastfiler-domain/src/user_commands.rs:229-268` |

## MECE サマリ

- ソース単位総数: 326
- 仕様でカバー: 277 (85.0%)
- 明示的除外: 0
- 未カバー: 49
