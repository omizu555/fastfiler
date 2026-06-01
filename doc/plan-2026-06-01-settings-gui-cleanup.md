# 設定 GUI 整理 実装計画

作成日: 2026-06-01

設定ボタン (`⚙ Settings`) から開く設定ダイアログ (`crates/fastfiler-native/src/settings/`) の
GUI を整理する。**不採用機能 (ADR で確定) の撤去**と、**冗長で分かりにくい設定の一本化**を行う。

判断軸は [`/CONTEXT.md`](../CONTEXT.md) と各 [`adr/`](./adr/)。
進行は「1 セッション 1 論点」の原則に従い **3 ステップに分割**する。

---

## 1. 調査結果 (現状)

設定ダイアログのタブ構成は `General / Workspace / Search / Terminal / Hotkeys / Plugins / Debug` の 7 枚。
関連ファイル:

- `settings/dialog.rs` — 各タブの UI 構築 + ダイアログ本体 `settings_view`
- `settings/model.rs` — `AppSettings` (ランタイム signal 群) + `default_hotkeys`
- `settings/persisted.rs` — `PersistedSettings` (serde mirror) + RON I/O
- `settings/widgets.rs` — `row_check` / `row_input` / `row_select` / `row_font` / `section_label`
- 消費側: `ui/app_view.rs` (テーマ effect / レイアウト構築)、`ui/pane.rs` (icon_set / hide_pane_toolbar)、`hotkeys.rs`

### 1.1 完全に dead な設定 (UI/effect から一切消費されていない)

| 設定 | UI 定義 | model / persisted | 根拠 |
|---|---|---|---|
| Terminal タブ全体 (`show_terminal` / `terminal_height` / `terminal_shell` / `terminal_font` / `terminal_font_size`) | dialog.rs:101-112 | model.rs:43-47 / persisted.rs:64-73 | [ADR 0004](./adr/0004-no-builtin-terminal.md) 内蔵ターミナル不採用 |
| Plugins タブ全体 (`plugins_enabled`) | dialog.rs:141-176 | model.rs:53 / persisted.rs:77-78 | [ADR 0003](./adr/0003-remove-plugin-system.md) プラグイン機構不採用 |
| General「プラグインパネル表示」(`show_plugin_panel`) | dialog.rs:22-25 | model.rs:18 / persisted.rs:24 | ADR 0003 |
| General「サムネイル表示」(`show_thumbnails`) | dialog.rs:20 | model.rs:16 / persisted.rs:20 | [ADR 0005](./adr/0005-no-media-preview.md)。default が `true` で誤解を招く |
| General「プレビュー表示」(`show_preview`) | dialog.rs:21 | model.rs:17 / persisted.rs:22 | ADR 0005 |
| General「アイコンパック」(`icon_pack`) | dialog.rs:50 | model.rs:24 / persisted.rs:36 | theme 側で**参照ゼロ**。`icon_set` と重複し機能していない |
| Workspace「同パネル積み重ね」(`same_panel_stack`) | dialog.rs:65 | model.rs:32 / persisted.rs:49 | どこからも参照なし |

> 補足: `icon_pack` は `app_view.rs:311,319` でテーマ effect の変更検知 tuple に含まれるだけで、
> `theme/` 側に消費コードが存在しない (grep で 0 件)。実質 no-op。

### 1.2 生きているが冗長・分かりにくい設定

- **`workspace_layout` と `panel_dock_tabs` の二重定義**
  `app_view.rs:355-357` で
  `tabs_hidden = layout=="tabsHidden" || dock_tabs=="hidden"`、
  `tabs_right  = dock_tabs=="right"  || layout=="tabsRight"`
  と **両方を OR 解釈**。ユーザーには似た設定が 2 つ見え、どちらを操作すべきか不明。
  → `panel_dock_tabs` に一本化する。

- **panelDock の選択肢に未実装値が混在** (`dialog.rs:74,79`)
  選択肢 `left/right/top/bottom/float/hidden` のうち、実機能は `left/right/hidden` のみ。
  `top/bottom` は STATUS §2「採用予定 (未実装)」、`float` は記載なし。
  選べるのに動かず、バグに見える。→ 実装済みの 3 択に限定する。

- **ラベルへの開発者向けキー名併記** (`(showHidden)` 等)
  一般ユーザーには冗長。

### 1.3 残すもの (生きている / 中核)

- General: `initial_path` / `show_hidden` / `hide_pane_toolbar` (pane.rs:246 で使用) /
  `theme` / `theme_preset` / `accent_color` / `icon_set` (pane.rs:600 で使用) / `ui_font` / `ui_font_size`
- Workspace: `tab_columns` / `tabs_width` / `tree_width` / `panel_dock_tabs` / `panel_dock_tree`
- Search: `search_backend` / `everything_port` / `everything_scope`
- Hotkeys: `hotkeys` (default_hotkeys 23 アクション)
- Debug: perf スナップショット (撤去対象外)
- ウィンドウ状態 / open_tabs / tab_layouts / tab_locked / tree_unc_shares (UI 非表示の永続化のみ。対象外)

---

## 2. スコープ分割

### Step 1 — 不採用機能の撤去 (低リスク・最初に着手)  ✅ 完了 (2026-06-01)

§1.1 の dead 設定をすべて削除する。

> **実施結果**: dialog.rs から Terminal/Plugins タブと General の thumbnails/preview/
> pluginPanel/iconPack、Workspace の samePanelStack を削除。model.rs / persisted.rs の
> 対応フィールドを除去 (plugins_enabled も完全削除)。app_view.rs のテーマ effect から
> icon_pack を除去 (tuple 5→4 要素)。タブ構成は General/Workspace/Search/Hotkeys/Debug の 5 枚に。
> 後方互換は一時テストで確認済 (RON は未知フィールドを無視するため旧 settings.ron をそのまま読める)。
> `cargo fmt` / `cargo test --workspace` (7 passed) / `cargo build -p fastfiler-native` 通過。
> clippy `-D warnings` はリポジトリのベースライン (pane.rs/tabs.rs/tree.rs/domain tests) で
> 既に失敗しており、本変更ファイルには新規警告なし。

### Step 2 — 冗長設定の一本化  ✅ 完了 (2026-06-01)

`workspace_layout` 廃止 → `panel_dock_tabs` 統合。panelDock 選択肢を実装済み 3 値に限定。

> **実施結果**: app_view.rs の `main_row` から `workspace_layout` 依存を除去し、
> 判定を `tabs_hidden = dock_tabs=="hidden"` / `tabs_right = dock_tabs=="right"` に簡素化
> (検知 tuple も 3→2 要素)。dialog.rs から「レイアウト」row_select を削除し、
> panel_dock_tabs / panel_dock_tree の選択肢を `left/right/hidden` の 3 値に限定。
> model.rs / persisted.rs から `workspace_layout` フィールドと `def_tabs_left` 関数を削除。
> `toggle-tabs` ホットキーは panel_dock_tabs 直接トグルのため影響なし。
> `cargo fmt` / `cargo build -p fastfiler-native` / `cargo test -p fastfiler-native` 通過。新規 clippy 警告なし。

### Step 3 — わかりやすさ向上  ✅ 完了 (2026-06-01)

ラベル整理 (キー名併記の除去)、タブ構成を 5 枚に再整理、ドキュメント更新。

> **実施結果**: dialog.rs の各 row ラベルから開発者向け camelCase キー名併記
> (`(showHidden)` 等) を除去。単位・形式ヒントは残す/追加 (`(px)` / `(#rrggbb)` / `(1〜4)`)。
> タブ構成は Step 1 で General/Workspace/Search/Hotkeys/Debug の 5 枚に確定済み。
> USAGE.md §12 の設定ダイアログ表を実態 (5 タブ・英語名) に更新し、Terminal/Plugins 撤去を明記。
> §9 の「基本」タブ表記を「General」に整合。さらに調査で設定 UI に実在しない
> 「ユーザーコマンド管理 / テンプレ管理 / 再読込ボタン」を USAGE が記述していたため、
> §11.1 の手順を実態 (APPDATA フォルダ直接 + 再起動で反映) に修正。
> STATUS.md §3 不採用表に plugin/terminal/thumbnail/preview の「設定 UI も撤去済」を追記。
> `cargo fmt` / `cargo build -p fastfiler-native` / `cargo test -p fastfiler-native` (7 passed) 通過。

> 本計画では **Step 1 → Step 2 → Step 3 の順で別々に実装・検証**することを想定。
> 各ステップ完了時に `cargo fmt/clippy/test/build` を通す。

---

## 3. 変更箇所の列挙

### Step 1: 不採用機能の撤去

**`settings/dialog.rs`**
- `tab_general` から削除: `show_thumbnails` / `show_preview` / `show_plugin_panel` の `row_check` (20-25 行)、
  `icon_pack` の `row_input` (50 行)
- `tab_workspace` から削除: `same_panel_stack` の `row_check` (65 行)
- `tab_terminal` 関数を丸ごと削除 (101-112 行)
- `tab_plugins` 関数を丸ごと削除 (141-176 行)
- `settings_view` 内のタブ定義から `make_tab("terminal", "Terminal")` / `make_tab("plugins", "Plugins")` を削除 (239,241 行)
- `body` の `dyn_container` match から `"terminal"` / `"plugins"` アーム削除 (260,262 行)
- モジュール doc コメント (1 行目) のタブ列挙を更新

**`settings/model.rs`**
- `AppSettings` から削除: `show_thumbnails` / `show_preview` / `show_plugin_panel` / `icon_pack` /
  `same_panel_stack` / `show_terminal` / `terminal_height` / `terminal_shell` / `terminal_font` /
  `terminal_font_size` / `plugins_enabled`
- `from_persisted` の対応初期化行、および `plugins_enabled` 構築ブロック (97-101 行) を削除

**`settings/persisted.rs`**
- `PersistedSettings` から上記フィールドを削除
- `Default::default()` / `from_app` の対応行を削除
- 不要になった default 関数 (`def_emoji` が icon_set でまだ使われるか確認。`icon_pack` 専用のものはない。
  `show_terminal` 関連は `def_240` を terminal_height で共有しているため関数自体は残す) を精査し、未使用になった関数のみ削除

**`ui/app_view.rs`**
- テーマ effect から `icon_pack_sig` を削除し、検知 tuple を 5 要素 → 4 要素に変更 (310-331 行)

> **後方互換**: `PersistedSettings` は全フィールド `#[serde(default)]`。削除フィールドが旧
> `settings.ron` に残っていても **RON の未知フィールドはエラーになる** 点に注意。
> → 削除後も旧ファイルを読めるよう、`load()` を「未知フィールド無視」で deserialize するか確認が必要。
> ron は既定で未知フィールドを無視する (struct に無いキーは黙殺) ため、通常は問題なし。
> 念のため Step 1 完了後、旧フィールドを含む `settings.ron` を用意して起動確認する。

### Step 2: 冗長設定の一本化

**`settings/dialog.rs`**
- `tab_workspace` から「レイアウト (workspace.layout)」の `row_select` (66-70 行) を削除
- `panel_dock_tabs` / `panel_dock_tree` の選択肢を `vec!["left", "right", "hidden"]` に変更 (74,79 行)

**`ui/app_view.rs`**
- `main_row` の `dyn_container` から `layout_sig` を除去 (343,348 行)
- 判定を `tabs_hidden = dock_tabs == "hidden"`、`tabs_right = dock_tabs == "right"` に簡素化 (355,357 行)

**`settings/model.rs` / `persisted.rs`**
- `workspace_layout` フィールドと `def_tabs_left` 関数を削除

> `toggle-tabs` ホットキー (`hotkeys.rs:273`) は `panel_dock_tabs` を直接トグルしており、
> `workspace_layout` に依存しないため影響なし。

### Step 3: わかりやすさ向上

**`settings/dialog.rs`**
- 各 `row_*` のラベルから `(showHidden)` 形式のキー名併記を除去 (例: 「隠しファイルを表示」)。
  必要なら `widgets.rs` に補助テキスト引数を追加して薄いグレーで併記する案も検討
- `settings_view` のタブを `General / Workspace / Search / Hotkeys / Debug` の 5 枚に確定

**ドキュメント**
- `doc/STATUS.md` §1「ウィンドウサイズ…」「検索バックエンド」周辺の永続化項目記述を実態に合わせる
- 設定から消えた項目があれば `doc/USAGE.md` を更新

---

## 4. 検証手順

各ステップ完了ごとに以下を順に実行する ([`doc/BUILD.md`](./BUILD.md) 準拠)。

```powershell
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p fastfiler-native
```

加えて手動確認:

1. **後方互換 (Step 1)**: 削除対象フィールドを含む旧 `settings.ron` を
   `%APPDATA%\FastFiler\settings.ron` に置いて起動 → クラッシュせず読み込めること。
2. **保存往復**: 設定ダイアログを開いて `× Close` → `settings.ron` から
   削除フィールドが消え、残存フィールドが正しく書き出されること。
3. **レイアウト (Step 2)**: `panel_dock_tabs` を left/right/hidden で切替え、
   タブパネルの位置・非表示が期待どおり動くこと。`Ctrl+B` (toggle-tabs) も確認。

> GUI 実行確認はヘッドレス前提にしない。まず `cargo build -p fastfiler-native` で止め、
> 手動確認は実機 GUI で行う。

---

## 5. リスク / 留意点

- floem の `dyn_container` 内で `create_effect` を作らない (既存の effect は外側で生成済み。踏襲する)。
- `model.rs` / `persisted.rs` のフィールド削除はコンパイラが漏れを検出するため安全側。
- RON の未知フィールド扱いは Step 1 の手動確認 (検証手順 1) で必ず確認する。
- Step 3 のラベル変更は機能に影響しないが、ユーザーマニュアル (USAGE.md) との整合に注意。
