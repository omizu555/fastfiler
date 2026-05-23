# FastFiler アーキテクチャ

最終更新: 2026-05-23 (ADR 0001/0003/0004/0005 によるコード削除後)

---

## 1. クレート構成 (Cargo workspace)

```
fastfiler/
├── Cargo.toml                  workspace 定義
├── crates/
│   ├── fastfiler-domain/       OS / GUI 非依存のロジック (lib)
│   │   └── src/
│   │       ├── error.rs        AppError 型 + kind タグ
│   │       ├── events.rs       EventSink trait + NullSink (テスト用)
│   │       ├── fs.rs           list_dir / list_dirs / stat_path
│   │       ├── file_ops.rs     create / rename / copy / move
│   │       ├── file_jobs.rs    JobRegistry (キャンセル可能な転送)
│   │       ├── watcher.rs      WatcherCore (notify ラッパ)
│   │       ├── search.rs       streaming 内蔵検索
│   │       ├── everything.rs   Everything HTTP API クライアント
│   │       ├── shell.rs        ShellExecuteW / open_with_shell
│   │       ├── shell_assoc.rs  拡張子→ProgID ルックアップ
│   │       ├── icons.rs        SHGetFileInfo + ExtractIcon + LRU
│   │       ├── user_commands.rs commands.json のロード + 実行
│   │       ├── templates.rs    テンプレからのファイル作成
│   │       └── win_clipboard.rs Windows クリップボード操作
│   │
│   └── fastfiler-native/       floem ベースの GUI (bin + lib)
│       └── src/
│           ├── main.rs         3 行のエントリ shim → run_app() を呼ぶ
│           ├── lib.rs          run_app() 公開、機能別モジュール宣言
│           ├── logger.rs       ファイルロガー + flog! マクロ
│           ├── hotkeys.rs      KeyCombo パース + dispatch
│           ├── theme/
│           │   ├── mod.rs      色パレット (Light/Dark + 5 プリセット)
│           │   └── fonts.rs    インストール済みフォント取得
│           ├── core/
│           │   ├── mod.rs
│           │   ├── state.rs    AppState / Tab / PaneState / SplitNode
│           │   ├── actions.rs  delete / paste / copy / rename / open
│           │   └── fs_model.rs FileRow / SortKey / 書式整形
│           ├── settings/
│           │   └── mod.rs      AppSettings / PersistedSettings + 設定ダイアログ
│           └── ui/
│               ├── mod.rs
│               ├── app_view.rs  ルートレイアウト + キーハンドラ
│               ├── tabs.rs      縦型タブパネル
│               ├── tree.rs      フォルダツリーペイン
│               ├── pane.rs      フォルダペイン (一覧 / 検索バー / D&D)
│               ├── modal_dialog.rs  新規フォルダ/新規ファイル/リネーム用センターポップアップ
│               ├── footer.rs    ステータスバー
│               └── splitter.rs  ドラッグリサイザ
│
└── doc/                         本ドキュメント群
```

### 1.1 ライブラリ / バイナリ分離

`fastfiler-native` は同一クレート内に `lib.rs` と `main.rs` を持ち、

- `lib.rs` がモジュールツリーを所有し `pub fn run_app()` を公開
- `main.rs` は `fastfiler_native::run_app()` を呼ぶだけ

これにより以下が容易になる:

- 別バイナリ (例: スモークテスト用) から FastFiler を起動できる
- 将来 GUI の差し替え (egui / iced 等) を試す際に core/settings/theme をそのまま再利用可能

### 1.2 後方互換 re-export

`lib.rs` で `pub use core::{state, actions, fs_model};` を行っており、
旧来の `crate::state::Foo` / `crate::actions::Bar` パスはそのまま動作する。
ファイル分割によるインポート修正の連鎖を抑える目的。

---

## 2. 状態モデル

```
AppState (グローバル)
├── tabs:        RwSignal<im::Vector<Tab>>
├── active:      RwSignal<TabId>
├── settings:    AppSettings (永続化対象は serde 経由で settings.ron)
├── theme_rev:   RwSignal<u64>          テーマ変更のたびに +1 → UI が再構築
└── splitter_drag, drag_state, ...
   │
   └─ Tab
       ├── id, title (primary pane の dir 名と連動)
       └── root: RwSignal<SplitNode>     BSP ツリー (横/縦の任意分割)
            │
            └─ SplitNode
                ├── Leaf(PaneState)
                └── Split { dir, children: Vec<SplitNode> }
                  │
                  └─ PaneState (Clone, 全フィールド RwSignal/Arc)
                      ├── id, cur_path, path_input, history
                      ├── rows, stats, selected, anchor, sort_key
                      ├── modal_kind, modal_input
                      ├── search_open, search_query, search_results
                      ├── status_msg, fs_event_signal, fs_change_tick
                      ├── sink (CounterSink)
                      └── watcher (Arc<WatcherCore>)
```

`PaneState` がすべて `RwSignal/Arc` でできているため値渡し可能で、
borrow / lifetime 問題が発生しない構造を維持している。

---

## 3. リアクティビティ方針

- floem の `RwSignal` + `create_effect` がすべての更新ループの中心
- effect の中で `set` する場合は `set_untracked` か「変化時のみ set」で再入を防ぐ
- 大きい木 (split tree など) は `dyn_container` でなく **`dyn_stack` + key 関数** を優先
  - key は `(idx, name, is_dir)` 等、レイアウト同一性を保てる組合せ
- 起動時の重い初期化 (テーマ反映など) は `lib.rs::run_app()` で 1 回だけ実行

---

## 4. クラッシュ耐性メモ

過去に **0xc0000005 (STATUS_ACCESS_VIOLATION)** や **0xc00000fd (STATUS_STACK_OVERFLOW)** が発生した教訓:

1. **削除操作**: COM の `IFileOperation` ではなく `SHFileOperationW` を使用 (SEH 回避)
2. **クリック処理**: `PaneState::click_row` で範囲外インデックスをクランプ (sort/reload 直後対策)
3. **effect の重複**: `dyn_container` 内で `create_effect` を作るとスコープ寿命と整合せず爆発する → `tree_pane` などはトップレベル 1 effect に統合
4. **連鎖更新**: `tabs.set + active.set` のような連続書込は untracked 比較を挟む

---

## 5. 拡張ポイント

| やりたいこと | 触る場所 |
|---|---|
| 新しいファイル操作 | `fastfiler-domain::file_ops` + `core::actions` |
| UI レイアウト変更 | `ui/app_view.rs` |
| 新しいビュー追加 | `ui/` 配下にモジュール追加 → `ui/mod.rs` 登録 |
| 配色 / プリセット追加 | `theme/mod.rs` の `PresetColors` |
| 永続化項目追加 | `settings/mod.rs` の `AppSettings` / `PersistedSettings` |
| ホットキー追加 | `hotkeys.rs` の `dispatch_action` + 既定値 |
| 検索バックエンド追加 | `fastfiler-domain::search` を拡張 + `ui/pane.rs` の effect |

---

## 6. 今後のリファクタ候補

- `ui/pane.rs` (~950 行) → `ui/pane/{list, search, dnd, context_menu, modal}.rs` への分割
- `settings/mod.rs` (~745 行) → `settings/{model, persisted, dialog/{tab_general, tab_theme, ...}}.rs` への分割
- `ui/app_view.rs` の effect 群を `ui/effects.rs` に集約 (見通し改善)

---

## 7. 不採用機能と削除済モジュール

[`adr/`](./adr/) の決定により、以下は **意図的に持たない**。コード上にも残骸を残さず、将来「未完成のフックがある」と誤読されないようにしている。

| 不採用 | ADR | 削除/不在のもの |
|---|---|---|
| ペイン連動 (Red/Blue) | [0001](./adr/0001-remove-pane-linking.md) | `PaneState` から連動フィールドを削除済 |
| プラグイン機構 (JS/WASM) | [0003](./adr/0003-remove-plugin-system.md) | `fastfiler-domain::plugin` / `doc/plugins-sample/` / `AppError::Plugin` / `zip` 依存 |
| 内蔵ターミナル | [0004](./adr/0004-no-builtin-terminal.md) | `fastfiler-domain::term` / `portable-pty` 依存 / `toggle-terminal` ホットキー |
| サムネイル / プレビュー | [0005](./adr/0005-no-media-preview.md) | `fastfiler-domain::thumbnail` / `preview` / `base64` 依存 / `toggle-preview` ホットキー |

ユーザーが追加機能を欲しがった場合の正規ルートは **`commands.json` (外部プロセス起動)** または **Shift+右クリック (`IContextMenu`)** ([ADR 0007](./adr/0007-shell-context-menu-shift-only.md))。
