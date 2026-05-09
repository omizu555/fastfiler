# FastFiler アーキテクチャ

最終更新: 2026-05-09 (大規模リファクタ後)

## クレート構成

```
fastfiler (Cargo workspace)
├── crates/fastfiler-domain   ライブラリ: OS 非依存 + Windows 依存ロジック
│   ├── fs              フォルダ列挙 / メタデータ取得
│   ├── file_ops        コピー / 移動 / 削除 / リネーム (SHFileOperationW)
│   ├── icons           Material Symbols アイコン
│   └── examples/       単体検証用バイナリ (例: trash_test)
│
└── crates/fastfiler-native   バイナリ: floem GUI
    └── src/
        ├── main.rs           エントリポイントのみ (~25 行)
        ├── fs_model.rs       純粋関数 + 値型 (FileRow / SortKey / History 等)
        ├── state.rs          シグナル群 (PaneState / Tab / AppState)
        ├── settings.rs       永続化 (RON)
        ├── theme.rs          色定義
        └── ui/
            ├── mod.rs
            ├── app_view.rs   アプリ全体レイアウト
            ├── tabs.rs       タブパネル
            ├── tree.rs       フォルダツリー
            ├── pane.rs       1 ペイン (ファイル一覧 + D&D + モーダル)
            ├── footer.rs     ステータスバー
            └── splitter.rs   ドラッグ可能な仕切り
```

## 状態モデル

```
AppState (グローバル)
├── tabs:        Vec<Tab>
├── active:      RwSignal<TabId>
├── settings:    AppSettings (永続化)
├── splitter_drag, …
└── …
   │
   └─ Tab
       ├── id, columns: RwSignal<u32>
       └── columns: Vec<Vec<PaneState>>   (横列 × 縦行)
            │
            └─ PaneState (Clone, 全フィールドが RwSignal/Arc)
                ├── cur_path, path_input, history
                ├── rows, stats, selected, anchor
                ├── modal_kind, modal_input
                ├── status_msg, fs_event_signal, fs_change_tick
                ├── sink (CounterSink)
                └── watcher (Arc<WatcherCore>)
```

`PaneState` が `Clone + 全 RwSignal/Arc` なので、UI 関数の引数として値渡し可能。
borrow / lifetime 問題が発生しない構造。

## クラッシュ耐性

過去に **0xc0000005 (STATUS_ACCESS_VIOLATION)** が頻発したため、以下の対策を入れている:

1. **削除操作**: `IFileOperation` (COM, STA 必須) ではなく `SHFileOperationW` を使用。
   - SEH (catch_unwind で捕捉不可) を回避。
   - `crates/fastfiler-domain/examples/trash_test.rs` で単体検証可能。

2. **クリック処理**: `PaneState::click_row` に範囲外インデックスのガードを追加。
   - sort / reload 直後に古い `bg_idx` が virtual_stack から渡されてもクラッシュしない。
   - Shift 範囲も `hi.min(len-1)` でクランプ。

## 拡張ポイント

| 何をしたいか | どこを触るか |
|---|---|
| 新しいファイル操作 | `fastfiler-domain::file_ops` + `state::AppState` |
| UI レイアウト変更 | `ui/app_view.rs` |
| 新しいビュー | `ui/` 配下に新モジュール追加 → `ui/mod.rs` に登録 |
| 配色変更 | `theme.rs` の色関数を編集 (Light/Dark 切替もここ) |
| 永続化項目追加 | `settings.rs` の `AppSettings` / `PersistedSettings` |
