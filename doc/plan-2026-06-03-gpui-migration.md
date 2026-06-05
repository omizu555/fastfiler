# FastFiler GPUI 移植計画

作成: 2026-06-03
方針決定: **GPUI へ全面移植**（UI 層を作り直す。`fastfiler-domain` は流用）

---

## 0. なぜこの計画か（背景）

現行 FastFiler は floem (wgpu) ベース。動作は一通り完成しているが
**タブ/ペインの開閉でメモリが増え続ける**問題を抱えている。

調査の結論:

- 原因は floem 自体ではなく、**リアクティブ scope / effect / watcher スレッドの
  ライフサイクル管理のリーク**。`dyn_container` 再構築のたびに
  `create_signal_from_channel` が監視スレッドを再 spawn する等
  （`ui/pane.rs:110-170`, `pane.rs:1188-1217`, `core/tree_model.rs:40-54`）。
- これは floem の「手動 scope 管理」モデルと戦い続ける限り再発しやすい。

Zed の GPUI は、この問題を**構造的に解決**している:

| 課題 | GPUI の仕組み | 場所 (zed/) |
|---|---|---|
| 状態の解放漏れ | Entity の参照カウント + リーク検出器 | `crates/gpui/src/app/entity_map.rs` |
| 大量行の描画 | `uniform_list` (可視範囲のみ描画) | `crates/gpui/src/elements/uniform_list.rs` |
| 数万ファイルの保持 | `SumTree` スナップショット (Arc 共有・O(log N)・差分) | `crates/sum_tree/`, `crates/worktree/` |
| watcher 寿命 | background scanner が Task 寿命に紐づく | `crates/worktree/src/worktree.rs` |
| Windows 対応 | wgpu + DirectX のフル実装 | `crates/gpui_windows/` (18 ファイル) |

そして決定的な事実:

> **`fastfiler-domain` (約 4,300 行) は GUI フレームワークに一切依存していない。**
> floem/Tauri/GPUI いずれの型も参照しておらず、`EventSink` trait
> (`crates/fastfiler-domain/src/events.rs:10-21`) を境界に完全に疎結合。

つまり今回の移植は「全面再構築」と言いつつ、実体は
**UI 層 (`fastfiler-native`) だけを GPUI で書き直す**作業であり、
Windows シェル統合・ファイル操作・検索・監視・アイコン取得という
最も泥臭い 4,300 行はそのまま再利用できる。これが本計画の勝ち筋。

---

## 1. ゴールと非ゴール

### ゴール
- UI 層を GPUI 化し、タブ/ペイン開閉でメモリがベースラインへ戻る状態にする。
- 現行 v0.1.0 の中核機能 (縦タブ + BSP 分割ペイン + 速度 + シェル統合) を再現。
- `fastfiler-domain` を無改造、もしくは最小改造で流用。

### 非ゴール
- domain クレートの仕様変更・機能追加 (移植が終わるまで凍結)。
- CONTEXT.md / ADR で「持たない」と決めた機能 (プラグイン/ターミナル/プレビュー等) の復活。
- macOS / Linux 対応 (Windows 専用のまま。GPUI はクロスだが当面 Windows のみ検証)。

---

## 2. 依存の取り込み方針（最重要・最初に潰すリスク）

GPUI は crates.io 公開 (`gpui 0.2.2`, `publish = true`) されているが、
内部で **fork 版 wgpu を git 依存**しており、crates.io 版は機能差・追従遅れの
リスクがある。手元に **zed のフルチェックアウト (`E:\temp\Files\zed`)** がある以上、
これを直接使うのが最速かつ最も確実。

### 採用案: ローカル zed への path 依存（開発期）
```toml
# crates/fastfiler-gpui/Cargo.toml
[dependencies]
gpui = { path = "../../../zed/crates/gpui" }
gpui_platform = { path = "../../../zed/crates/gpui_platform" }
fastfiler-domain = { path = "../fastfiler-domain" }
```
- `gpui` を path 依存にすると、芋づる式に必要な兄弟クレート
  (`gpui_macros` / `gpui_shared_string` / `gpui_platform` / `scheduler` /
  `collections` / `gpui_util` / `http_client`) は cargo が zed 内 path から自動解決。
- ネットワーク不要・即イテレーション可能。

### 安定化案: git rev ピン留め（リリース前に切替検討）
```toml
gpui = { git = "https://github.com/zed-industries/zed", rev = "<固定コミット>" }
```
- 再現性が要るなら zed の特定コミットに固定。ローカル zed の `git rev-parse HEAD`
  をそのまま rev にすれば手元と一致する。

### ツールチェーン
- GPUI は **Rust 1.95.0 / edition 2024** 要求 (`zed/rust-toolchain.toml`)。
  現行 FastFiler は 1.94.1 / edition 2021。
- 対応: リポジトリ直下に `rust-toolchain.toml` を置き 1.95.0 固定。
  新クレートは edition 2024、既存 domain は 2021 のままで混在可。

### 検証ステップ（Phase 0 の最初の関門）
1. `rustup toolchain install 1.95.0`
2. 新クレートで zed の `hello_world.rs`
   (`zed/crates/gpui/examples/hello_world.rs:1-122`) を写経してビルド & 起動。
3. **Windows でウィンドウが出るまでを最優先**で確認する。
   ここが通れば移植は技術的に成立。通らなければ git rev / vendor 等を再検討。

> ⚠️ リスク: GPUI は巨大依存でフルビルドが重い (zed 全体の workspace dep を引く)。
> 初回 `cargo build` は数十分かかり得る。CI/ビルド時間の悪化を許容するか、
> 必要 crate だけ vendor する案 (後述「9. リスク」) を別途検討。

---

## 3. アーキテクチャ対応表（floem → GPUI）

| 現行 (floem) | GPUI での置き換え | 参考 (zed/) |
|---|---|---|
| `AppState` (RwSignal 群) | ルート `Entity<FastFilerApp>` | `crates/workspace/src/workspace.rs` |
| `Tab` / `PaneState` (Clone な signal 袋) | 各 `Entity<TabModel>` / `Entity<PaneModel>` | gpui Entity |
| `SplitNode` BSP ツリー | 自前の split tree + `pane_group` 参照 | `crates/workspace/src/pane_group.rs` |
| `virtual_stack` + item scope 手動 dispose | `uniform_list(id, count, f)` | `crates/gpui/src/elements/uniform_list.rs:22-55` |
| `create_effect` / `RwSignal` | `cx.observe` / `cx.subscribe` / `cx.notify()` | `crates/gpui/src/app/context.rs` |
| `create_signal_from_channel` (リーク源) | `cx.spawn` + `EventSink → Entity` 通知 | gpui async |
| テーマ `theme_rev` で全再構築 | `cx.global::<Theme>()` + `cx.notify` | gpui global |
| `ui/footer.rs` 等の view 関数 | `impl Render for XxxView` | `crates/gpui/examples/` |
| 行クリックの index クランプ | 同様にクランプ (移植) | — |

### 状態モデル（GPUI 版）
```
Entity<FastFilerApp>            ルート。tabs, active, settings, theme(global)
 └─ Entity<TabModel>           id, title, root: SplitTree
     └─ SplitTree (enum)       Leaf(Entity<PaneModel>) | Split{dir, children}
         └─ Entity<PaneModel>  cur_path, entries(Vec<Entry> スナップショット),
                               selected, anchor, sort_key, search, watcher,
                               scroll: UniformListScrollHandle
```
- **PaneModel は signal 袋をやめて、ただの struct + Entity**。更新は
  `pane.update(cx, |p, cx| { p.entries = ...; cx.notify() })` の 1 ルートに統一。
- Entity が drop されれば watcher (Arc) も連鎖して落ちる。
  → **タブ/ペイン close でメモリがベースラインに戻る**のが構造的に保証される。

### domain との接続（EventSink ブリッジ）
`fastfiler-domain` のバックグラウンド処理 (検索・ファイルジョブ・watcher) は
別スレッドから `EventSink::emit_json(event, payload)` を呼ぶ。これを GPUI へ橋渡し:

```rust
// 案: チャネル + cx.spawn で UI スレッドへ
struct ChannelSink { tx: smol::channel::Sender<(String, serde_json::Value)> }
impl EventSink for ChannelSink {
    fn emit_json(&self, ev: &str, payload: serde_json::Value) {
        let _ = self.tx.try_send((ev.into(), payload));
    }
}
// UI 側: cx.spawn(async move { while let Ok((ev,p)) = rx.recv().await {
//   app.update(cx, |app, cx| dispatch_event(app, &ev, p, cx)); } })
```
- `"fs-change"` / `"search-hit"` / `"search-done"` / `"fs:job:progress"` /
  `"fs:job:done"` を `dispatch_event` で対応 Entity に流し込み `cx.notify()`。
- これが **floem の create_signal_from_channel リークを根本から消す**置き換え点。

### アイコン
- `domain::icons::system_icon_png()` は **RGBA PNG (`Arc<Vec<u8>>`)** を返す
  (`crates/fastfiler-domain/src/icons.rs:250-256`, LRU 256)。
- GPUI では `img()` / `Image::from_bytes` 系でそのまま描画可能。
  PNG → GPUI テクスチャのキャッシュは GPUI 側に任せる (二重キャッシュに注意)。

---

## 4. 新クレート構成

```
crates/
├ fastfiler-domain/     ← 無改造で流用 (凍結)
├ fastfiler-native/     ← floem 版。移植完了まで残し、完了後に削除
└ fastfiler-gpui/       ← 新規。GPUI バイナリ
    src/
      main.rs           application().run(...) の shim
      app.rs            Entity<FastFilerApp> ルート + イベント dispatch
      sink.rs           ChannelSink (EventSink → Entity ブリッジ)
      theme.rs          GPUI global テーマ (現 theme/mod.rs の色を移植)
      pane/
        model.rs        PaneModel
        list_view.rs    uniform_list でのファイル一覧描画
        tree_view.rs    ペイン内ツリー
      tab_bar.rs        縦タブ
      split.rs          BSP 分割 + リサイザ
      workspace_tree.rs ワークスペースツリー (Phase 4)
      search_bar.rs     検索 UI
      footer.rs         ステータスバー
      hotkeys.rs        現 hotkeys.rs を GPUI Action に移植
      settings_dialog.rs
```
- `experimental/` に GPUI 検証 POC を置いてから本実装に入ると安全
  (現行も `experimental/floem-filelist-poc` がある慣習に合わせる)。

---

## 5. フェーズ計画（各フェーズで「動くもの」を出す）

### Phase 0 — 足場（最重要の関門）
- `rust-toolchain.toml` 追加 (1.95.0)。
- `crates/fastfiler-gpui` 作成、zed への path 依存を通す。
- `hello_world` 相当でウィンドウ起動を確認。
- **Exit 条件: Windows で空ウィンドウが出る。** ここを越えれば GO。

### Phase 1 — 単一ペインのファイル一覧（メモリ問題の本丸検証）
- `PaneModel` + `uniform_list` で `domain::fs::list_dir` の結果を表示。
- 名前/サイズ/更新日時/アイコン (RGBA PNG) の列描画。
- スクロール仮想化が効いていること、System32 級フォルダが瞬時に出ることを確認。
- **Exit 条件: 大量フォルダを開閉してもメモリがベースラインに戻る**
  (既存 `core/debug_mem.rs` の計装を新クレートへ移植して計測)。

### Phase 2 — 操作とリアクティビティ
- ソート (`SortKey`)、範囲選択 (anchor/shift)、キーボードナビ。
- `domain::actions` 相当: open / delete (SHFileOperationW) / rename / copy / paste。
- `EventSink` ブリッジ経由で **watcher 連携** (`fs-change` で一覧自動更新)。
- ファイルジョブ進捗 (`fs:job:progress`/`done`) → footer / progress 表示。
- **Exit 条件: watcher/ジョブのスレッドがペイン close で確実に止まる** (handles/threads 計測)。

### Phase 3 — 縦タブ + BSP 分割ペイン
- `Entity<TabModel>` の縦タブバー、タブ追加/閉じ/切替、フォーカスペイン復元。
- `SplitTree` の縦横分割 + ドラッグリサイザ。zed `pane_group.rs` を参考に**自前の軽量実装**
  (workspace crate 丸ごと依存は重いので持ち込まない)。
- **Exit 条件: タブ/ペインを多数開閉してメモリがベースラインに戻る** (本計画の主目的達成)。

### Phase 4 — ツリー系
- ワークスペースツリー (ドライブ起点・サーバ/share ノード) と ペイン内ツリー。
- ノード数が数万規模になり得るため、ここで初めて **SumTree 採用を検討**
  (`zed/crates/sum_tree`)。まずは素朴な `Vec`+`uniform_list` で実装し、
  実測でメモリ/速度が問題になったら SumTree へ差し替える (早すぎる最適化を避ける)。
- `settings.ron` の `tree_unc_shares` 永続化を移植。

### Phase 5 — 周辺機能
- 内蔵検索 + Everything (`domain::search`、`search-hit`/`search-done`)。
- D&D: 内部ペイン間、外部受信 (OLE)、右ボタン D&D。
  **要注意** (→ 9. リスク): OLE IDropTarget / win32 subclass は HWND が要る。
  GPUI のウィンドウから HWND を取得し、既存 `win32/right_drag_hook.rs` /
  `domain::ole_dnd` を再利用できるか Phase 5 冒頭で先行検証する。
- テーマ/ホットキー/フォント、設定ダイアログ、footer、modal、undo。
- ユーザーコマンド (`commands.json`)、Shift+右クリックのシェルメニュー。

### Phase 6 — 切替とクリーンアップ
- 機能パリティを STATUS.md でチェック。
- 既定バイナリを `fastfiler-gpui` に切替、`fastfiler-native` (floem) を削除。
- workspace の `floem` 依存削除、Cargo.toml の `strip=true` 復帰。
- README / ARCHITECTURE / ADR 更新 (「floem → GPUI へ移行」の ADR を 1 本起こす)。

---

## 6. メモリ検証のやり方（移植の合否判定）

- `core/debug_mem.rs` の `mem-debug` feature と `TrackingAlloc` /
  `log_snapshot` を新クレートへ移植。`PANES_ALIVE` / `TABS_ALIVE` を
  Entity 生成/drop で増減させ、**閉じた後 0 に戻るか**を機械的に確認。
- 受け入れ基準 (各 Phase の Exit 条件で使用):
  1. タブ/ペインを N 回開閉 → `panes`/`tabs` カウントがベースラインへ。
  2. `threads` / `handles` が開閉で増え続けない (watcher/検索スレッドの解放確認)。
  3. `heap_cur` が開閉サイクルで単調増加しない (wgpu の WorkingSet 据え置きは許容)。

---

## 7. domain クレートに触れる必要が出る可能性（最小改造の想定）

原則「凍結」。ただし以下は移植中に出るかもしれない:
- `EventSink` のペイロードが JSON 固定なので、GPUI 側で都度 serde_json から
  デシリアライズが必要。ホットパス (watcher 大量イベント等) で問題なら
  **型付きチャネル版の sink を domain に1本追加**する余地あり (後方互換で追加のみ)。
- それ以外 (fs/file_ops/icons/search/watcher/shell/clipboard/ole_dnd) は無改造想定。

---

## 8. 作業順序サマリ（依存関係）

```
Phase 0 (足場/ビルド確立) ──→ Phase 1 (一覧+メモリ検証)
        │                          │
        ▼                          ▼
   ここが GO/NO-GO            Phase 2 (操作+watcher)
                                   │
                                   ▼
                             Phase 3 (タブ+分割) ★主目的達成
                                   │
                       ┌───────────┼───────────┐
                       ▼           ▼           ▼
                  Phase 4(ツリー) Phase 5(D&D/検索/設定) 
                                   │
                                   ▼
                             Phase 6 (切替/撤去)
```

---

## 9. リスクと対策

| リスク | 影響 | 対策 |
|---|---|---|
| GPUI フルビルドが重い・依存巨大 | ビルド時間悪化 | Phase 0 で許容可否を判断。不可なら必要 crate のみ vendor (`zed/crates/{gpui,gpui_macros,...}` をコピーし path 依存) |
| GPUI が moving target (API 変化) | 追従コスト | git rev / vendor で**コミット固定**。安易に追従しない |
| edition 2024 / rustc 1.95 強制 | 既存ビルド影響 | 新クレートのみ 2024。domain は 2021 据え置きで混在 |
| BSP 分割が GPUI 標準に無い | 実装コスト | zed `pane_group.rs` を参考に自前実装 (workspace crate は依存しない) |
| OLE D&D 受信 / 右ボタン D&D の HWND 依存 | D&D が GPUI で再現できない恐れ | Phase 5 冒頭で **GPUI ウィンドウ→HWND 取得**を先行検証。取れれば `win32/`・`ole_dnd` 流用。取れなければ D&D 範囲を内部のみへ一時縮小 (ADR で記録) |
| アイコンの二重キャッシュ (domain LRU + GPUI texture) | メモリ微増 | どちらか一方に寄せる。GPUI 側キャッシュを使い domain LRU を縮小する選択も |
| 移植中に floem 版が腐る | 並行保守コスト | Phase 6 まで floem 版は「触らず残すだけ」。バグ修正は GPUI 版に集約 |

---

## 10. 最初の一歩（Phase 0 の具体タスク）

1. `rustup toolchain install 1.95.0` / 直下に `rust-toolchain.toml`。
2. `crates/fastfiler-gpui` を `cargo new --bin` で作成、workspace members に追加。
3. `Cargo.toml` に `gpui` / `gpui_platform` の path 依存
   (`../../../zed/crates/...`) と `fastfiler-domain` を追加。
4. `zed/crates/gpui/examples/hello_world.rs` を写経して起動確認。
5. 通ったら Phase 1 (uniform_list で `domain::fs::list_dir` 表示) へ。

> この計画は「動くものを各フェーズで出し、メモリがベースラインに戻ることを
> 都度計測で確認しながら進む」ことを最重視している。Phase 0 と Phase 3 が
> 二大関門 (ビルド成立 / 主目的=メモリ達成)。

---

## 11. 実行ログ (Progress)

### 2026-06-03 — Phase 0 完了 ✅ (最大の関門クリア)
- `crates/fastfiler-gpui` を新設。`gpui` / `gpui_platform` を **手元 zed への path 依存**
  (`../../../zed/crates/...`) で取り込み、**Windows でウィンドウ起動を確認**。
  これで GPUI 移植は技術的に成立。
- `rust-toolchain.toml` を 1.95.0 で追加 (zed 要求に一致)。
- 取り込みで判明した実務上の勘所:
  - **SSL 失効チェック失敗** (`CRYPT_E_NO_REVOCATION_CHECK 0x80092012`) で依存取得が
    失敗 → `.cargo/config.toml` に `[http] check-revoke = false` を追加して解決。
    (ネットワークが失効チェック可能になれば削除可)
  - Zed の `[patch.crates-io]` のうち、Windows 最小 gpui グラフに効くのは
    **`async-task` のみ**。これだけ Files 側 `Cargo.toml` に転記。
    notify/livekit/calloop 等はグラフに現れず不要。
  - `wgpu` (git fork) は zed の `[workspace.dependencies]` 経由なので、gpui_windows が
    `workspace = true` で**自動継承**。Files 側に書く必要なし。
  - **ビルドは速い** (フル ~1分、増分 ~5秒)。gpui/wgpu 等の重い依存は zed ビルド時の
    コンパイル成果物が再利用されるため。巨大依存の懸念 (リスク表) は実測で軽微と判明。

### 2026-06-03 — Phase 1 完了 ✅ (移植の中核思想を実証)
- `crates/fastfiler-gpui/src/pane.rs` に `PaneView` (= `Entity<PaneView>`) を実装。
  - 一覧データは **`fastfiler-domain::fs::list_dir` を無改造で再利用** (domain 流用が成立)。
  - 描画は **`uniform_list` で可視範囲のみ仮想化**。floem の virtual_stack +
    手動 scope dispose を置換。
  - 状態更新は `cx.notify()` の 1 ルートのみ。signal 袋 / effect 多重張りを排除。
  - 操作: フォルダ先頭ソート / 行クリックで選択 / ダブルクリックでフォルダへ移動 /
    「↑ 上へ」で親へ移動。クリックは `cx.listener` (内部 weak 参照、循環なし) 経由。
- GPUI/edition 2024 の勘所:
  - **行描画メソッドは `-> AnyElement` (`.into_any_element()`) で返す**。
    edition 2024 の RPIT は `&self`/`&mut Context` のライフタイムを既定で捕捉するため、
    `-> impl IntoElement` だと uniform_list の `cx.processor` クロージャから
    要素を返せずコンパイルエラーになる。AnyElement (’static 具体型) で回避。
  - `uniform_list(id, count, cx.processor(|this, range: Range<usize>, _w, cx| {...}))`。
    `range` は型注釈必須。`.track_scroll(&self.scroll)` でスクロール状態を view に保持。
  - 一覧は `div().flex_1().overflow_hidden().child(uniform_list(...).size_full())` で
    残り高さに収め、内部でスクロールさせる。

### 2026-06-03 — GPUI を vendor 化 ✅ (zed フォルダ非依存に変更)
- 方針変更: Files と zed は別 Git のため、**zed フォルダへの path 依存を廃止**し、
  GPUI と依存クレートを `Files/vendor/` に**完全移植 (vendor)**。
- `vendor/` は独立サブワークスペース。zed ルートの `[workspace.*]` をミラーし、
  zed と同じ `crates/<name>` レイアウトで 18 クレートをコピー。詳細は
  [`vendor/README.md`](../vendor/README.md)。
- 取り込み元 zed コミット: `6d72acdb99` (2026-06-03)。
- 改変は最小 (vendor していない非 Windows / dev 依存の除去のみ):
  `gpui_platform` を Windows 専用化、`gpui` から `media`/`reqwest_client`/`gpui_web` 除去。
- 検証: `Cargo.lock` に zed フォルダの絶対パス 0 件。`cargo build -p fastfiler-gpui`
  成功 (21.9s) → vendor 版でウィンドウ起動・一覧描画を確認。
- 残課題 (任意): lock に `zed-industries/{font-kit,scap}` の git 参照が残るが
  Windows ビルドでは未使用 (font-kit=macOS / scap=任意機能オフ)。完全除去は保留。
- 以後の Phase 1.5〜 はこの vendor 版 (`vendor/crates/gpui`) を基盤に進める。

### 2026-06-03 — Phase 1.5 完了 ✅ (アイコン + watcher 自動更新)
- **アイコン**: `domain::icons::{system_icon_png, folder_icon_png}` (RGBA PNG) を
  `gpui::Image::from_bytes(ImageFormat::Png, ..)` → `img()` で描画。フォルダ/拡張子
  単位で `Arc<Image>` を共有し、reload 時にまとめて用意 (render は不変参照のみ)。
- **watcher 自動更新 (リーク源置換の本丸)**: `crates/fastfiler-gpui/src/sink.rs` に
  `ChannelSink` (`EventSink` 実装) を追加。`watcher → ChannelSink → async-channel →
  cx.spawn ループ → reload → cx.notify` の鎖を構成。
  - **PaneView が drop されると sink と watcher が落ち、チャネルが閉じて spawn ループが
    自然終了する** → floem の `create_signal_from_channel` のスレッド/シグナルリークを
    構造的に排除。これが本移植の主目的 (メモリ) の中核実証。
  - 検証: 監視中フォルダにファイルを作成/削除 → 自動 reload が走りクラッシュ無し。
- 依存追加: `async-channel`(2.5) / `serde_json`(1)。`gpui::{Image, ImageFormat, img}` 使用。

### 2026-06-03 — zed-industries への git 依存を完全 purge ✅
- vendored gpui / gpui_windows から **font-kit (macOS git fork)** と
  **scap (screen-capture 用・zed git の windows-capture を芋づる)** を除去。
  関連 feature (`x11`/`screen-capture`) は no-op 化、`default` から非 Windows 機能を除外。
- 結果: `Cargo.lock` の git ソースは **`smol-rs/async-task` (patch) のみ**。
  zed-industries への参照はフォルダ・remote ともにゼロ。ビルド/起動とも正常。

### 2026-06-03 — Phase 2 (一部) 完了 ✅ (キーボード / 開く / 削除 / ソート)
- **キーボードナビ**: ルート div を `track_focus(&focus_handle)` で focusable にし、
  `on_key_down` で処理 (actions/keymap は使わず直接ハンドリング)。
  ↑↓ / PageUp/Down / Home/End で選択移動 (選択は `scroll_to_item(Nearest)` で追従)、
  Enter で開く、Backspace で親へ、Delete でごみ箱、F5 で再読込。初回描画で自動フォーカス。
- **ファイルを開く**: ダブルクリック / Enter でフォルダ→移動、ファイル→
  `domain::shell::open_with_shell` (既定アプリ)。
- **ごみ箱削除**: Delete で `domain::file_ops::delete_to_trash`。watcher が自動反映する
  が即時性のため明示 reload も。失敗時は status に表示。
- **ソート列切替**: 列見出し (名前/サイズ/種類) クリックでソート、再クリックで昇降反転
  (▲/▼ 表示)。フォルダは常に先頭。`reload(false)` で選択を名前保持したまま再描画。
- いずれも `domain` (shell / file_ops) を**無改造で再利用**。ビルド/起動とも正常。

### 2026-06-03 — Phase 3a 完了 ✅ (縦タブ + ライフサイクル計測 = メモリ目標の実証)
- ルート Entity を `FastFilerApp` (`crates/fastfiler-gpui/src/app.rs`) に変更。
  `Vec<Tab>` (= `Entity<PaneView>` + 観測 `Subscription`) と active index を保持。
- **縦タブバー**: 左 200px。タブクリックで切替、`×` で閉じる、`＋ 新規タブ` で追加
  (アクティブと同じフォルダで開く)。最後の 1 枚は残す。タブ見出しはペインの
  表示フォルダ名に追従 (`cx.observe`)。
- **メモリ目標の可視化 (本移植の主目的)**: `pane.rs` に `PANES_ALIVE`(AtomicI64) を追加。
  `PaneView::new` で +1、`Drop` で -1。タブバー下部に `live panes: N` を表示。
  - **タブを閉じると `Entity<PaneView>` が drop → `PaneView::drop` で watcher(Arc) /
    sink / `cx.spawn` 受信ループ / 観測購読が連鎖解放** され、`live panes` がベース
    ラインへ戻る。floem 版で増殖していたライフサイクルが構造的に解決されたことを
    実機で確認できる (開く→N 増、閉じる→N 減)。
  - 観測購読は `detach()` せず `Tab` に保持し、閉じたら一緒に drop (購読の蓄積も防止)。

### 2026-06-03 — Phase 3b 完了 ✅ (BSP 分割ペイン)
- タブ内を単一ペイン → **BSP ツリー** (`PaneNode::Leaf(Entity<PaneView>)` /
  `Split{dir, children}`) へ拡張 (`crates/fastfiler-gpui/src/app.rs`)。
- **ペイン↔タブ間通信**は `PaneView: EventEmitter<PaneEvent>` で実装:
  - ペインヘッダに `↔`(左右分割) / `↕`(上下分割) / `×`(ペインを閉じる) ボタン。
  - ペイン内クリック (`on_mouse_down`) で `Activated` を emit → フォーカスペイン更新
    (青枠ハイライト)。`FastFilerApp` が `cx.subscribe` で受けてツリーを操作。
- ツリー操作はフリー関数 (`split_node` / `remove_node` / `find_pane` / `count_leaves`)。
  ペインを閉じると子 1 つの Split は畳む。タブ内最後の 1 ペインは残す。
- **メモリ目標がペイン単位でも効く**: ペインを閉じると購読(`Subscription`)を drop →
  `Entity<PaneView>` も drop → `PaneView::drop` で watcher/sink/spawn 解放。
  `live panes` が分割で増え、閉じる/タブ閉じで戻る。
- 各ペインは完全独立 (個別のフォルダ・watcher・選択・キーボード)。

### 2026-06-04 — ドラッグリサイザ完了 ✅ (Phase 3 完結)
- `Split` に `id`(安定識別子) と `ratios: Vec<f32>`(合計1.0) を追加。子は
  `flex_grow(ratio) + flex_basis(0)` で比例配分、境界に 5px のハンドルを挟む。
- ドラッグは **GPUI 標準のドラッグ機構**で実装 (canvas での bounds 取得は不要):
  - ハンンドル: `.on_drag(DraggedHandle{split_id, ix}, |..| cx.new(|_| Empty))`
    (プレビュー無しドラッグ)。カーソルは `cursor_col_resize`/`cursor_row_resize`。
  - split コンテナ: `.on_drag_move::<DraggedHandle>` — **`DragMoveEvent` が listener
    要素の実寸 `bounds` を渡す**ため、マウス位置→比率変換が直接できる。
  - ネスト対策: 親コンテナにも move が届くため、ペイロードの `split_id` が自分と
    一致する場合のみ処理。最小ペイン幅 80px でクランプ。
  - 参考: `zed/crates/editor/src/split_editor_view.rs` (読み参照のみ)。
- ペイン削除時は対応する ratio も除去して残りを正規化。
- **これで Phase 3 (縦タブ + 任意分割ペイン = FastFiler の中核アイデンティティ) が
  GPUI 上で完結**。メモリ目標も live panes で実機確認可能。

### 2026-06-04 — テキスト入力 + リネーム/新規作成 完了 ✅
- **`text_input.rs`**: gpui 公式サンプル (`vendor/crates/gpui/examples/input.rs`) を移植した
  単一行テキスト入力。`EntityInputHandler` 実装のため **IME (日本語入力) 対応**。
  カーソル/選択/IME 変換下線は custom Element (`TextElement`) で直接描画。
  - キーバインドは `bind_keys()` で **`"TextInput"` コンテキスト限定**に登録
    (backspace/left/right/ctrl-a/c/v/x 等)。入力欄フォーカス時のみ有効になり、
    PaneView の生キー処理 (`on_key_down`) と干渉しない。
- **入力モーダル** (PaneView): `F2`=リネーム (拡張子手前まで初期選択) / `F7`=新しい
  フォルダ / `F8`=新しいファイル。Enter=実行 / Esc=キャンセル / 背景クリック=キャンセル。
  - モーダル表示中は PaneView のキー操作 (Delete 等) をガード。
  - 実行は domain: `rename_path_no_overwrite` / `create_dir` (新規ファイルのみ std、
    `create_new` で非上書き)。成功後 reload + 新名を選択。失敗は status に表示。
  - オーバーレイは `.absolute()+.occlude()` (下のペインへのクリック透過を遮断)。
    閉じると `Entity<TextInput>` ごと drop (リーク無し)。

### 2026-06-04 — コピー / 切り取り / 貼り付け + ジョブ進捗 完了 ✅
- **Ctrl+C / Ctrl+X**: 選択項目を `domain::win_clipboard::clipboard_write_paths`
  (CF_HDROP + PreferredDropEffect) でクリップボードへ。**エクスプローラと相互運用可**
  (こちらでコピー→エクスプローラで貼り付け、逆も可)。
- **Ctrl+V**: `clipboard_read_paths` で読み、`domain::file_jobs::run_copy/run_move`
  (op="cut" なら move) を **std::thread で実行** (ブロッキング API のため)。
  - 進捗は既存の **sink ブリッジを再利用**: `fs:job:progress` (80ms スロットル) →
    footer に「コピー中 3/120 ファイル名」を表示、`fs:job:done` で完了/失敗表示 + reload。
  - 同一パスへの貼り付け (from==to) は安全のためスキップ。
  - `JobRegistry` はペイン毎に保持 (cancel API あり、UI は未配線)。
- 現状は単一選択のみ対象 (複数選択は未実装)。

### 2026-06-04 — 複数選択 完了 ✅
- 選択モデルを **cursor (キーボード現在位置) + selected (BTreeSet) + anchor (Shift起点)**
  のエクスプローラ型に刷新。
  - クリック=単一選択 / **Ctrl+クリック**=トグル / **Shift+クリック**=範囲選択
  - **Shift+↑↓/PageUp/Dn/Home/End**=範囲選択しながら移動 / **Ctrl+A**=全選択
- **コピー/切り取り (Ctrl+C/X) と Delete が複数対象に対応** (selected_paths() 一括)。
- 自動更新 (watcher reload) 時はカーソル/選択を**名前で復元**。
- footer 右端に「N 個選択」表示。行は 選択=青地 / カーソル=明色 で区別。
- リネーム (F2) / Enter で開く はカーソル項目が対象。

### 2026-06-04 — セッション永続化 完了 ✅ (タブ / 分割構成の保存・復元)
- `crates/fastfiler-gpui/src/session.rs`: タブ + ペインツリー (分割方向・比率・各
  ペインのフォルダ・フォーカス) を JSON で保存。
  保存先 `%APPDATA%\FastFiler\gpui_session.json`。
- **保存**: 構成変更 (タブ/分割/移動/リサイズ/フォーカス) ごとに **800ms デバウンス**
  (`cx.spawn` + `background_executor().timer`) + **アプリ終了時** (`cx.on_app_quit`)。
- **復元**: 起動時に `FastFilerApp::from_session` でツリー再構築。壊れたデータは
  安全側に補正 (存在しないフォルダ→ホーム / 子1つのSplit→畳む / 比率不正→均等)。
- 検証: 起動→自動保存→ファイル内容確認→再起動で復元起動、までエンドツーエンドで確認。
- 未対応: ウィンドウサイズ/位置の保存 (今後)。

### 2026-06-04 — 右クリックコンテキストメニュー 完了 ✅
- **行の右クリック**: 開く / コピー / 切り取り / 貼り付け / 名前の変更 / 削除 /
  新しいフォルダ / 新しいファイル。選択外の行なら単一選択に、選択内なら選択を
  保ったまま (複数対象の操作可)。
- **背景の右クリック**: 貼り付け / 最新の情報に更新 / 新しいフォルダ / ファイル。
  行ハンドラが `stop_propagation` するため背景メニューと衝突しない。
- 配置は **`deferred(anchored().position(クリック座標).snap_to_window_with_margin)`**
  (zed のメニューと同じ仕組み・画面端で自動調整)。
- 貼り付けはメニューを開いた時点のクリップボード状態で活性/不活性。
- 閉じる: メニュー外クリック (左右) / Esc。既存アクション (menu_action) へ全て委譲。

### 2026-06-04 — Phase 4 (ワークスペースツリー) 完了 ✅
- `crates/fastfiler-gpui/src/tree.rs`: **ドライブ起点のフォルダツリーパネル**。
  - ルートは `domain::fs::list_drives` (ラベル + ドライブ文字表示)。フォルダのみ。
  - **遅延展開** (`domain::fs::list_dirs`、展開時に読み直し)・子キャッシュ・⟳ で更新。
  - 表示は展開状態から平坦化した items を **`uniform_list` で仮想化描画**
    (深い展開でも軽い)。
  - ▶/▼ で展開トグル、**名前クリックで `TreeEvent::OpenDir` → アクティブタブの
    フォーカスペインに開く** (CONTEXT.md の定義どおり)。
- レイアウト: [縦タブ 200px][ツリー 220px (トグル)][ペイン群]。タブバーの
  「ツリー」ボタンで表示切替、状態は **セッションに保存** (`show_tree`)。
- 未対応 (今後): UNC サーバ/share ノード、ツリーのドラッグ幅変更、watcher 連動更新。

### 2026-06-04 — 仕上げ一式 完了 ✅
- **ツリーパネル幅のドラッグリサイズ**: `DraggedTreeHandle` + `on_drag_move`
  (分割リサイザと同方式)。幅はセッション保存 (`tree_width`)。
- **ウィンドウ位置/サイズの保存復元**: render で `window.bounds()` を監視し変化時に
  保存予約 → セッションに `[x,y,w,h]`。起動時に復元 (異常値はセンタリングへ)。
- **watcher reload デバウンス**: `fs-change` を 150ms まとめて 1 回の reload に
  (notify バースト対策。大量コピー中の連続 reload を防ぐ)。
- **キーボード切替**: **F6**=タブ内ペイン巡回 / **Ctrl+Tab / Ctrl+Shift+Tab**=タブ移動。
  - ペインは `PaneEvent::{FocusNextPane, SwitchTab}` を emit → app が
    `pending_focus` に積み、**次 render で対象ペインの FocusHandle へフォーカス**
    (subscribe ハンドラに Window が無い問題の回避)。タブ切替時もフォーカス追従。

### 2026-06-04 — Phase 5 (D&D: 内部 + 外部受信) 完了 ✅
- **重要な発見: GPUI は外部ファイルドロップをネイティブ対応** (`ExternalPaths`、
  gpui_windows が IDropTarget を実装済み) → **HWND/OLE 作業ゼロで Explorer →
  FastFiler のドロップ受信が成立**。floem 版の win32 subclass (ADR 0011) は不要に。
- **内部ペイン間 D&D**: 行を `on_drag(DraggedFiles)` でドラッグ (選択内の行なら選択
  全体、選択外なら単一)。プレビューはファイル名 or「N 個の項目」のチップ表示。
- **ドロップ先 = ペインの表示中フォルダ** (ADR 0009 どおり)。`drag_over` で受け入れ
  ハイライト。`on_drop` は内部 (`DraggedFiles`) と外部 (`ExternalPaths`) の両対応。
- **move/copy 判定**: `domain::path_util::volume_key` で同一ボリューム=移動 /
  異なる=コピー (エクスプローラ準拠)。既存の `file_jobs` スレッド + 進捗表示を共用
  (`run_transfer` に集約、貼り付けも同経路化)。
- 安全ガード: from==to スキップ / **自分自身(子孫)への転送スキップ** (無限再帰防止)。
- 未対応 (今後 / ADR 0010 どおり): FastFiler → Explorer への外部送信、
  右ボタン D&D、フォルダ行への直接ドロップ。

### 2026-06-04 — Phase 6 (リリース準備) 完了 ✅ — **計画の全 Phase 完了**
- **release ビルド成立**: `cargo build -p fastfiler-gpui --release` 1m57s、
  **6.1MB** (LTO + codegen-units=1 + opt-level=s + strip)。起動確認済み。
- リリースビルドでコンソール非表示 (`windows_subsystem = "windows"`)。
- workspace の `strip=true` を復帰 (メモリ調査用の strip=false/debug=1 は GPUI 移植で
  役目を終えた)。
- **[ADR 0012](./adr/0012-migrate-floem-to-gpui.md)** を起票 (floem → GPUI 移行の
  背景・決定・結果・未対応一覧)。
- **README.md** を GPUI 版前提に全面更新 (構成 / ビルド / 操作一覧 / セッション)。
- **floem 版 (`crates/fastfiler-native`) は削除せず残置**。
  ユーザーの調査 WIP (未コミット変更) を含むため、削除はパリティ最終確認後に
  ユーザー判断で行う。

### 2026-06-04 — main へマージ & push / floem 版削除 / ジョブキャンセル UI ✅
- ブランチ `gpui-migration` (3 コミット: WIP保全 → GPUI移植 → floem削除+doc更新) を
  main へ fast-forward マージし、origin/main へ push (`b6476ff`)。
- doc/{ARCHITECTURE,STATUS,USAGE,BUILD,RELEASE}.md を GPUI 版へ全面書き直し。
- **ジョブキャンセル UI**: コピー/移動ジョブ実行中、フッタに
  「キャンセル (Esc)」ボタン + **Esc キー**で `JobRegistry::cancel(job_id)`。
  ジョブスレッドがフラグを検知して中断し `fs:job:done (canceled)` → 「キャンセル
  しました」表示。対象は直近開始のジョブ (active_job)。

### 2026-06-04 — 検索 UI (Ctrl+F) 完了 ✅
- **Ctrl+F** で検索バー (IME 対応 TextInput 流用)。Enter=検索 / Esc=閉じる / ×ボタン。
- ペインの表示フォルダ起点。`domain::search::SearchState::start_with_sink` を流用し、
  **Everything (HTTP port 80) が応答すれば利用、不達なら内蔵検索へ自動フォールバック**。
  前回検索は domain 側で自動キャンセル。
- ヒットは `search-hit` でストリーミング受信 (sink ブリッジ共用)、結果リストを
  **uniform_list で仮想化表示** (検索中は列見出し非表示)。完了時に
  「N 件 (Everything/内蔵検索)」を表示。max 2000 件。
- **ダブルクリックでジャンプ**: フォルダ→開く / ファイル→親フォルダを開いて選択+
  中央スクロール。フォルダ移動時は検索モードを自動クローズ。

### 2026-06-04 — Undo (Ctrl+Z) 完了 ✅
- `domain::undo::UndoManager` (スタック N=20) をペイン毎に保持。
- 記録: **リネーム** (`UndoOp::Rename`) と **ごみ箱送り** (`UndoOp::Trash` —
  削除前に FileEntry から `TrashedItem` メタデータを構築し restore 照合に使う)。
- **Ctrl+Z** で逆実行: rename は `rename_path_no_overwrite(to→from)`、trash は
  `restore_from_trash` (IFileOperation)。成功で「元に戻しました: ラベル」+ reload。
- Move (貼り付け/D&D) の記録は未対応 (ジョブが非同期のため部分失敗の扱いが課題。今後)。

### 2026-06-04 — フォルダ行への直接ドロップ 完了 ✅
- ペイン内一覧の**フォルダ行**が D&D のドロップ先になった (内部 `DraggedFiles` /
  外部 `ExternalPaths` 両対応)。行ハイライトで受け入れ表示。
- ドロップ先はその行のフォルダ。行側で `stop_propagation` し、ペイン全体の
  ドロップ (表示中フォルダへ) と二重処理しない。自分自身への転送は既存ガードで防止。
- `drop_paths` は `drop_paths_into(dst_dir, ..)` に一般化。

### 2026-06-04 — exe アイコン埋め込み + 多重起動防止 完了 ✅
- **アイコン**: 旧 floem 版の `assets/icon.{ico,rc}` を git 履歴 (`7a2f11c`) から復元し
  `crates/fastfiler-gpui/assets/` へ。build.rs + embed-resource(2) で exe へ埋め込み。
- **多重起動防止**: 旧 `win32/single_instance.rs` を**そのまま移植**
  (`win32_single_instance.rs`)。Named Mutex (`Local\FastFiler-...`) で判定し、
  二重起動時は `FindWindowW(NULL, "FastFiler")` → 復元 + `SetForegroundWindow`
  して静かに終了。**ウィンドウタイトルを "FastFiler" に設定** (TitlebarOptions)。
- 実機テスト: 2 プロセス起動 → 1 個目生存 / 2 個目即終了を確認。

### 2026-06-04 — アドレスバー直接入力 + 履歴 (戻る/進む) 完了 ✅
- **履歴**: navigate (ユーザー操作の移動) で戻るスタックに積み、進むはクリア。
  **Alt+← / Alt+→**、パスバーの **← / → ボタン**、**マウス第4/第5ボタン**
  (`MouseButton::Navigate`) に対応。初期表示・戻る/進む自体は履歴に積まない。
- **アドレスバー**: パス表示を**クリックで直接入力モード** (IME 対応 TextInput、
  全選択でプリフィル)。Enter=移動 (存在しないフォルダはエラー表示) / Esc=取消。
- 既存の open() を navigate (履歴あり) / open_inner (履歴なし) に分離。

### 2026-06-04 — 新規ファイルテンプレート 完了 ✅
- 右クリックメニューに **「新規: <テンプレ名>」** (先頭10件) と
  **「テンプレートフォルダを開く」** を追加。`domain::templates` 流用
  (`%APPDATA%\fastfiler\templates` — **floem 時代のテンプレートをそのまま引き継ぐ**)。
- 作成は `create_file_from_template` (同名衝突は自動で一意名)。
  作成後は選択 + **すぐリネームモード** (エクスプローラ同様)。
- メニュー項目が動的になったため `MenuAction` を Copy→Clone 化、
  `menu_item` のラベルを動的文字列対応に。

### 2026-06-05 — ユーザーコマンド 完了 ✅ (ADR 0003 の拡張点)
- 右クリックメニューに `commands.json` のコマンドを表示 (`domain::user_commands`
  流用、`%APPDATA%\fastfiler\commands` — **floem 時代の設定をそのまま引き継ぐ**)。
- 絞り込み: 行メニューは `when` (file/dir/any) と `extensions` でフィルタ、
  背景メニューは `when=="any"` のみ。各 10 件まで。
- 実行は `run_user_command(id, RunCtx{paths: 選択全件, cwd})` — プレースホルダ
  ({path}/{paths}/{cwd} 等)・shell 実行・CREATE_NO_WINDOW は domain 側が処理。
- 「ユーザーコマンドの設定...」でコマンドフォルダを開ける (sample 自動生成あり)。

### 2026-06-05 — UNC サーバ / share ノード 完了 ✅ (CONTEXT.md の定義どおり)
- **ペインで UNC (`\\server\share\...`) を開くと自動登録** (app の pane 観測で検知 →
  `TreeView::register_unc`)。ツリーのドライブ群の下にサーバごとにグルーピング表示。
- **サーバノードは実在しないコンテナ**: クリック無効・**右クリックでサーバごと削除**。
  share ノードは通常ノード (クリックでフォーカスペインに開く・▶ で展開)。
- **セッションに永続化** (`unc_shares`)。サーバが応答しなくてもツリー UI は壊れない
  (子取得失敗は空扱い)。
- アドレスバーに `\\server\share` を直接入力しても登録される。

### 2026-06-05 — テーマ (配色プリセット) 完了 ✅
- `theme.rs`: 全 UI 色を **37 個の意味フィールド** (Theme 構造体) に集約。
  プリセット 3 種: **ダーク / ライト / ミッドナイト**。
- 取得は `th()` (static + atomic index)。**hover クロージャや custom Element の
  paint 内からも参照できる** (gpui Global 方式だと cx の無い場所で使えないため)。
- 切替: タブバー下部の **「テーマ: <名前>」ボタン**でプリセット巡回 →
  `refresh_all` (全ペイン + ツリーへ notify) で即時反映。**セッションに保存**され
  次回起動時に復元 (描画前に `set_by_name` するためフラッシュ無し)。
- 既存 48 種の色リテラルを意味名へ一括置換 (pane/app/tree/text_input)。

### 2026-06-05 — ホットキー設定 完了 ✅
- `hotkeys.rs`: **コマンド系 18 アクション** (open/parent/delete/rename/new-folder/
  new-file/refresh/search/undo/copy/cut/paste/select-all/back/forward/next-pane/
  next-tab/prev-tab) のキー割り当てを **`%APPDATA%\FastFiler\gpui_hotkeys.json`**
  でカスタマイズ可能に (`"action": "ctrl+shift+n"` 形式)。
- 初回起動時に既定値 + `_help` 付きで自動生成。不正な combo はそのアクションだけ
  既定値へフォールバック。static (RwLock) 方式で `on_key` から `lookup(ks)` 1 発。
- 背景右クリックに **「ホットキー設定を開く」(既定エディタ)** と
  **「ホットキーを再読み込み」(再起動不要)** を追加。
- 移動系 (矢印/PageUp/Dn/Home/End + Shift 範囲拡張) とモーダル内 Enter/Esc は固定。

### 2026-06-05 — Shift+右クリック = Windows シェルメニュー 完了 ✅ (ADR 0007)
- **`domain::shell::show_shell_context_menu(hwnd: isize, paths)`** を新設
  (ADR 0007 想定の「将来追加」を実装。domain への後方互換追加 — 計画 §7)。
  SHParseDisplayName → SHBindToParent → GetUIObjectOf(IContextMenu) →
  QueryContextMenu → TrackPopupMenuEx(カーソル位置) → InvokeCommand。
- **HWND は gpui Window の `HasWindowHandle` (raw-window-handle 0.6) から取得**
  (`hwnd_of`)。gpui 継承メソッドと名前衝突するためトレイト明示呼び出し。
- ペイン側: 行を **Shift+右クリック** → 選択を通常右クリックと同じ扱いにして
  シェルメニュー表示 (複数選択対応・同一フォルダ前提)。閉じたら reload。
- 制限: IContextMenu2/3 のメッセージ転送は未実装 (「新規作成」等の動的サブメニューは
  出ない場合がある)。メニュー表示中は UI スレッドをブロック (エクスプローラ同様のモーダル)。

### 2026-06-05 — D&D 外部送信 完了 ✅ (ADR 0010 — ドラッグを OLE に統一)
- **行ドラッグを GPUI 内部ドラッグから OLE `DoDragDrop` に一本化**。
  `domain::ole_dnd::start_drag` (CF_HDROP + PreferredDropEffect、件数/サイズ上限、
  最適化ムーブ対応の削除判定つき) を**無改造で利用**。
- これにより **FastFiler → Explorer への外部送信が成立** (Ctrl=コピー / Shift=移動 /
  既定はターゲット判断)。**内部ペイン間ドロップも同じ 1 ドラッグ**で動く —
  自ウィンドウは gpui の IDropTarget が ExternalPaths として受けるため、既存の
  受信ハンドラ (volume 判定 move/copy + フォルダ行ドロップ) がそのまま機能。
  DraggedFiles / DragPreview の内部ドラッグ機構は削除 (コード簡素化)。
- 開始判定: 行で左押下 → 5px 移動で発動 (クリック/ダブルクリックと共存)。
- **再入対策 (重要・実機クラッシュで判明)**: DoDragDrop はメッセージポンプを回す
  ため **UI スレッドでは呼べない**。`cx.defer` でも不十分 (App RefCell の update
  サイクル内のままで、wndproc の再借用により "RefCell already borrowed" panic)。
  → **専用 STA ワーカースレッドで実行** (OleInitialize はスレッド単位)。UI スレッド
  は自由なため自ウィンドウのドロップ受信・再描画も正常動作。結果は sink チャネル
  経由で `ole-drag-done` イベントとして UI へ戻す。自アプリ内ドロップは
  `SELF_DROP` フラグ (IDropTarget→on_drop で設定) で検知し、外部向けの元削除をスキップ。
- Move 後の元削除は `DragOutcome::Move{delete_source:true}` のときのみ
  (PerformedDropEffect 照合 — データ損失防止は domain 実装どおり)。

### 2026-06-05 — 右ボタン D&D 完了 ✅ (ADR 0010/0011 — 全タスク消化)
- **domain**: `DragRequest` に `button: DragButton{Left,Right}` を追加し、
  `CDropSource` をボタンマスク方式に (対象ボタンが離されたらドロップ)。
- **行の右ボタン押下**は即メニューではなく**候補記録**に変更:
  - 動かず離す → 従来どおりメニュー (Shift ならシェルメニュー)。mouse **up** で表示。
  - 5px 動く → **右ボタン OLE ドラッグ**開始 (RIGHT_DRAG フラグ)。
- **ドロップ先での効果選択**:
  - 自アプリ内 (ペイン / フォルダ行) → **チューザー表示「ここにコピー / ここに移動 /
    キャンセル」** (ドロップ位置に anchored)。Esc / 外クリックでキャンセル。
  - Explorer 等の外部 → keystate に MK_RBUTTON が乗るため**ターゲット側が標準メニュー
    を表示** (コピー/移動/ショートカット)。
- 制限: Explorer→FastFiler 方向の右ドラッグは判別不能のため左ドラッグ相当
  (volume 判定の自動 move/copy)。

### 残タスク (改善候補 — 優先順位はユーザー指定待ち→おすすめ順で進行中)
- **未定 (保留)**: ペイン内ツリー (リスト⇔ペイン cwd 起点ツリーの表示切替)
- 検索 UI (内蔵 + Everything) / Undo UI 配線 / アドレスバー・履歴 (戻る/進む)
- UNC サーバ・share ノード / 新規ファイルテンプレート / Shift+右クリックシェルメニュー /
  ユーザーコマンド commands.json
- テーマ・ホットキーのカスタマイズ / exe アイコン埋め込み / 多重起動防止
- D&D 外部送信 (ADR 0010) / 右ボタン D&D / フォルダ行への直接ドロップ
</content>
</invoke>
