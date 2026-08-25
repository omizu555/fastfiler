# FastFiler iced 全面移行 + 再設計計画

作成: 2026-07-02
方針: **GUI 層を iced へ全面移植** + **フレームワーク非依存コアの新設** (次の移行に耐える構造へ)
パリティ正典: [2026-07-02-feature-inventory.md](./2026-07-02-feature-inventory.md) (以下「インベントリ」)

---

## 0. なぜこの計画か (背景)

GPUI 版 (ADR 0012) は機能一式が完成し、メモリ健全性も達成済み。それでも移行する理由:

1. **vendor 18 クレートの自己管理コスト**。GPUI は zed 専用に進化し続ける moving target で、
   再 vendor のたびに改変点の追従が要る。iced は crates.io の通常依存で済む。
2. **ライセンス**。vendor の `zlog`/`ztracing` (GPL-3.0) をリンクするため成果物全体が
   GPL-3.0 強制。iced (MIT) へ移れば vendor ごと落とせて、ライセンスを自由に選び直せる。
3. **ツールチェーン拘束**。GPUI 要求で rustc 1.95.0 固定 + edition 2024 強制。
4. **コードの構造疲労**。`pane.rs` 3,883 行 (責務 14 個・フィールド 33 個)、
   `app.rs` 1,896 行 (責務 9 個)。フレームワークを替えるたびに全部書き直しになる
   構造そのものが最大の負債。
5. **エクスプローラへ近づける余地**。ビューモード・列拡張などの縫い目が現構造には無い。

**過去の教訓 (Tauri → floem → GPUI と 3 度の移行)**:

- floem → GPUI で実証済みの勝ち筋: **`fastfiler-domain` (4,794 行) は `EventSink` trait
  境界で GUI 非依存 → 無改造で全面再利用**。今回も同じ。
- floem 敗因はメモリ・ライフサイクル。iced は Elm 型 (単一 Model 所有 + `update`) なので
  scope/Entity のリークは構造的に起きないが、**watcher/検索スレッドの寿命管理**は
  同じ検証 (PANES_ALIVE 相当の計装) を必須とする。
- GPUI がネイティブ提供していた外部 D&D (`ExternalPaths`) と IME 入力は iced では
  自前になる。ただし **`ole_dnd.rs` に floem 時代の OLE 受信実装 (IDropTarget、約 220 行)
  が現存** (GPUI 版では未使用) — 復活させて使う。

そして今回の再設計の核心:

> **4 度目の移行を「最後の全書き直し」にする。**
> 状態・ロジックをフレームワーク非依存の `fastfiler-core` に抽出し、
> iced 層は「描画と入力の変換」だけの薄い皮にする。
> 次に GUI を替えたくなっても、書き直すのは薄い皮だけ。

---

## 1. ゴールと非ゴール

### ゴール
1. **機能パリティ**: インベントリの F-101〜F-1005 / U-01〜U-15 / N-01〜N-05 全項目。
2. **より高速に**: System32 級で GPUI 版と同等以上。10 万件級の合成フォルダで改善
   (§9 の実測ベンチで判定)。
3. **メモリ健全性の維持**: タブ/ペイン開閉でペイン数・スレッド・ハンドル・ヒープが
   ベースラインへ戻る (計装込み、§12 Phase 3 の Exit 条件)。
4. **コード整理**: 現行の構造問題トップ 10 (§10) を新設計で解消。core の純ロジックに
   単体テストを付ける。
5. **拡張余地の予約**: エクスプローラ接近方向の縫い目 (§11) を設計に織り込む
   (実装はしない)。

### 非ゴール
- 新機能の追加 (凍結リスト: ペイン内ツリー / インクリメンタルサーチ / パネルプリセット /
  Spring-loaded / 性能パネル / ASCII ツリー UI は移行対象外のまま)。
- ADR で不採用とした機能の復活 (0001/0003/0004/0005 は iced 版でも有効)。
- `fastfiler-domain` の大改造 (§5.4 の追加的・最小改造のみ)。
- macOS / Linux 対応 (iced はクロスだが検証は Windows のみ。将来余地としては残る)。
- ユーザー設定ファイル (themes/commands/hotkeys) の形式変更。

---

## 2. 依存の取り込み方針

> 版数・根拠は 2026-07-02 実施の iced 実力調査で確定済み (多ソース 3 票検証。§13 に出典)。

- **iced 0.14.0 (2025-12-07 安定版) を採用し、ピン留めする**。0.14 は必須条件:
  - IME 対応 (PR [#2777](https://github.com/iced-rs/iced/pull/2777)) は 0.14 が初出。
    **0.13.x に IME は存在しない**ため選択肢にならない。
  - マルチウィンドウの致命バグ (#2848) も 0.14 で修正済み (0.13 未バックポート)。
  - 調査時点で `[Unreleased]` は空 (0.14.0 以降の破壊的変更なし)。master は 0.15.0-dev。
- **crates.io の通常依存**とする (vendor しない)。fork が必要になった場合のみ
  `[patch.crates-io]` で git rev 固定に切替。
- **iced_aw 0.14.1 (2026-06-04、活発に保守中)** を必要に応じて併用可
  (ContextMenu / DropDown 提供、iced 0.14 対応済み)。ただしメニューは自前オーバーレイ
  (§4) を第一候補とし、iced_aw は部品の参考実装・保険とする。
- レンダリング: 既定 (wgpu、失敗時 tiny-skia 系フォールバック) をそのまま使う。
  性能実測は未検証情報のため Phase 0-1 のベンチで自前確認 (§13 R-7)。
- toolchain: 1.95.0 のままで開始。GPUI/vendor 撤去後に edition/toolchain の拘束を見直す。
- `windows` crate: **workspace 全体で 1 バージョンに統一** (現状 domain 0.58 / gpui 0.61
  の二重化を解消。§10-5)。

---

## 3. 三大リスクの先行検証 (Phase 0 スパイク)

計画全体の GO/NO-GO はこの 3 スパイクで判定する。いずれも「小さな捨てアプリ」で検証し、
結果を本ファイル §15 に記録する。

### S-1: 日本語 IME
- iced 0.14 の `text_input` で日本語入力 (未確定文字列表示・変換・確定・カーソル位置) を
  Windows 11 + MS-IME の実機で確認。iced の IME は **over-the-spot 方式**
  (未確定文字列はランタイムがオーバーレイ描画、`InputMethod::Enabled { cursor, preedit, .. }`
  の cursor 矩形が変換候補ウィンドウ位置になる)。
- **確認必須シナリオ**: 未確定→変換→確定 / 長い未確定文字列 / スクロール中の入力欄 /
  **キーボード言語切替後の変換ポップアップ位置** (未解決 issue
  [#3189](https://github.com/iced-rs/iced/issues/3189): 切替後にポップアップが画面右下へ
  飛ぶ Windows 固有バグが open。再現条件と実害を必ず確認)。
- 合格基準: インベントリ N-03 (リネーム/新規/パス/検索の 4 用途で使える品質)。
  #3189 を踏んだ場合は再現条件が「実用上稀」であることの確認 + 回避策メモで条件付き合格可。
- NG 時の代替: winit の `Ime` イベントを購読する自前入力ウィジェット (現 `text_input.rs`
  と同じ役割を iced の低レベル API で再実装)。それも不可なら計画中止級 (§13 R-1)。

### S-2: 仮想リスト性能
- §6 の自前 `FileList` ウィジェットの原型で、10 万行のダミーデータをスクロール。
- 合格基準: スクロール 60fps 相当・初期表示が体感即時 (System32 実データでも確認)。
- NG 時の代替: 描画のさらなる直接化 (glyph キャッシュ / レイアウト再利用)。

### S-3: OLE D&D の共存
- iced ウィンドウで winit の既定 OLE 登録を止める (winit の
  `WindowAttributesExtWindows::with_drag_and_drop(false)` 相当。iced 0.14 の
  `window::Settings` からの指定可否を確認し、不可なら `RevokeDragDrop` → 再登録の順で検証) →
  HWND 取得 (iced 0.14 は `window::run` タスク [#2718] で native handle にアクセス。
  `iced::window::raw_window_handle` モジュールが re-export されている) →
  domain `ole_dnd::register_drop_target` で自前 IDropTarget を登録 →
  エクスプローラからのドロップ (左/右ボタン、修飾キー) を受信。
- 併せて送信側 (`ole_dnd::start_drag` + `AttachThreadInput` ワーカー) が iced の
  イベントループでも機能することを確認。
- 合格基準: 受信 (効果判定・右ボタン判別 = grfKeyState の MK_RBUTTON) と送信の両立。
- NG 時の代替: ADR 0011 方式 (Win32 サブクラス) の再導入。

---

## 4. アーキテクチャ対応表 (GPUI → iced)

| 現行 (GPUI) | iced 版での置き換え | 備考 |
|---|---|---|
| `Entity<FastFilerApp>` / `Entity<PaneView>` | 単一 `AppModel` + `SlotMap<PaneId, PaneState>` (core) | Entity 参照の代わりに ID。所有は常に AppModel |
| `cx.subscribe` / `cx.observe` / `EventEmitter` | 単一 `Msg` enum + `update()` (Elm) | PaneEvent/TreeEvent は `Msg::Pane(id, ..)` 等へ吸収 |
| `cx.notify()` | 不要 (update 後に必ず view 再評価) | 差分は仮想リスト側で吸収 |
| `EventSink → async-channel → cx.spawn` | `EventSink → async-channel →` **`iced::Subscription`** | sink.rs の考え方をそのまま移す。domain 無改造 |
| `uniform_list` + `UniformListScrollHandle` | **自前 `FileList` ウィジェット** (§6) | 一覧・検索結果・ツリーの 3 箇所を同系で |
| `div()` DSL | `column!` / `row!` / `container` + カスタム Widget | |
| `deferred` / `anchored` (メニュー・モーダル) | オーバーレイ層を **view 最上段の `stack` で自前合成** | メニュー木のロジックは core に置き共通化 (§10-9) |
| `FocusHandle` / `pending_focus` | core の「フォーカスペイン」= 純状態。ウィジェットフォーカスは入力欄のみ | F6 巡回・タブ切替復元は core の update で完結 |
| `actions!` + `bind_keys` + `on_key_down` | `keyboard::on_key_press` 購読 → `hotkeys::lookup` (再利用) → `Msg` | combo 正規化ロジックは core へ移設 |
| `EntityInputHandler` (IME) | iced `text_input` または自前入力ウィジェット (S-1 の結果で確定) | |
| `cx.spawn` / `background_executor` / `.timer` | `Task::perform` / `Subscription` + デバウンスは update 内タイマー管理 | 150ms/800ms/80ms の既定値は踏襲 |
| `cx.on_app_quit` | ウィンドウ close イベント捕捉 → 保存 → 明示 exit | クラッシュ安全保存 (persist.rs) は再利用 |
| GPUI `ExternalPaths` (外部 D&D 受信) | **domain `register_drop_target` 復活** (S-3) | winit 既定のファイルドロップは使わない (右ボタン/効果が取れないため) |
| GPUI `on_drag` (内部 D&D / リサイズ) | マウスイベント + core の drag 状態機械 | iced は右ボタンも素通しなので ADR 0011 サブクラスは不要見込み |
| `cx.read_from_clipboard` (テキスト) | iced `clipboard::read/write` | ファイルは domain `win_clipboard` (CF_HDROP) 続投 |
| `Image`/`img()` (アイコン) | `image::Handle::from_bytes` + ハンドルキャッシュ | PNG 生成 (domain icons.rs) は無改造 |
| `WindowOptions` / bounds 追跡 | `window::Settings` + window イベント購読 | 最大化/位置のセッション復元を再構築 |

---

## 5. 新アーキテクチャ

### 5.1 クレート構成

```
crates/
├ fastfiler-domain/   既存 (最小改造 §5.4)。OS/ファイル操作・シェル統合・OLE
├ fastfiler-core/     ★新設。フレームワーク非依存の状態 + 純ロジック (lib)
│   src/
│     model.rs        AppModel / TabState / PaneState / PaneNode(BSP) / Overlay
│     msg.rs          Msg 階層 (Tab / Pane / Tree / Domain / Window / Settings)
│     update.rs       純 reducer 群: fn update(&mut AppModel, Msg) -> Vec<Effect>
│     effect.rs       Effect enum (SpawnJob / StartDrag / OpenShell / Save / ...)
│     selection.rs    カーソル/複数選択/矩形選択/名前復元
│     history.rs      戻る/進む + ロックタブ規則
│     hotkeys.rs      HotAction 18 種 + combo 正規化 (現 hotkeys.rs 移設)
│     theme_data.rs   37 色キー + プリセット + themes/*.json (色は非依存型で保持)
│     session.rs      SessionData/NodeData スキーマ (現 session.rs 移設)
│     settings.rs     AppSettings (現 settings_store.rs 移設)
│     persist.rs      クラッシュ安全 I/O (現 persist.rs 移設、テスト付き)
│     menu.rs         右クリック/ドロップメニューの木構築 (二重実装を統合 §10-9)
├ fastfiler-win/      ★新設。HWND が要る Win32 統合 (lib, cfg(windows))
│   src/
│     drop_target.rs  OLE 受信の配線 (domain ole_dnd::register_drop_target を使用)
│     single_instance.rs  多重起動防止 (堅牢化 §10 補)
│     window_interop.rs   hwnd 取得ヘルパ (raw-window-handle)
└ fastfiler-iced/     ★新設。iced バイナリ (bin 名 fastfiler)
    src/
      main.rs         起動 / ウィンドウ設定 / セッション復元 / Subscription 配線
      app.rs          iced Program: view() 組み立てと Msg 変換だけ (薄い皮)
      subscriptions.rs  domain チャネル / キーボード / window イベント
      effects.rs      Effect -> iced Task 変換
      widgets/
        file_list.rs  ★仮想リスト (§6)。一覧・検索結果・ツリー共用の基盤
        text_input.rs S-1 の結果次第 (標準 or 自前)
        overlay.rs    メニュー/モーダルの合成
      views/
        tab_bar.rs / pane.rs / tree.rs / settings.rs / footer.rs / search.rs
      theme_bridge.rs core の色データ -> iced::Color / スタイル変換
```

分割の原則: **「純状態・純ロジック → core」「HWND が要る → win」「描画と入力変換 → iced」**。
迷ったら core に置く (テストが書ける側に倒す)。

### 5.2 状態モデル

```
AppModel (単一所有・単一 update)
├ tabs: Vec<TabState>            並び順 = 表示順
│   TabState { root: PaneNode, focused: Option<PaneId>, locked: bool, name_src: PaneId }
│   PaneNode = Leaf(PaneId) | Split { dir, ratios, children }   ← メソッド化 (§10-6)
├ panes: SlotMap<PaneId, PaneState>
│   PaneState { cur_path, entries, sort, cols, cursor, selected, anchor,
│               history, overlay: Option<Overlay>, job: Option<JobStatus> }
│   Overlay = Modal(..) | ContextMenu(..) | Search(..) | PathEdit(..)
│           | DropMenu(..) | ConflictDialog(..)      ← 8 個の Option を 1 enum に (§10-3)
├ tree: TreeState (ノード・展開・UNC share・追従)
├ drag: Option<DragState>        内部 D&D / ラバーバンド / リサイズの状態機械
├ focus: フォーカスペイン (タブ毎に復元)
├ settings / theme / hotkeys / window_bounds
└ debounce: 保存 800ms / watcher 150ms のタイマー台帳
```

- **メモリ健全性の構造保証**: ペインを閉じる = `panes.remove(id)` + 付随リソース表
  (watcher / sink 送信端 / アイコンキャッシュ) からの除去。所有が 1 か所なので
  リーク経路が存在しない。`PANES_ALIVE` 相当のカウンタは iced 版でも計装し、
  Phase 3 の Exit 条件で実測確認する (ADR 0012 の教訓)。
- **watcher の寿命**: ペイン毎の `WatcherCore` を `PaneState` 保持ではなく
  「PaneId → watcher」のリソース表で持ち、remove 時に確実に `unwatch`。
  sink はペイン単位ではなくアプリ単一チャネルに集約し、イベントに PaneId を載せる。

### 5.3 メッセージと副作用

```rust
enum Msg {
    Tab(TabMsg), Pane(PaneId, PaneMsg), Tree(TreeMsg),
    Domain(DomainEvent),          // 型付き (§10-8)
    Key(HotAction), RawKey(..), Mouse(..),
    Window(WindowMsg), Settings(SettingsMsg), Tick(TimerId),
}
enum Effect {
    SpawnCopyJob{..}, SpawnMoveJob{..}, StartOleDrag{..}, ShowShellMenu{..},
    OpenPath{..}, SaveSession, SaveSettings, LoadIcons{pane, keys},
    RunUserCommand{..}, StartSearch{..}, ...
}
fn update(m: &mut AppModel, msg: Msg) -> Vec<Effect>   // 純ロジック・単体テスト対象
```

- `update` は I/O を一切しない。`Effect` を返し、iced 層 (`effects.rs`) が
  `Task::perform` / スレッド起動 / domain 呼び出しに変換する。
- これにより「ロックタブは移動を新タブへ逃がす」「衝突ダイアログの一括適用」
  「Undo 記録条件」のような複雑な規則が**全て単体テストできる**。
- domain イベントは `ChannelSink` (現 sink.rs と同じ) → アプリ単一の
  `Subscription::run` で受信 → `Msg::Domain` に変換。
  `serde_json::Value` の手掘り分岐は core 側の `DomainEvent` パーサに一元化。

### 5.4 fastfiler-domain の最小改造 (追加のみ / 互換維持)

1. `windows` crate を workspace 統一バージョンへ (コード修正は API 差分のみ)。
2. `file_jobs::run_job` の panic 時に `JobRegistry` から unregister されない問題
   (cc-rsg Q-022) を guard で修正。
3. (任意・ホットパスで必要になった場合のみ) 型付きイベントの enum を domain 側に追加
   (`emit_json` と併存、後方互換)。
4. `ole_dnd` 受信側 (`register_drop_target`) は**正式採用に格上げ** (現状デッドコード)。
   ドキュメントコメントに iced 版での役割を明記。

---

## 6. 仮想リスト設計 (最重要ウィジェット)

GPUI `uniform_list` の代替であり、性能目標 N-01 の要。**iced の既製ウィジェットの
組み合わせでは作らない** (数万行で widget ツリー構築がボトルネック化するため)。

> 裏付け (2026-07-02 調査): iced 0.14.0 に lazy/virtual リストは**存在しない**。
> 新設の table/grid/sensor/smart scrollbars も仮想化ではなく、column/row の
> Primitive culling (#2611) は描画カリングのみでレイアウトとツリー構築は O(n)。
> コミュニティでは `scrollable` が約 1,000 件で遅延するとの報告 (discourse)。
> **仮想リストの自作が本移植の工数の中心**であることは調査でも確定した。

設計方針 — `FileList`: iced の `Widget` trait を直接実装する単一ウィジェット:

- **固定行高** (フォントサイズから算出、行高追従は F-902)。総高 = 行数 × 行高で
  スクロールバー領域を確定し、**可視範囲の行だけを `draw` で直接描画**
  (テキスト + アイコン + 選択帯 + カーソル枠)。行を子ウィジェットにしない。
- ヒットテストは座標→行 index の算術。クリック/ダブルクリック/右クリック/
  ドラッグ開始 (閾値超) / ラバーバンド / ホイールを **メッセージとして放出**し、
  解釈 (選択・活性化・D&D 開始) は core の update に任せる。
- `scroll_to(ix, Top|Center)` 相当の制御 (検索ジャンプ・ツリー追従・キーボードナビ)。
- 列見出し・列幅ドラッグは同ウィジェットのヘッダ領域として実装 (40〜400px、名前列吸収)。
- テキスト整形 (`…` 省略・`YYYY/MM/DD HH:MM`) は描画時に幅から算出、
  シェイピング結果を行キャッシュ (スクロール中の再シェイピングを避ける)。
- **ツリーへの流用**: 行モデルを「インデント + 展開矢印 + アイコン + ラベル」に
  差し替えるだけで `TreeView` 相当になる (uniform_list を共用していた現行と同型)。
- アイコンは `LoadIcons` Effect で可視範囲を優先非同期ロードし、届くまで既定アイコン。
  iced `image::Handle` を拡張子/フォルダ単位で共有 (テクスチャ二重キャッシュを回避)。

---

## 7. テキスト入力と IME

- 現 `text_input.rs` (680 行) は GPUI 固有実装のため全面作り直し。
  **1 実装を 4 用途 (パスバー / リネーム / 新規 / 検索) で共有する構造は維持**する。
- iced 0.14 の IME の実力 (2026-07-02 調査で 3 票検証済み):
  - PR #2777 (2025-02 マージ) で CJK 対応が `text_input`/`text_editor` に統合され、
    0.14.0 に同梱 (公式 FAQ も「IME は 0.14 から対応」と明言)。
  - 未確定文字列は **over-the-spot** (オーバーレイ描画)。インライン (on-the-spot) は未実装。
    変換候補ウィンドウはキャレット位置に追従 (初期位置バグは #2793 で修正済み)。
  - preedit の可変サイズ (#2790)・スクロール時の位置ずれ (#2798) も 0.14.0 で修正済み。
  - **残存リスク**: メンテナ自身が「Far from perfect」と明言・エッジケース未対応 (FAQ)。
    Windows 固有の open バグ #3189 (言語切替後の候補ウィンドウ位置)。
- 採用判断は S-1 スパイクで確定:
  - iced 標準 `text_input` が実機品質を満たす → それを採用し、「F2 で拡張子手前まで
    初期選択」「Enter/Esc」「ドラッグ部分選択」を上に足す。
  - 不足する場合 → winit `Ime` イベント (`Preedit`/`Commit`) を購読する自前ウィジェット。
    marked_range の描画・カーソル管理は現 text_input.rs の仕様 (doc/spec 第9章) を踏襲。
- 確定した方式と検証結果 (変換ウィンドウ位置を含む) を §15 に記録する。

---

## 8. Win32 統合の再実装

| 項目 | 方式 | 再利用資産 |
|---|---|---|
| HWND 取得 | iced 0.14 の `window::run` タスク (#2718。native handle へのクロージャアクセス。ハンドルは非 Clone・短命なので **HWND を isize で取り出して保持**する) + `iced::window::raw_window_handle` re-export | 現 `hwnd_of` と同じ「Win32 ハンドルを isize 化して domain へ渡す」流儀 (pane.rs:3791 の先例) |
| OLE 初期化 | 起動時 `init_ole()` (UI スレッド) | main.rs:43 と同じ |
| 外部 D&D 受信 | winit 既定のドロップ登録を無効化 → `register_drop_target(hwnd, callbacks)` | domain ole_dnd.rs:629-848 (復活) |
| 右ボタン外部受信 | IDropTarget の `grfKeyState` (MK_RBUTTON) で判別 → ドロップメニュー | 同上 |
| 外部 D&D 送信 | `start_drag` を専用スレッド + `AttachThreadInput` | pane.rs:2341-2388 の手順を移植 |
| 内部 D&D (右ボタン含む) | iced はマウスボタンを素通しするため**自前状態機械で完結** (ADR 0011 のサブクラス不要見込み → S-3 で確認) | core `DragState` |
| シェルメニュー (Shift+右クリック) | `show_shell_context_menu(hwnd, paths)` | domain shell.rs (無改造) |
| クリップボード (ファイル) | CF_HDROP + Preferred DropEffect | domain win_clipboard (無改造) |
| 多重起動防止 | Named Mutex は続投。既存窓の発見を `FindWindowW(タイトル)` から堅牢な方式 (専用ウィンドウクラス名 or 名前付きイベント + HWND 共有) へ改善 (cc-rsg Q-073) | win32_single_instance.rs を fastfiler-win へ移設 |
| ブロッキング回避 | ShellExecuteW / DoDragDrop / プロセス起動は必ず別スレッド (UI 再入禁止は iced でも同じ) | shell.rs の STA ワーカー (無改造) |

---

## 9. 性能戦略 (「より高速に」の中身)

### 改善の狙いどころ
1. **一覧描画の直接化** (§6): 行を widget 化せず draw 直描き。GPUI 版
   (要素ツリー構築あり) より一段軽くできる余地。
2. **大量フォルダの段階表示**: `list_dir` を background Task 化し、
   「名前一覧を即表示 → stat / アイコンを追補」の 2 段階に。System32 級は従来どおり
   一撃、10 万件級で体感改善。キャンセル (連続ナビゲーション) 対応。
3. **アイコンの可視範囲優先ロード** + ハンドル共有 (拡張子キー)。
4. **ソート**: 列キーの事前計算 (小文字化・数値化) で再ソートを O(n log n) の
   比較コスト最小に。
5. **起動時間**: wgpu 初期化と最初の描画までを計測し、セッション復元 (ディスク I/O)
   と並行化。
6. 将来の性能パネル (IDEAS #22) が挿さるよう、計測点 (open→描画 ms / フレーム時間 /
   ジョブ速度) を core の `perf` フックとして予約。

### ベンチマーク手順 (Phase 毎に同一マシンで GPUI 版と比較)
| 計測 | 対象 | 合格 |
|---|---|---|
| B-1 | `C:\Windows\System32` open → 描画完了 | GPUI 版と同等以下 |
| B-2 | 10 万件合成フォルダ open / スクロール | 初期表示 < 500ms・スクロール滑らか |
| B-3 | タブ 50 個開閉 → メモリ/スレッド/ハンドル | ベースライン復帰 (N-02) |
| B-4 | 起動 → 操作可能まで | GPUI 版と同等以下 |
| B-5 | 1GB / 1 万ファイルのコピー (ジョブ) | domain 依存なので同等のはず (劣化なし確認) |

---

## 10. コード整理の方針 (現行の問題 → 新設計での解消)

| # | 現行の問題 | 解消 |
|---|---|---|
| 1 | `pane.rs` 3,883 行・責務 14 個・フィールド 33 個 | core (selection/history/menu/update) + iced (views/pane, widgets/file_list) へ分割。1 ファイル 500 行目安 |
| 2 | `app.rs::render_settings` 476 行 + 設定用一時フィールド 6 個の混入 | `views/settings.rs` に分離、状態は `Overlay::Settings` へ |
| 3 | 8 個の `Option` オーバーレイ状態と `on_key` の手動優先分岐 | `enum Overlay` に統合。キー処理は「Overlay があれば Overlay へ、なければ一覧へ」の 2 段だけ |
| 4 | `ole_dnd` 受信側 220 行がデッドコード | 正式採用 (S-3 / §8) |
| 5 | `windows` crate 0.58 / 0.61 の二重化 | workspace 統一 (§2) |
| 6 | BSP 操作が app.rs 末尾のフリー関数群 | `impl PaneNode` (core/model.rs) + 単体テスト |
| 7 | `SELF_DROP` / `RIGHT_DRAG` 等のグローバル可変 static | core の `DragState` に明示化 (update 経由でのみ変更) |
| 8 | domain イベントが文字列 + `serde_json::Value` 手掘り | core の `DomainEvent` enum + 一元パーサ (§5.3) |
| 9 | メニュー木ロジックの二重実装 (右クリック用 / ドロップ用) | core/menu.rs に統合し両者から使う |
| 10 | `theme.rs` の色データに GPUI 型が混入 + 再読込ごとの `Box::leak` | 色は core/theme_data.rs (非依存型)。iced 層で変換。leak は撤廃し通常の状態として持つ |

補: 多重起動のタイトル一致依存 (Q-073)、ジョブ panic 時の登録残り (Q-022) も §8 / §5.4 で解消。

### テスト戦略
- **core**: 選択モデル (Ctrl/Shift/矩形/名前復元) / BSP 分割・削除・collapse /
  履歴とロックタブ規則 / 衝突解決の一括適用 / Undo 記録条件 / hotkey 正規化 /
  メニュー木 / セッション serde 往復 — すべて純関数なので単体テストを書く。
- **domain**: 既存テスト (ユニット + 統合 367 行) を維持。Q-022 修正のテスト追加。
- **UI**: 自動化しない。スパイク (S-1〜S-3) + フェーズ Exit 条件 + インベントリの
  パリティチェック (手動) で担保。

---

## 11. エクスプローラ接近の余地 (設計に予約する縫い目 — 実装はしない)

| 縫い目 | 予約の中身 |
|---|---|
| ビューモード | `PaneState.view: ViewMode` (当面 `Details` のみの enum)。`FileList` は「行モデル供給者」を差し替え可能に — 将来 一覧/大アイコン/ペイン内ツリーが挿さる |
| 列システム | 列を `Column` enum + 幅 Vec で持つ (名前/更新日時/サイズ/種類固定をやめる)。将来「作成日時」「属性」等の追加が 1 箇所で済む |
| パスバー | `views/pane.rs` 内で独立コンポーネントに切る。将来ブレッドクラム (セグメントクリック) 化の余地 |
| インクリメンタルサーチ (#3) | キー入力の「一覧が受けた印字可能文字」を core が扱う経路を用意 (現状は捨てている) |
| Spring-loaded folder (#17) | `DragState` にホバー滞留タイマーの席を用意 |
| 性能パネル (#22) | §9 の perf フック |
| シェル New メニュー / プロパティ | domain `show_properties` は実装済み。メニュー木 (core/menu.rs) に項目を足すだけで済む構造に → **プロパティは 2026-08-22 に実装済み** (§15)。New メニューは未着手 |
| ごみ箱・特殊フォルダ | 将来シェル名前空間 (PIDL) を扱うなら domain に閉じる方針だけ明記 |

---

## 12. フェーズ計画 (各フェーズで「動くもの」を出す)

### Phase 0 — 足場 + 三大スパイク (GO/NO-GO 関門)
- `fastfiler-iced` / `fastfiler-core` / `fastfiler-win` の空クレート + workspace 配線。
- iced でウィンドウが出る (hello world)。
- **S-1 (IME) / S-2 (仮想リスト) / S-3 (OLE) を消化**し、結果を §15 へ。
- Exit: 3 スパイク全て合格 (代替込み)。不合格が出たら §13 の撤退基準へ。

### Phase 1 — core 骨格 + 単一ペイン一覧
- core: AppModel/Msg/update/Effect の骨格 + 選択モデル + ソート (単体テスト付き)。
- iced: `FileList` 本実装。`list_dir` 段階表示 + アイコン非同期ロード。
- Exit: System32 が瞬時に開き、スクロール・選択・ソート・キーボードナビが動く (B-1/B-2)。

### Phase 2 — 操作・watcher・ジョブ
- 開く/親へ/履歴/パス入力/F5。リネーム・新規 (テキスト入力 = S-1 の方式)。
- コピー/切り取り/貼り付け (CF_HDROP)、削除、進捗 + キャンセル、同名衝突ダイアログ、Undo。
- `Subscription` で domain イベント (fs-change 150ms デバウンス / ジョブ進捗) 接続。
- Exit: インベントリ §2.3 / §2.5 が GPUI 版と同挙動。watcher スレッドがペイン close で止まる。

### Phase 3 — 縦タブ + BSP 分割 + セッション (メモリ関門)
- タブ (追加/切替/閉じ/D&D 並べ替え/ロック/列数/幅) + BSP 分割 + リサイズ + F6。
- セッション保存/復元 (800ms デバウンス + 終了時、persist 経由)。
- Exit: **タブ/ペイン 50 開閉でメモリ・スレッド・ハンドルがベースライン復帰 (B-3 = N-02)**。

### Phase 4 — ワークスペースツリー + 検索
- ツリー (遅延展開 / UNC サーバ・share / 自動追従) — `FileList` 基盤の流用。
- 検索バー (Ctrl+F / Everything + 内蔵フォールバック / 結果ジャンプ)。
- Exit: インベントリ §2.7 / §2.8。

### Phase 5 — D&D 一式 + シェル統合 + ユーザーコマンド
- 内部 D&D (ペイン間 / フォルダ行 / 選択全体 / 右ボタンチューザー)。
- 外部受信 (S-3 の本配線) / 外部送信 / 修飾キー規則 / 安全側削除。
- 右クリックメニュー (サブメニュー 3 階層) / Shift+右クリックシェルメニュー /
  ユーザーコマンド / テンプレート新規。
- Exit: インベントリ §2.6 / F-904〜906。エクスプローラ相互の全経路を実機確認。

### Phase 6 — 設定・テーマ・仕上げ
- 設定画面 / テーマ (37 色キー互換) / スタイル / フォント / ホットキー再読込 /
  Everything ポート / 多重起動 (堅牢化版) / exe アイコン。
- Exit: インベントリ §2.9 / §2.10 + N-05 (既存ユーザーファイルがそのまま読める)。

### Phase 7 — パリティ総検証 + 切替 + 撤去
- インベントリ全項目 (F/U/N) をチェックリスト消化。ベンチ B-1〜B-5 の最終計測。
- 既定バイナリを `fastfiler-iced` へ。`fastfiler-gpui` / `vendor/` / async-task patch 削除。
- ADR 0013 (GPUI → iced 移行) 起草。README / ARCHITECTURE / USAGE / BUILD 更新。
  ライセンス再検討 (GPL 強制の解除可否)。toolchain 拘束の見直し。
- Exit: main へマージ可能な状態。

依存: 0 → 1 → 2 → 3 → {4, 5, 6 は並行可} → 7。
並行作業の運用は §14。**Phase 7 まで GPUI 版は削除しない** (比較基準 + 撤退先)。

---

## 13. リスク台帳

> 致命度は 2026-07-02 の iced 実力調査 (5 系統の並列 Web 調査 → 一次ソース取得 →
> クレーム毎 3 票の反証検証) で確定。判定凡例: **致命** = 不成立なら計画中止級 /
> **回避策あり** = 設計・スパイクで潰せる / **問題なし** = 検証済みで障害でない。

| ID | リスク | 調査判定 | 対策 / フォールバック |
|---|---|---|---|
| R-1 | 日本語 IME が iced で成立しない | **回避策あり・要実機検証**。0.14 で正式対応 ([#2777](https://github.com/iced-rs/iced/pull/2777)、over-the-spot)。修正済み: 候補位置 #2793・スクロール位置 #2798。**open**: Windows 言語切替バグ [#3189](https://github.com/iced-rs/iced/issues/3189)。FAQ 曰くエッジケース残 | S-1 を最初に実施 (シナリオ指定済み §3)。NG なら自前ウィジェット (winit Ime) → それでも不可なら**計画中止・GPUI 継続** |
| R-2 | 仮想リストの性能不足 | **回避策あり (自作前提)**。0.14 に lazy/virtual 無し・culling は描画のみ・scrollable は ~1,000 件で遅延報告 | 自作 `FileList` (§6、直描き設計)。S-2 で 10 万行検証 |
| R-3 | winit の OLE 登録と自前 IDropTarget の衝突 | **回避策あり (未検証)**。HWND 取得は `window::run` (#2718) で可能と確認済み。winit 既定登録との共存 (`with_drag_and_drop(false)` の iced からの指定可否 / RevokeDragDrop) が未検証の open question | S-3 で検証。最悪 ADR 0011 サブクラス再導入 |
| R-4 | iced の API 変動 (0.14 → 0.15-dev) | **問題なし (現時点)**。0.14.0 安定・Unreleased 空。ただし 0.13→0.14 は 15 か月ぶりの大改版だった前例 | 0.14.0 ピン留め。core 分離により被弾面は薄い皮のみ |
| R-5 | オーバーレイ (メニュー/モーダル) の実装煩雑 | **問題なし**。自前 stack 合成が第一候補 (GPUI 版も自前)。保険として iced_aw 0.14.1 (2026-06 保守中) の ContextMenu/DropDown | view 最上段 stack で統一 |
| R-6 | cosmic-text の日本語フォールバック・フォント選択 | **未検証** (調査で 3 票検証を通過したクレームなし) | Phase 1 で System32 + 日本語ファイル名 + フォント設定 (F-902) を実機確認 |
| R-7 | wgpu / tiny-skia の性能・起動時間 | **未検証** (同上) | B-1/B-4 ベンチで自前計測。参考実装として COSMIC Files (libcosmic = iced 系ファイラ) を必要時に読む |
| R-8 | 移行の長期化で main と乖離 | — | GPUI 版は凍結 (バグ修正のみ)。ドキュメント/domain の変更は両ブランチへ |
| R-9 | 「ついで機能追加」でスコープ膨張 | — | 非ゴール厳守。凍結リストは Phase 7 後に解凍 |

**撤退基準**: Phase 0 の S-1 が代替案込みで不成立の場合、本計画を中止し ADR に記録する
(GPUI 版が現役のため実害なし)。

**調査の未回答領域** (計画は上記フォールバックで吸収済み、深掘りは必要時に):
レンダリングバックエンドの実測性能 / cosmic-text 日本語フォールバック品質 /
iced 製ファイラの実運用規模感 / GPUI との定量比較。

---

## 14. 進め方の運用 (複数セッション / Issues / スキル)

- **ブランチ**: `iced-rewrite` を基幹とし、フェーズ作業は `iced-rewrite` から
  トピックブランチ or `git worktree` (並行時) を切って PR/マージで戻す。
- **GitHub Issues**: マイルストーン `iced-rewrite` にフェーズ = Issue で登録済み
  (Exit 条件をチェックボックス化)。着手時に自分をアサインし、複数セッションの
  衝突を防ぐ。発見事項は Issue コメントに残す。
- **プロジェクトスキル**: `.claude/skills/iced-rewrite/` — 作業セッションの入口
  (正典の読み順 / 規約 / 検証手順)。新しいセッションはまずこれを読む。
- **コミット規約**: 現行踏襲 (`feat(iced): …` / `refactor(core): …` / `docs(plan): …`、日本語)。
- **進捗記録**: 本ファイル §15 に日付付きで追記 (gpui-migration 計画と同じ流儀)。
- **ドキュメント義務**: 機能挙動に触れたら USAGE.md、重い決定は ADR、
  仕様疑義はインベントリ (§6 既知の内部論点) を更新。
- **凍結**: `fastfiler-gpui` はバグ修正以外触らない。domain は追加のみ (§5.4)。

---

## 15. 実行ログ (Progress)

### 2026-07-02 — 計画策定 ✅
- 現行機能・UI 棚卸し (インベントリ) を作成。正典の層構造を確定。
- コード構造マップ + GPUI 依存面の全量調査 (再利用資産 / 書き直し対象 / 整理課題トップ 10)。
- cc-rsg 逆生成仕様 16 章を `doc/spec/` としてリポジトリへ取り込み。
- iced 実力調査 (多ソース・クレーム毎 3 票の反証検証、一次ソース裏取り) を実施:
  **iced 0.14.0 (2025-12-07) を採用基盤に確定** (IME 初対応・マルチウィンドウ修正込み)。
  仮想リスト自作の必然性と OLE 共存の未検証点を特定 → §2 / §3 / §6 / §7 / §8 / §13 に反映。
- ブランチ `iced-rewrite` 作成・push。本計画書を起草。
- GitHub マイルストーン `iced-rewrite` + Issue #1〜#8 (Phase 0〜7) を登録。
- プロジェクトスキル `.claude/skills/iced-rewrite/` を作成 (作業セッションの入口)。
- 次の一歩: **Issue #1 (Phase 0 — 足場 + 三大スパイク)**。

### 2026-07-03 — Phase 0: 足場 + 三大スパイク (自動検証パス) ✅
- **足場**: `fastfiler-core` / `fastfiler-win` / `fastfiler-iced` 新設、iced `=0.14.0`
  (features: advanced / tokio / image / advanced-shaping。MSRV 1.88 ≤ 1.95 ✓)。
  初回ビルド一発成功。開発期の bin 名は `fastfiler-iced` (gpui 版 `fastfiler` と
  target/ 内衝突回避。Phase 7 で改名)。
- **ウィンドウ起動**: ✅ `WINDOW_OK frames=152` (2.5 秒 ≒ 60fps、日本語表示込み)。
- **S-2 仮想リスト**: ✅ **合格**。自前 `VirtualList` ウィジェット (可視範囲のみ直描き) で
  10 万行、毎フレーム 1,237px ジャンプ (全行入れ替え worst case) を 600 フレーム計測:
  `avg 16.67ms / p50 16.66 / p95 18.15 / max 22.2 / 60fps` (release)。
  行生成 8.8ms、起動→初回フレーム 580ms。§6 の設計 (widget ツリーを作らない直描き) を実証。
- **S-3 OLE 共存**: ✅ **自動検証部分パス**。
  `window::Settings.platform_specific.drag_and_drop = false` は iced 0.14 に存在。
  `OleInitialize` (UI スレッド、run 前) → `OLE_AVAILABLE true` →
  `window::raw_id` で HWND → update() 内で `register_drop_target` → **`OLE_REGISTER_OK`**。
  domain 側は winit 残留登録を `RevokeDragDrop` してから登録する設計 (ole_dnd.rs:832) で
  二重の保険。**実ドロップ (左/右/修飾キー) は手動確認待ち** (手順は Issue #1)。
- **S-1 IME**: スパイク実装・起動確認済み (`IME_SPIKE_BUILT_OK`)。
  **品質判定は実機タイピングの手動確認待ち** (手順は Issue #1。iced#3189 の言語切替
  シナリオ含む)。0.14 の IME API (`InputMethod`) はコンパイルレベルで存在確認。
- **実装知見**: iced 0.14 は `application(boot, update, view)` 形式 / `update()` は
  UI スレッド実行なので COM 登録がそこで安全に行える / `DropTargetRegistration` (!Send)
  は thread_local 保持 / `window::frames()` は Frame メッセージ→update→再描画の
  自走ループになる (計測・自動スクロールに好適)。
- 判定: **GO** (機械検証はすべて合格。S-1/S-3 の手動確認 2 件は Phase 1 と並行で実施可、
  NG が出た場合のフォールバックは §3/§13 のまま有効)。
- **セルフレビュー (8 観点・検証付き) → 8 件検出・全件修正済み**。主なもの:
  - HWND 取得を `window::raw_id` (winit 内部表現依存) から
    **`window::run` + `HasWindowHandle` の正規経路へ変更** (§8 の表どおり)。
  - drop_target: OLE 未初期化ガード / 同一・再利用 HWND の登録置換 / `revoke(hwnd)` API
    (ウィンドウ close 時に必須) / 希望 effect は allowed マスク内から選ぶ規約を明文化。
  - `fastfiler-core` のライセンスを **MIT OR Apache-2.0** に変更 (GPL コードを一切
    リンクしない「次の移行を生き残る」クレートに GPL を宣言する理由がない。§0-2 と整合)。
  - fastfiler-iced を bin+lib 化 (examples が部品を共有可能に。Phase 1 の widgets/ 置き場)。
- 判明事項: `windows` crate が **3 バージョン並存** (domain+win 0.58 / gpui 0.61 /
  winit 経由 0.62)。§10-5 の統一作業は「0.62 は winit 支配下で動かせない」前提で
  スコープすること。root の async-task patch は gpui 専用 — Phase 7 撤去時に必ず削除
  (Cargo.toml にコメント追記済み)。

### 2026-07-03 — Phase 0 手動確認完了 → Issue #1 close ✅ (三大スパイク全合格)
- **S-1 IME**: 実機合格 (日本語入力・変換・確定 OK。iced#3189 は再現せず)。
  フォント品質は要改善だが後回し (ユーザー合意) → Phase 6 F-902 で対応 (Issue #7)。
- **S-3 実ドロップ**: 実機合格。左/右/Ctrl/Shift/Ctrl+Shift の全経路で受信・効果判定 OK。
- **Phase 5 への重要知見**: `IDropTarget::Drop` の grfKeyState に **MK_RBUTTON は
  含まれない** (ドロップ時点でボタン解放済み。実測: ENTER keys=0x02 → DROP keys=0x00)。
  右ボタン判別は **DragEnter/DragOver でラッチ**して Drop で参照する (Issue #6 に転記)。

### 2026-07-03 — Phase 1: core 骨格 + FileList 本実装 (Issue #2) ✅
- **core**: Entry (表示テキスト前計算) / 選択モデル (クリック/Ctrl/Shift/キーナビ/名前復元) /
  ソート (dir 先頭、GPUI 同一規則) / update_pane (純 reducer、世代キャンセル、
  親戻り時のカーソル復元) / format (human_size・日時・種類 — GPUI パリティ)。
  **単体テスト 23 本**。domain 非依存 (MIT ライセンスの根拠を維持)。
- **iced**: `FileList` 本実装 (ヘッダ + ソート表示 + 列幅ドラッグ (右端逆算・GPUI 同式) +
  仮想描画 + 選択/カーソル/縞 + アイコン + ダブルクリック + スクロールバー)。
  app.rs (薄い皮: Msg 変換と Effect 実行のみ) / effects.rs (spawn_blocking で
  list_dir + 拡張子単位アイコン取得)。
- **ベンチ (release)**: B-1 System32 (4,890 件) open 677ms / paint 678ms
  — 起動 (wgpu ~580ms) 込みなので **増分 ≈ 100ms**。
  B-2 合成 10 万件 open 618ms / paint 638ms — **増分 ≈ 60ms**。いずれも Exit 条件内。
- **セルフレビュー (2 観点並列) → 10 件検出・全件修正**:
  - 正確性: ダブルクリック判定に一覧の世代を含める (フォルダ移動直後の誤爆防止) /
    修飾キーを Focused/Unfocused でリセット (Alt+Tab 復帰後の stale Ctrl) /
    Ctrl+A の CapsLock 対応 (eq_ignore_ascii_case) / 狭ウィンドウで名前列の最小幅を確保
    (負幅クリップ・ヘッダ誤判定の防止)
  - **GPUI パリティ齟齬 4 件を是正**: PageUp/Dn は固定 ±10 行 / カーソル未設定は
    -1 行扱い (End=最終行) / Esc・空白クリックはカーソルも解除 / Ctrl+A は anchor=0。
    core テスト 24 本に拡充。
  - 効率: Effect なしメッセージでのアイコンキー集合複製を排除。

### 2026-07-04 — Phase 2: 操作・watcher・ジョブ (Issue #3)
- **2a (c873f7b)**: 履歴 (戻る/進む、マウス第4·5ボタン、Alt+←→、←→↑ ボタン) / F5 /
  パスバー直接入力 (Overlay::PathEdit) / **watcher 自動更新** (ChannelSink → 静的チャネル →
  `Subscription::run`、150ms デバウンス、選択・スクロール維持 reload) /
  DomainEvent 型付き一元パーサ (§10-8)。実機で自動反映を確認 (実行中の追加 → rows 1→3)。
  boot 時に watcher が開始されないバグを実機検証で発見・修正。
- **2b (9ee18b8)**: Ctrl+C/X/V (CF_HDROP) / コピー・移動ジョブ (専用スレッド + JobRegistry
  キャンセル + 進捗フッタ + Esc 優先キャンセル) / Delete ごみ箱 / F2 (拡張子手前選択
  `operation::select_range`) / F7/F8 / 同名衝突ダイアログ (core/transfer に計画・一括解決・
  連番別名を純ロジック実装) / Undo (domain UndoManager 再利用、移動は JobDone 全件成功時
  のみ記録 — ADR 0006/0008)。core テスト 43 本。
- 実装メモ: Undo の記録条件は app.rs (iced 層) に置いた — UndoManager を App が所有する
  ため。純度は §5.3 の原則から半歩譲歩 (条件自体は 3 行)。Phase 3 の AppModel 導入時に
  core へ引き上げるか再判断。

### 2026-07-04 — Phase 3: 縦タブ + BSP 分割 + セッション (Issue #4) — メモリ関門合格 ✅
- **core (66 テスト)**: bsp.rs (n-ary 分割/昇格/リサイズ) / AppModel (SlotMap 単一所有 +
  PANES_ALIVE) / update_app (AppMsg/TabMsg、ロックタブの新タブ逃がし) /
  session.rs (GPUI スキーマ互換 + 往復テスト) / persist.rs ポート。
- **iced**: TabBar・DragHandle ウィジェット / BSP 再帰描画 / ペイン毎ヘッダ・フッタ・
  青枠 / watcher のペイン毎資源表 / 800ms デバウンス保存 + 終了時保存 +
  ウィンドウ位置・最大化復元。**gpui_session.json からの自動移行を実機確認** (26 ペイン)。
- **B-3 (N-02) 実測合格**: STRESS=50 (実 watcher 込み) で panes 1→1 / watchers 1→1 /
  Threads 58→58 / Handles 690→690 / WS +0.8MB。**完全ベースライン復帰**。

### 2026-07-04 — Phase 4: ワークスペースツリー + 検索 (Issue #5) ✅
- **core (69 テスト)**: tree.rs (遅延展開 / UNC 自動登録・グルーピング・解除 / reveal 追従) /
  SearchUi (Ctrl+F バー・結果リスト・job_id 突き合わせ) / NavigateTo (ロック規則適用) /
  セッションに show_tree・tree_width・unc_shares。
- **iced**: TreeList widget / 検索バー UI / entries_override / LoadDrives・
  LoadTreeChildren・start_search (Everything→内蔵フォールバック = domain 再利用)。
- **セルフレビュー 5 件修正**: 検索結果中の Ctrl+C/Delete が無関係ファイルを対象にする
  破壊的バグ (visible_len 統一 + 操作禁止) / 検索連打の旧結果混入 (job_id ガード) /
  UNC reveal のサーバノード展開 / 遅延ロード後の追従再計算 / ツリー幅とリサイズ感度。

### 2026-07-04 — Phase 5a+5b: メニュー / シェル統合 / 内部 D&D (Issue #6)
- **5a (132eb51)**: core/menu.rs にメニュー木を一本化 (§10-9、行/背景/ドロップの 3 種 +
  when 6 種フィルタをテストで検証)。ContextMenu widget (全画面レイヤ直描き、
  画面端フリップ、サブメニュー 3 階層クリック開閉)。Shift+右クリック = シェルメニュー
  (HWND 正規経路 + UI スレッド同期 TrackPopupMenu)。テンプレート新規 / ユーザーコマンド
  (domain 無改造)。
- **5b (c8e07c9)**: AppModel.drag 状態機械 (§10-7 の static 置換)。選択全体を運ぶ /
  フォルダ行ドロップ / 修飾キー規則 (same_volume 実装) / 右ボタンチューザー
  (Overlay::DropMenu + when:"drop")。衝突検出は plan_transfer 経路で全経路共通 (F-503)。
- 残: **5c** — 外部 OLE D&D (受信 register_drop_target 本配線 + ペイン/行ヒットテスト +
  右ボタンラッチ (Phase 0 知見) / 送信 start_drag + ウィンドウ外検出 / 安全側削除 F-606)。

### 2026-07-04 — Phase 5c + セルフレビュー: 外部 OLE D&D 完成 (Issue #6 実装完了)
- **5c (6983096)**: S-3 スパイクの本配線 — init_ole + drag_and_drop=false / 受信は
  ヒットテスト表 (Arc<Mutex<(矩形[物理px], cur_path)>>) で enter/over を即答、drop は
  既存 domain チャネル (ole:drop) 経由 / 送信は CursorLeft → DoDragDrop (UI スレッド
  同期) → MOVE 完了時のみゴミ箱 (F-606) / decide_drop_effect を core 純関数化。
- **レビュー 6 件修正 (525bd61)**: リスト外リリースの drag 残置→誤転送 (グローバル
  Released 監視) / 幽霊ドラッグ / **自己ネスト転送** (dest.starts_with(src) 拒否) /
  異 DPI モニタの追従 / pane_rects 掃除 / 両ボタン同時押し。core 75 テスト。
- 既知の逸脱候補: 外部受信はペイン単位ゾーン (行レベルは内部のみ) — Phase 7 で照合。

### 2026-07-04 — Phase 6: 設定・テーマ・仕上げ (Issue #7 実装完了)
- settings/theme/hotkeys/settings_view を新設 (iced_*.json + gpui_* から自動移行 = N-05)。
  テーマ再読込は Vec 差し替えで Box::leak を解消 (§10 改善)。フォント既定を
  Yu Gothic UI 化 (S-1 で指摘の日本語フォント問題に対応)、サイズは行高に即時追従。
- 多重起動防止は共有メモリ HWND 公開方式に堅牢化 (タイトル一致依存を解消)。
  exe アイコン移植。mutex は iced 専用名 (Phase 7 で統一判断)。
- セルフレビュー 4 件修正: **編集中の Ctrl+Z が実 Undo を発火** (グローバルキー遮断) /
  検証フロー回帰 / ポート欄編集不能 / スライダの過剰ディスク書き込み。テスト 79 本。
- 残 (Phase 7 送り): スタイル (角丸) の描画反映、37 色キーの細部照合。

### 2026-07-04 — Phase 7 前半: 設定ボタン修正 / スタイル反映 / 最終ベンチ / ADR 0013
- 「設定」ボタン欠落を修正 (置換パッチ不適用 — ユーザー報告)。⚙ グリフはフォント
  依存で欠けるためテキスト表記に。
- スタイル (モダン 6/4px / シャープ 0px / ソフト 10/6px) を直描き widget の角丸に反映。
- **最終ベンチ (release、フル機能状態)**: B-1 System32 open 609〜671ms /
  B-2 合成 10 万件 592〜639ms — Phase 1 計測と同等 (機能追加による劣化なし)。
  10 万件の増分 ≈ 0〜30ms、wgpu 初期化が支配的。
- ADR 0013 (GPUI → iced 移行) 起草。ユーザー実機確認: Issue #3/#4/#5 の
  手動確認項目が消化され「当初の UI に戻りつつある」と好評価。
- 残: bin 切替 / GPUI・vendor 撤去 / ドキュメント更新 / main マージ (ユーザー承認後)。

### 2026-07-04 — Phase 7 完了: 切替 (ユーザー承認済み)
- D&D 全パターンの実機確認完了 (Issue #10 クローズ)。UI 操作シミュレーション
  テスト 5 本 + core 77 本。
- 省メモリレンダラ (tiny-skia) を既定化 (GPU は設定で選択可)。
- ファイル名継承: session/settings/hotkeys.json (旧 gpui_/iced_ 接頭辞から自動移行
  チェーン。実機で 25 ペイン構成の移行を確認)。
- bin 名を fastfiler へ (exe アイコン継承)。fastfiler-gpui / vendor/ /
  [patch.crates-io] async-task を撤去。
- ライセンスを GPL-3.0 → **MIT OR Apache-2.0** へ (GPL 強制源の vendor 撤去による)。
- README / ARCHITECTURE / BUILD / USAGE / doc README を iced 版に更新。ADR 0013 完了。

### 2026-07-04 — 切替後の仕上げ第 2 弾: 省メモリレンダラの描画品質 (実機フィードバック)
- **実描画のはみ出し**: tiny-skia は fill_text/draw_image の clip 引数を完全な
  マスクとして使わない (カリングのみ)。対策: 全 widget のテキストは
  「概算幅で切り詰め + …」(ellipsize 共通ヘルパ)、部分行は文字/アイコンを
  描かない。幅制限だけだと折り返しが発生して行が重なる点に注意。
- **差分描画の残像**: (1) 操作 Msg 後の背景揺らし → (2) バッファ age で 1 拍
  漏れる → 素数周期 (17) カウンタ化 → (3) Msg なしの内部スクロールで漏れる →
  入力イベント (Wheel/CursorMoved) を Msg 化して theme 再評価を駆動。
  ※ window::frames() 方式は Msg→再描画→frames の自走ループになり
  アイドルで 1 コア消費 (実測 4s/4s) — 使用禁止。
- **pick_list はドロップダウンのスクロールクリップが tiny-skia で効かない**
  (iced 0.14.0 標準部品側) → フォント選択を「絞り込み入力 + 候補ボタン (max 8)」
  方式へ置換して回避。
- ほか: 検索「不動作」はユーザーの hotkeys.json カスタム (search: ctrl+k) が
  正しく移行された結果と実証 (ヘッドレステスト)。不正 combo は既定へ
  フォールバックする救済を追加。UI テスト 6 本。

### 2026-07-05 — 切替後の仕上げ第 3 弾: 実機フィードバック 3 件
- **複数選択の左ドラッグ移動が不能** (押下の瞬間に単一選択へ潰れる):
  RowPressed で「修飾なし + 選択済み行」なら選択を維持し (`pending_click`)、
  ドラッグに至らなかった場合のみ新設 RowReleased で単一選択へ確定する
  (エクスプローラ準拠。右ドラッグの RightPressed と同じ発想を左に展開)。
- **パス直接入力で存在しないパスを確定すると偽パス状態が残る**
  (一覧は旧フォルダのまま cur_path だけ偽 → ダブルクリックで偽パスが伸びる):
  PaneState に `loaded_path` (表示中一覧の実パス) を追加し、LoadFailed 時に
  cur_path をそこへ復帰 + 直前のナビゲーションが積んだ履歴を掃除。
  その場 reload の失敗は従来どおり load_error 表示のみ。
- **アプリフッター (U-14) を廃止**: ⚙ 設定は上部メニューへ移設済みで、
  静的な "FastFiler" ラベルだけが縦 24px を消費していた。ペイン領域を全高へ。
  (外部 D&D ヒットテストは元々全高基準だったため、むしろ整合)。
- core テスト 84 本 (新規 6: 押下保留 3 + LoadFailed 復帰 3)。
- (2026-07-06 追記) レビュー提案を反映: LoadFailed 復帰の clone を 1 回削減 /
  file_list.rs の左 Released 3 分岐 (バンド/列仕切り/行) を 1 arm へ統合。
  挙動不変 (core 84 本 + UI シミュレーション 8 本で確認)。

### 2026-07-06 — 実機報告: パスの二重区切り (D:\AI\comfy\output)
- 原因: domain `list_drives` の letter は "D:\" (末尾 \ 付き) だが、core tree の
  rows() は "C:" 前提で "\" を付け足し "D:\\" を生成 — **ツリーのドライブ行起点**の
  ナビゲーションだけで二重区切りが発生し、join で子孫パス全体へ伝播していた。
- Path の Eq/Ord/Hash は components ベースで区切り重複を吸収するため動作は
  ほぼ正常 (階層も下がれる) — 表示と文字列比較 (watcher 一致判定) だけが狂う
  ので発見が遅れた。
- 修正: (1) 発生源 = tree DrivesLoaded で letter 末尾の \ を除去。
  (2) チョークポイント = normalize_path (components().collect()) を
  PaneState::new と set_path_and_load に敷き、ツリー / 手入力 (連続・末尾区切り、
  スラッシュ) / 保存済みセッション由来の表記ゆれを一括正規化 —
  汚染済みセッションも次回起動で自動浄化される。domain は無改造 (互換凍結)。
- core テスト 87 本 (新規 3: normalize 変種 / DrivesLoaded 正規化 / navigate 正規化)。
- (2026-07-07 追記) レビュー提案を反映: ツリーのドライブ行 (表示名 + ルートパス) を
  DrivesLoaded 時に前計算し rows() の組み立てを一本化 / ツリー行クリック →
  cur_path 正規化の UI 統合テストを追加 (座標はレイアウト式フック
  tree_row0_center_for_test で決定 — 2D 総当たりプローブは 69s かかった上に
  行 0 を外した。UI テスト 9 本)。

### 2026-07-08 — 実機報告: 起動時に「cmd っぽい窓」が一瞬見える
- 切り分け (EnumWindows 10ms ポーリング + 連写スクショ):
  (1) debug exe はコンソールサブシステムのため Windows Terminal
  (CASCADIA_HOSTING_WINDOW_CLASS、画面中央 1249x635) がホストされる — 仕様どおり。
  (2) release exe はコンソール類ゼロだが、**未描画のメインウィンドウが既定サイズ
  976x679・画面中央で可視化 → 数百 ms 後にセッション位置へジャンプ**していた
  (ダークテーマでは黒い矩形 = cmd 窓に見える)。iced/winit は既定で可視生成のため。
- 修正: main.rs `window::Settings { visible: false }` で非表示生成し、
  WindowOpened で move_to / resize / maximize を適用した最後に
  `window::set_mode(id, Mode::Windowed)` で表示 (0.14 は set_mode が可視切替を兼ねる。
  SetMode は set_visible + set_fullscreen(None) のみで maximize は保持 — ソース確認済み)。
- 実機検証 (release): コンソール類の出現ゼロ / メインウィンドウは 52ms で
  **最初から保存位置 (-7,1420 1665x699) に出現**、既定位置経由のジャンプ消滅。
  WINDOW_OK / BENCH マーカーは可視性非依存のため検証ハーネスへの影響なし。
- 教訓 (LESSONS 反映): Win11 のコンソールホストは conhost でなく Windows Terminal /
  `cargo build | tail` はリンク失敗 (実行中 exe のロック) を隠す — PIPESTATUS で見る。

### 2026-07-08 — 実機報告 2 件: フッタ右クリックの無反応 / サブメニューの重なり
- **フッタ右クリックの反応が悪い**: overlay はフォーカスペインのものしか描画されない
  のに、右クリック系 (OpenMenu / RightPressed / ShellMenuRequest) がフォーカス移動の
  対象外だった — 非フォーカスペインでは「メニューは開いているが描画されない」。
  update_app のフォーカス移動 matches! に 3 つを追加 (右ドラッグ DropMenu は
  Dropped 側で focused を移す先例あり)。行/背景の右クリックも同時に直る。
- **右端でサブメニューが親に重なる**: panels() の右端フリップが `x - PANEL_W` で、
  サブメニューでは親の位置 -4px (真上) になる計算ミス。place_panel_x (純関数) に
  切り出し、ルート=カーソルの左 / サブ=親の左隣 (x - 2*PANEL_W + 8)、
  一度左に折り返したら以降の階層も左へ展開し続ける方式に修正 (エクスプローラ準拠。
  完全に右へ出すとウィンドウ内描画のため右端で切れる — 右優先は維持し、
  収まらないときだけ左隣へ)。
- テスト: place_panel_x 3 本 (iced lib) + OpenMenu フォーカス移動 1 本 (core 88 本目)。

### 2026-07-08 — 実機報告: テンプレートのショートカット (.lnk) がテンプレート自体を開く
- 原因: open_with_shell は Office テンプレート拡張子のとき verb=None (既定 verb
  "new" = テンプレートから新規) にするが、.lnk は `_ =>` で verb="open" 明示に
  落ちる。ShellExecuteW は .lnk に明示 verb を渡すとリンク解決後のターゲットにも
  verb が伝播するため、ショートカット経由では "open" (テンプレート自体の編集) に
  なっていた。
- 修正: .lnk は IShellLinkW で自前解決 (win::resolve_shortcut — ターゲット/引数/
  作業フォルダ) し、「ターゲットの拡張子」で verb を選び直して実行 (エクスプローラ
  と同じ流儀)。解決できないリンク (MSI 広告等) は従来どおり .lnk を直接 shell へ。
  verb 選択は shell_verb_for に切り出し。
- テスト: verb 選択表 + 実 .lnk 生成→解決→テンプレート verb 確定のラウンドトリップ
  (domain 20 本)。domain の凍結解除後初の fmt/clippy 適用 (既存 lint 2 件も解消)。
- (同日追記) フォルダを指す .lnk のダブルクリックは Explorer 起動でなく
  **FastFiler の新規タブ**で開くように (実機要望)。effects 層で resolve_shortcut →
  is_dir なら新設 TabMsg::OpenFor(path) を発行 (ロックタブの OpenTabFor と同じ
  open_new_tab 展開)。resolve は UI スレッド (COM 初期化済み) で同期実行。
  core テスト 89 本目 (OpenFor が新規タブ + LoadDir を発行)。USAGE §2 更新。

### 2026-07-09〜10 — 全コードレビュー (133 エージェント) と一括改善の実施
- **レビュー**: ファイル群 9 班 + 横断 4 班で全 1.6 万行を精読 → 抜け漏れ監査 →
  全 119 所見を 1 件ずつ反証検証 (refuted 3 / confirmed 59 / partial 57)。
  統合後 99 項目をレポート化し、ユーザー承認の上で「テスト提案以外」を一括実施。
- **①バグ 10 件**: SearchHit の job_id ガード漏れ (旧検索の混入) / ロックタブ検索の
  pending_cursor_name リーク / junction・フォルダ symlink に入れない / hotkeys を
  UI スレッド同期 shell で開いていた / 背景揺らしが純白テーマで死ぬ /
  row_at の未クランプ offset (file_list・tree_list — 検索開閉・折りたたみでクリック
  行ずれ) / SetData の STGMEDIUM 解放漏れ / shell_assoc の cfg 二重定義 /
  CursorLeft の幽霊ラバーバンド・列リサイズ。
- **②性能・メモリ**: restore_selection を HashMap 化 (全選択 10 万行 reload の
  O(n²) 停止を根絶 — 最重要) / overlay の毎 view deep clone を参照 match 化 +
  ContextMenu 借用化 / Entry を Box<str> 化 + icon_key フィールド廃止 (10 万行で
  常駐 ~10MB 減) / 並べ替えを index 再マップ化 / move の事前スキャン 1 回化 /
  sort_by_cached_key ×3 / 検索キャンセル配線 (Effect::CancelSearch — 4 経路 +
  ペイン close。job_id ガード付き) / DragHover の dedupe。
- **M-3**: ツリー行キャッシュ (変異点で再構築、TreeList は借用 — FileList と同じ流儀。
  セッション復元は set_unc_shares 経由に変更)。
- **③依存**: iced feature を image-without-codecs へ (rav1e 等 55 パッケージ削減:
  Cargo.lock 553→498) / lru 0.16 統一 / once_cell→std LazyLock / workspace.package・
  workspace.dependencies 継承 / fastfiler-win に Win32_Security を明示
  (単体 check が gpu-allocator の偶然の feature 合流に依存していた)。
- **④Win32 統合**: wstr (to_wide_z/from_wide_z — 6 定義 + 約 10 インライン を集約) /
  hdrop (CF_HDROP 構築 3 重複を統合 + HGlobalGuard でエラーパスのリーク解消) /
  CF_HDROP 解析を DragQueryFileW 化 (境界チェックなし生ポインタ走査 ~60 行を削除) /
  GetData の二重コピー解消 / DragOver キャッシュ Arc 化 / win_com (spawn_sta/with_sta —
  STA 定型 5 箇所と COINIT フラグ不揃いを統一)。
- **⑤整頓**: selected_paths ヘルパ (5 重複) / expand_effects 統合 / decide_op 一本化
  (F-604 の 2 実装) / config_dir 集約 (7→2) / セッション dir の serde enum 化
  (JSON 不変) / unc_parts / update_pane から domain イベント・検索系を関数分離 /
  dead code 削除 (is_focused_pane / _quote_paths / tab_bar の Instant / domain
  path_util::volume_key 110 行) / stale な Tauri 言及 5 ファイル掃除 / ほか小粒多数。
- 見送り (検証により): refresh_ole_snapshot のスキップ (意図的設計) /
  domain_event の serde ミラー化 (削減ほぼゼロ) / L-7 の text_cell・スクロールバー
  完全統一 (意図的差分あり・正味価値薄 — wheel_dy と DRAG_THRESHOLD のみ共有)。
- テスト: 全 153 本 (core 94 / domain 20+lib23 相当 / iced lib 7 / UI 9) 緑。
  新規テスト: 検索ガード・スクロールクランプ・ソート再マップ・CancelSearch・
  wstr/hdrop バイト互換 等。

### 2026-07-11 — 実機報告: パスバークリック直後、一覧のクリックが無反応
- 症状: パスバーをクリックして編集モードに入ると、Enter/Esc で閉じるまで
  ファイル・フォルダ・ツリーのクリックがすべて効かない。
- 原因: update_overlay の PathEdit arm が編集系以外のメッセージを
  `_ => Some(vec![])` で全部握りつぶしていた (§10-3 の 2 段ディスパッチは
  「一覧向けキー操作の横取り防止」が目的だが、マウス操作まで巻き込んでいた)。
- 修正 (エクスプローラのアドレスバー準拠):
  (1) PathEdit 中のマウス由来メッセージ (行クリック/ダブルクリック/空白/バンド/
  右クリック系/ヘッダ/列リサイズ/NavigateTo/↑・戻る・進む/Reload) は編集を破棄して
  None でフォールスルー — **1 クリック目からその操作がそのまま効く**。入力途中の
  値は破棄。キー操作はオーバーレイ表示中 GUI 層で止まる既存設計のため、この区別は
  reducer 側で安全に成立する (マウス由来しか届かない)。
  (2) update_app のフォーカス移動時、移動元ペインの PathEdit も破棄 — 残すと
  編集状態が不可視のまま持ち越され、戻ったとき古い入力が現れる。
- テスト: core 96 本 (新規 2: マウスキャンセル+クリック反映 / 別ペインクリックで
  破棄) + UI 10 本 (新規 1: パスバー実クリック → 行を座標クリック → 1 回で選択。
  overlay 状態フック path_edit_open_for_test 追加)。USAGE §2 更新。

### 2026-07-16 — 機能改善: 新規フォルダ (F7) の複数行一括作成
- 要望: ダイアログを大きくし、メモ帳のノリで複数行を書けるようにして
  1 行 = 1 フォルダで一括作成したい。
- core: ModalCommit の NewFolder を行分割に変更 (parse_folder_names —
  trim・空行無視・重複畳み込み・`\`/`/` を含む行があれば確定拒否)。
  行ごとに Effect::CreateDir を発行 (create_dir_all なので既存名は冪等)。
  pending_cursor_name は先頭行。
- iced: NewFolder ダイアログだけ text_input → **text_editor** (複数行・高さ 160px・
  カード幅 420→560)。text_editor::Content はステートフル型で core に置けないため
  App::modal_editor に保持し、Action ごとに content.text() を ModalInput で core へ
  同期 (source of truth は編集中= Content / 確定検証= core の value)。
  Enter は改行、確定は OK / Ctrl+Enter、Esc はカスタム key_binding で 1 回キャンセル
  (既定 Binding は Esc を Unfocus で capture し keyboard::listen に届かない)。
- 副産物の修正: apply のフォーカス発行条件を「overlay 有無」→「入力オーバーレイ
  (PathEdit/Modal) の種類遷移」に変更 — 右クリックメニュー → 新しいフォルダ等の
  「オーバーレイからオーバーレイ」遷移で入力欄にフォーカスが移らない既存バグを解消。
- コードレビュー (10 観点並列) → 検証 → 修正の追加ラウンド:
  (1) **クロスペイン desync (CONFIRMED)** — Modal はフォーカス移動で破棄されず
  (破棄は PathEdit のみ)、2 ペイン/タブで同時に開くと単一の modal_editor が
  使い回され「表示は B の内容・確定は A の値」になる。修正 = update_app の
  フォーカス移動破棄を Modal にも拡張 (LESSONS 2026-07-11 の不変条件に整合) +
  modal_editor を (PaneId, Content) にしてペインが変われば作り直し (タブ切替・
  OLE ドロップ等フォーカスだけ動く経路の防御)。
  (2) **孤立 \r の行分割不一致 (CONFIRMED)** — cosmic-text は \r 単独も改行表示
  するが str::lines は割らない → split(['\n','\r']) に変更。
  (3) **不正文字の素通り (CONFIRMED)** — 「D:notes」は join で cur_path を
  すり替え別ドライブに作成される。\ / : * ? " < > | + 制御文字を行単位で拒否し、
  フッタへ理由を通知 (無反応だとどの行が悪いか分からない)。
  (4) パスバー編集 A → B の同種遷移でフォーカスが移らない既存バグ — 判定に
  「ペイン変更」を追加。ほか簡潔化 (discriminant 化・タイトル重複解消・
  テストのキーイベントヘルパー抽出)。
- テスト: core 98 本 (新規 2: 複数行/空行/重複/不正文字 4 種/CR 分割/単一行回帰、
  別ペインクリックでモーダル破棄) + UI 11 本 (新規 1: 開く → 貼り付け → OK で
  閉じる / 不正行は閉じない + 通知 / Enter は改行 / エディタ内 Esc が 1 回で
  ModalCancel)。実機 e2e (SendKeys): 「alpha\nbeta\n空行\ngamma」→ 3 フォルダ
  生成・空行無視、「a:b」→ 拒否 + フッタ通知 + モーダル維持を確認。USAGE §2 更新。

### 2026-07-17 — F7 複数行化の改善提案 4 点を実装 (レビュー指摘の残件)
- **高速化**: エディタの毎キー全文同期 (content.text() + clone) を廃止。
  編集中の全文は Content だけが持ち、core への同期は確定時
  (Msg::ModalEditorCommit = 全文同期 → ModalCommit の 2 段) と
  **破棄前フラッシュ** (sync_modal_editor — タブ切替や OLE ドロップのように
  モーダルを破棄せずフォーカスだけ動く経路で入力が消えないための書き戻し) のみ。
  重複畳み込みは HashSet 化 (O(n²)→O(n)) + 大文字小文字を同一視
  (NTFS 非区別 —「Docs/docs」の黙った半分成功を防ぐ)。
- **簡潔化**: ModalCommit を kind ごとの 3 分岐に一本化し、不到達だった
  commit_modal の NewFolder 腕ごと commit_modal/commit_new_folders を削除
  (kind ごとに検証 + Effect 発行を所有。共通部は single_line_name/clash_message)。
- **設計**: 「NewFolder は複数行」の知識を ModalKind::is_multiline() に集約。
  GUI のエディタ選択・Content 同期・キー割当 (modal_editor_key_binding に改名)
  はすべて述語から導出 — 将来 F8 を複数行化するときは述語を true にするだけ。
- **テスト**: core 100 本 (新規 2: 大小違い重複行の畳み込み / pending 不一致時の
  消費とカーソル不迷子) + UI 12 本 (新規 1: タブ切替でエディタ入力が失われない
  — フラッシュ復元の通し)。実機 e2e: 「Docs\ndocs\nmix1」→ Docs+mix1、
  新確定経路 (Ctrl+Enter) の動作を確認。

### 2026-07-17 — F7 改善第 2 弾 (一括 Effect / 文言 const / 前提固定 / OLE テスト)
- **Effect::CreateDir → CreateDirs (一括)**: N 行の作成を 1 blocking op に
  まとめ、OpDone → 明示 reload を N 回 → 1 回に。部分失敗はモーダルが既に
  閉じて再入力できないため、失敗した名前を列挙して 1 通で通知
  (「「block」を作成できませんでした (…os error 183)」— 実機確認済み)。
- ヒント文言を app::MULTILINE_HINT (pub const) に集約 — ui_smoke の
  テキスト検索と view の二重管理を解消。
- sync_modal_editor のフラッシュ (apply を経由しない直接 update_app) に
  debug_assert で「ModalInput は Effect を返さない」前提を固定。
- UI テスト +1: OLE 右ドロップ (focused を直接差し替える経路) でも
  エディタ入力がフラッシュ → 復元される通し (計 13 本)。ドロップ元は
  表示中フォルダの外にする必要がある (同一フォルダからのドロップは無視)。

### 2026-07-17 — F7 改善第 3 弾 (reload 二重走行の解消 / PartialFailure)
- **明示 reload が係留中の watcher デバウンスを吸収**: reload() で
  reload_seq を進め、OpDone の明示 reload 直後に満了する watcher tick を
  stale 化 — 自分の操作後の listing が 2 回 → 1 回に (F5 も同様)。
  reload 後の新しい FsChange は新 seq で改めてデバウンスされる (テストで固定)。
- **OpOutcome::PartialFailure を新設**: 一括操作の部分失敗を Done (undo 記録 +
  通知) / Failed (reload なし) と区別 — 通知した上で結果も反映する。
  CreateDirs の集計は Vec<(名前, エラー)> 1 本に整理し、blocking closure から
  create_dirs_outcome() として分離 → tempdir で部分失敗を再現する単体テストを
  追加 (core 101 / iced lib 8 / UI 13 本 green)。実機 e2e も再確認。

### 2026-07-17 — F7 改善第 4 弾 (名前検証の統一 / 通知整形の集約)
- **F2/F8 も F7 と同じ名前規則に統一**: 検証を check_name (NameCheck 3 値:
  Empty / Invalid / Ok) に集約し、単一行も `\ / : * ? " < > |` + 制御文字を
  拒否 + フッタ通知。従来は `\` `/` のみで、「a:b」は新規ファイルだと NTFS の
  代替ストリームを静かに作り (実測)、リネームは分かりにくい OS 構文エラーだった。
  実機 e2e: F2 で「a:b」→ 拒否 + 「「a:b.txt」は名前に使えません…」を確認。
  parse_folder_names も同じ check_name を行単位で使う (規則の定義は 1 箇所)。
- **通知整形を OpOutcome へ集約**: status_message() / reloads() を実装し、
  app.rs の OpDone は undo 記録 + 表示 + reload の配線だけに (「エラー: 」
  プレフィックス等の文言知識が variant 側に閉じた)。
- **reload 吸収の通しテスト**: load_gen_for_test フックを追加し、ui_smoke で
  FsChange 係留 → OpDone (明示 reload で gen+1) → 旧 tick が沈黙する通しを固定
  (計 UI 14 本)。SpawnJob 化 (数千行の進捗/キャンセル) は基盤変更が大きく
  現実的な行数では不要のため見送り (必要になったら転送ジョブ基盤を再利用)。

### 2026-08-22 — RDP / Outlook の仮想ファイル貼り付け対応 (実機要望)
- **リモートデスクトップ越しのコピー → 貼り付けが不可だった**: RDP の
  クリップボード転送 (MS-RDPECLIP) はファイルを CF_HDROP (実パス) ではなく
  仮想形式 (FileGroupDescriptorW + FileContents) で渡すため、CF_HDROP のみ
  対応の貼り付けでは「クリップボードにファイルがありません」になっていた。
  Outlook 添付のコピーも同形式 (今回の対応で同時に解決)。
- **domain**: virtual_files.rs を新設 — 記述子は素のクリップボード API で読み
  (can_paste 判定と衝突計画用・OLE 不要)、内容は lindex 指定の GetData が要る
  ため OleGetClipboard (with_ole_clipboard — ジョブ専用スレッドで OLE 初期化)。
  cFileName は外部プロセスが書ける入力なので sanitize_rel_path で検証
  (絶対パス / `..` / `:` / 不正文字 / 末尾 `.`・空白を拒否。単体テスト付き)。
  file_jobs::run_virtual_paste が既存の run_job 基盤 (kind="copy") に載せて
  進捗・キャンセル・フッタ表示をそのまま流用。IStream / HGLOBAL 両 tymed 対応。
- **core**: transfer::plan_virtual_paste / resolve_virtual_conflicts (純ロジック +
  テスト 4 本)。衝突はトップレベル名単位 (フォルダの中身は同じ新名の下へ)。
  PasteVirtualRead → (衝突なし) Effect::PasteVirtual / (衝突) Overlay::VirtualConflict。
  仮想貼り付けは**常にコピー動作** (rdpclip は切り取りの越境削除を通知できない —
  エクスプローラも同じ)。Undo 記録なし (通常コピーと同じ)。
- **iced**: ClipboardRead を CF_HDROP → 仮想の 2 段フォールバックに。衝突
  ダイアログは conflict_card として実パス版と共用 (確定 Msg も同じ)。
- **検証**: rdpclip / Outlook と同じ形式を自前 IDataObject で OleSetClipboard
  する e2e テスト (tests/virtual_paste.rs、クリップボードを書き換えるため
  #[ignore] — 手動で 5/5 green。IStream と HGLOBAL の両分岐を通す)。
  core 107 / domain 45 / iced 22 本 green。

### 2026-08-22 — 右クリックメニュー最下部に「プロパティ」(実機要望)
- ADR 0007 追記 (2026-06-07、GPUI 版) のパリティ回収 — §11 の縫い目
  「メニュー木に項目を足すだけ」の通り、core/menu.rs に MenuAction::Properties を
  追加 (行・背景とも最下部固定)。行の上はその項目、背景は現在フォルダ
  (背景では p.cursor へフォールバックしない — エクスプローラ準拠)。
- 実行は既存の domain shell::show_properties_async (SHObjectProperties を専用
  STA スレッドで投げっぱなし — ダイアログの寿命問題を回避) を Effect::ShowProperties
  で配線。core テスト +1 (行/背景の対象パス)。core 153 / iced 22 本 green。

### 2026-08-22 — セルフレビュー提案の回収 (can_paste 軽量化 / クリップボード定型の集約)
- **can_paste 判定を IsClipboardFormatAvailable に変更**: メニュー展開のたびに
  CF_HDROP を全読取 (GlobalLock + DragQueryFileW でパス列挙) していたのを、
  形式の有無チェックだけに (win_clipboard::clipboard_has_files 新設)。
- **「OpenClipboard → f → CloseClipboard」定型を with_clipboard に集約**:
  win_clipboard 3 箇所 + virtual_files の記述子読取の計 4 箇所が共用。
  クリップボード e2e (virtual_paste) で回帰なしを確認。

### 2026-08-26 — F7 の多階層フォルダ作成 (実機要望)
- `aaa\iii\uuu` 形式の 1 行で階層をまとめて作成できるように。区切りは \ と /
  の両方 (\ に正規化)、末尾区切りは無視。作成側は元々 create_dir_all (冪等)
  だったため、変更は検証 (parse) のみ — check_folder_line を新設し、成分ごとに
  「空成分 (絶対パス/連続区切り) / "." ".." (脱出) / : * ? " < > | と制御文字 /
  末尾 . ・空白 (OS が静かに削る)」を事前拒否。F2/F8 の単一名検証 (check_name —
  区切り拒否) はそのまま。
- 重複畳みは正規化後の相対パスで大小・区切り違いも同一視。カーソルは先頭行の
  トップレベル名へ (一覧に現れるのは先頭成分のみ)。ヒント文言と USAGE §2 更新。
- テスト: 階層/正規化/畳み/不正 10 パターン (core) + create_dirs_outcome の
  階層・冪等 (iced tempdir)。core 109 / iced 22 本 green。
