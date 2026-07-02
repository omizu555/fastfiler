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

> このセクションの版数・根拠は 2026-07-02 実施の iced 実力調査 (§13 リスク台帳の根拠) で確定する。

- **iced は crates.io の通常依存**とする (vendor しない)。GPUI と違い公開クレートとして
  維持されており、fork が必要になった場合のみ `[patch.crates-io]` で git rev 固定に切替。
- バージョンはリサーチ結果を踏まえ、**IME サポートを含む系列**を最優先条件に選定する
  (§7)。API 変動リスクを避けるため、採用したらマイナー版を Cargo.toml でピン留め。
- レンダリング: 既定 (wgpu、失敗時 tiny-skia 系フォールバック) をそのまま使う。
- toolchain: 1.95.0 のままで開始 (iced の MSRV はこれより低い)。GPUI/vendor 撤去後に
  edition/toolchain の拘束を見直す。
- `windows` crate: **workspace 全体で 1 バージョンに統一** (現状 domain 0.58 / gpui 0.61
  の二重化を解消。§10-5)。

---

## 3. 三大リスクの先行検証 (Phase 0 スパイク)

計画全体の GO/NO-GO はこの 3 スパイクで判定する。いずれも「小さな捨てアプリ」で検証し、
結果を本ファイル §15 に記録する。

### S-1: 日本語 IME
- iced の `text_input` で日本語入力 (未確定文字列表示・変換・確定・カーソル位置) が
  機能するかを実機確認。
- 合格基準: インベントリ N-03 (リネーム/新規/パス/検索の 4 用途で使える品質)。
- NG 時の代替: winit の `Ime` イベントを購読する自前入力ウィジェット (現 `text_input.rs`
  と同じ役割を iced の低レベル API で再実装)。それも不可なら計画中止級 (§13 R-1)。

### S-2: 仮想リスト性能
- §6 の自前 `FileList` ウィジェットの原型で、10 万行のダミーデータをスクロール。
- 合格基準: スクロール 60fps 相当・初期表示が体感即時 (System32 実データでも確認)。
- NG 時の代替: 描画のさらなる直接化 (glyph キャッシュ / レイアウト再利用)。

### S-3: OLE D&D の共存
- iced ウィンドウで `drag_and_drop` 無効化 (winit の既定 OLE 登録を止める) →
  `raw-window-handle` で HWND 取得 → domain `ole_dnd::register_drop_target` で
  自前 IDropTarget を登録 → エクスプローラからのドロップ (左/右ボタン、修飾キー) を受信。
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

設計方針 — `FileList`: iced の `Widget` trait を直接実装する单一ウィジェット:

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
- 採用判断は S-1 スパイクで確定:
  - iced 標準 `text_input` が日本語 IME (未確定表示・変換位置) を満たす → それを採用し、
    「F2 で拡張子手前まで初期選択」「Enter/Esc」「ドラッグ部分選択」を上に足す。
  - 不足する場合 → winit `Ime` イベント (`Preedit`/`Commit`) を購読する自前ウィジェット。
    marked_range の描画・カーソル管理は現 text_input.rs の仕様 (cc-rsg 第9章) を踏襲。
- 確定した方式と検証結果 (変換ウィンドウ位置を含む) を §15 に記録する。

---

## 8. Win32 統合の再実装

| 項目 | 方式 | 再利用資産 |
|---|---|---|
| HWND 取得 | `raw-window-handle` (iced/winit も `HasWindowHandle` 実装) | 現 `hwnd_of` と同じ手法 (pane.rs:3791 の先例) |
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
  メニュー木 / セッション serde 往復 — すべて純関数なので单体テストを書く。
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
| シェル New メニュー / プロパティ | domain `show_properties` は実装済み。メニュー木 (core/menu.rs) に項目を足すだけで済む構造に |
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

> 致命度・根拠 URL は 2026-07-02 の iced 実力調査で確定させ、ここを更新する。

| ID | リスク | 影響 | 対策 / フォールバック |
|---|---|---|---|
| R-1 | 日本語 IME が iced で成立しない | 致命 (リネーム等が使えない) | S-1 で最初に検証。自前ウィジェット (winit Ime) → それでも不可なら**計画中止・GPUI 継続** |
| R-2 | 仮想リストの性能不足 | 中核価値 A の毀損 | S-2 で検証。直描き設計 (§6) で回避可能見込み |
| R-3 | winit の OLE 登録と自前 IDropTarget の衝突 | D&D 縮退 | drag_and_drop 無効化 → 自前登録 (S-3)。最悪 ADR 0011 サブクラス再導入 |
| R-4 | iced の API 変動 (メジャー間の breaking change) | 追従コスト | 採用版をピン留め。core 分離により被弾面は薄い皮のみ |
| R-5 | オーバーレイ (メニュー/モーダル) の z 順・クリック外し実装が煩雑 | 工数増 | view 最上段 stack で自前合成に統一 (GPUI 版も自前だった) |
| R-6 | cosmic-text の日本語フォールバック・フォント選択 | 表示品質 | Phase 1 で System32 + 日本語ファイル名を確認。フォント設定 (F-902) で回避余地 |
| R-7 | wgpu 初期化が遅い/相性問題 | 起動時間・環境依存 | ソフトウェアレンダラへのフォールバック確認 (B-4) |
| R-8 | 移行の長期化で main と乖離 | リベースコスト | GPUI 版は凍結 (バグ修正のみ)。ドキュメント/domain の変更は両ブランチへ |
| R-9 | 「ついで機能追加」でスコープ膨張 | 完成しない | 非ゴール厳守。凍結リストは Phase 7 後に解凍 |

**撤退基準**: Phase 0 の S-1 が代替案込みで不成立の場合、本計画を中止し ADR に記録する
(GPUI 版が現役のため実害なし)。

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

### 2026-07-02 — 計画策定
- 現行機能・UI 棚卸し (インベントリ) を作成。正典の層構造を確定。
- コード構造マップ + GPUI 依存面の全量調査 (再利用資産 / 書き直し対象 / 整理課題トップ 10)。
- iced 実力調査 (IME / 仮想リスト / OLE 共存 / 事例) を実施 → §2 / §7 / §13 に反映。
- ブランチ `iced-rewrite` 作成。本計画書を起草。
