# FastFiler アーキテクチャ (GPUI 版)

最終更新: 2026-06-04 (floem → GPUI 全面移植後。経緯は [ADR 0012](./adr/0012-migrate-floem-to-gpui.md))

---

## 1. クレート構成 (Cargo workspace)

```
fastfiler/
├── Cargo.toml                  workspace 定義 ([patch.crates-io] async-task 含む)
├── rust-toolchain.toml         1.95.0 (GPUI 要求)
├── .cargo/config.toml          check-revoke=false (社内網などの失効チェック回避)
├── crates/
│   ├── fastfiler-domain/       OS / GUI 非依存のロジック (lib) — floem 時代から無改造
│   │   └── src/                fs / file_ops / file_jobs / watcher / search /
│   │                           shell / icons / win_clipboard / ole_dnd / undo ほか
│   └── fastfiler-gpui/         GPUI GUI (bin)
│       └── src/
│           ├── main.rs         エントリ。キーバインド登録 / セッション復元 / 窓生成
│           ├── app.rs          FastFilerApp (ルート Entity)。タブ / BSP ツリー /
│           │                   リサイズ / セッション保存 / ツリー連携
│           ├── pane.rs         PaneView (1 ペイン)。一覧 / 選択 / 操作 / モーダル /
│           │                   右クリックメニュー / D&D / watcher 連携
│           ├── tree.rs         ワークスペースツリー (ドライブ起点 / 遅延展開)
│           ├── text_input.rs   IME 対応の単一行テキスト入力 (gpui 公式例の移植)
│           ├── sink.rs         EventSink → async-channel ブリッジ
│           └── session.rs      セッション永続化 (JSON)
└── vendor/                     GPUI と依存 18 クレート (zed から完全移植・自己完結)
    └── README.md               取り込み元コミット / 改変点 / 再 vendor 手順
```

- `vendor/` は**独立サブワークスペース** (main workspace から exclude)。
  zed ルートの `[workspace.*]` をミラーし、`workspace = true` 継承を自前で解決する。
- zed フォルダや zed-industries の git リポジトリへの参照は **ゼロ**
  (唯一の git 依存は smol-rs/async-task の patch)。

---

## 2. 状態モデル

```
Entity<FastFilerApp>                    ルート
├── tabs: Vec<TabState>
│    └── TabState
│         ├── root: PaneNode            BSP ツリー
│         │    ├── Leaf(Entity<PaneView>)
│         │    └── Split { id, dir, ratios, children }
│         ├── focused: Option<EntityId> フォーカスペイン (青枠 / 操作の宛先)
│         └── subs: HashMap<EntityId, (Subscription, Subscription)>
│                                       ペイン毎の (イベント購読, 変化観測)
├── tree: Entity<TreeView>              ワークスペースツリー
├── active / show_tree / tree_width / window_bounds / pending_focus ...
│
└─ PaneView (1 ペイン = 1 Entity)
    ├── cur_path / entries / row_icons
    ├── cursor / selected(BTreeSet) / anchor   複数選択モデル
    ├── modal / context_menu / job_status
    └── watcher(Arc<WatcherCore>) / sink / jobs(Arc<JobRegistry>)
```

### メモリ・ライフサイクル (本移植の核心)

- **タブ/ペインを閉じる = `Entity<PaneView>` と `Subscription` を drop するだけ。**
  `PaneView::drop` を起点に watcher (notify スレッド) / sink (チャネル送信端) /
  `cx.spawn` 受信ループ / 観測購読が**連鎖解放**される。
- 計測: `pane.rs` の `PANES_ALIVE` (AtomicI64) を new/+1, Drop/-1 し、
  タブバー下部に `live panes: N` を常時表示。開閉でベースラインへ戻ることを
  実機確認できる。floem 版で増殖していたのはこのライフサイクル。

---

## 3. リアクティビティ方針

- 状態更新は **`entity.update(cx, |s, cx| { ...; cx.notify() })` の 1 ルート**。
- 親子間の通知は `cx.observe` (notify 追従) / `cx.subscribe` (型付きイベント)。
  - `PaneView: EventEmitter<PaneEvent>` — Activated / SplitRequested /
    CloseRequested / FocusNextPane / SwitchTab
  - `TreeView: EventEmitter<TreeEvent>` — OpenDir
- **subscribe ハンドラに `Window` は渡らない**。キーボードフォーカス移動が必要な
  処理は `pending_focus` に積み、次の `render` (Window がある) で実行する。
- 大量行は `uniform_list` (可視範囲のみ描画)。一覧・ツリーとも同方式。
- バックグラウンド → UI は `EventSink → async-channel → cx.spawn` (sink.rs)。
  ペイン drop で送信端が落ち、受信ループは自然終了する。
- デバウンスは `cx.spawn` + `background_executor().timer` (watcher reload 150ms /
  セッション保存 800ms)。

---

## 4. domain との境界

`fastfiler-domain` は GUI 非依存 (floem 時代から無改造)。接続点は 2 つだけ:

1. **同期 API 呼び出し**: `fs::list_dir` / `file_ops::*` / `icons::*` /
   `win_clipboard::*` / `shell::*` など。
2. **EventSink** (events.rs): watcher / file_jobs / search が別スレッドから emit する
   `fs-change` / `fs:job:progress` / `fs:job:done` 等を sink.rs が UI へ中継。

---

## 5. 拡張ポイント

| やりたいこと | 触る場所 |
|---|---|
| 新しいファイル操作 | `fastfiler-domain::file_ops` + `pane.rs` (キー/メニュー) |
| 一覧の列・表示変更 | `pane.rs` の `render_row` / 列見出し |
| メニュー項目追加 | `pane.rs` の `MenuAction` + `render_context_menu` |
| ホットキー追加 | `pane.rs::on_key` (生キー) / `text_input.rs::bind_keys` (入力欄) |
| パネル追加 | `app.rs` のレイアウト (タブバー/ツリーと同様に Entity を挿す) |
| 永続化項目追加 | `session.rs` の `SessionData` (serde default で後方互換) |
| GPUI 自体の更新 | `vendor/README.md` の再 vendor 手順 |

---

## 6. 不採用機能

[`adr/`](./adr/) の決定は GPUI 版でも有効 (プラグイン機構 / 内蔵ターミナル /
メディアプレビュー等は持たない)。旧 floem 実装は ADR 0012 により削除済み
(履歴: コミット `wip(floem): メモリ増殖調査の計装を保全` 以前)。
