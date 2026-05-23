# 0011. 右ボタン D&D の drop 検出を Win32 サブクラスで行う

- 状態: 採用
- 日付: 2026-05-24
- 関連: ADR 0010 (右ボタン D&D メニューのスコープ)

## 背景

ADR 0010 で「内部 D&D + 外部受信」に右ボタン D&D メニューを実装すると決めた。
内部 D&D 側の実装に着手したところ、**floem 0.2 では secondary (右) ボタンの
`PointerUp` を `EventListener::PointerUp` に配信しない** という制約が判明した。

`floem-0.2.0/src/context.rs` L290-389 を確認した結果:

- `Event::PointerUp` を受けたとき、primary 分岐 (L291-358) でのみ
  `apply_event(EventListener::PointerUp, ...)` と `EventListener::Drop` が呼ばれる。
- secondary 分岐 (L359-389) は `SecondaryClick` listener と
  `context_menu` 自動表示しか行わない。
- `cx.app_state.dragging` は `start_drag` (primary 専用) でしか set されないため、
  `EventListener::Drop` は secondary では絶対に発火しない。
- `PointerMoveEvent` は `pos` と `modifiers` (Ctrl/Shift/Alt) のみ保持し、
  マウスボタン状態を含まないため、PointerMove で「右ボタンが今離れた」
  を検出することもできない。
- `SecondaryClick` は「PointerDown と PointerUp が同一 view 内」が条件で、
  ペインをまたぐドラッグでは PointerUp 側 view で `last_pointer_down` が None
  になり発火しない。drop 先 view では受け取れない。

結論として、floem 0.2 のイベント機構のみで右ボタン drop 検出は不可能。

## 決定

**Windows メッセージレベルで `WM_RBUTTONUP` を直接フックする方式を採用する。**

具体的には:

1. アプリ起動時に floem ウィンドウの HWND を取得し、`SetWindowSubclass` で
   サブクラスを登録する (既存 `ole_dnd` の初期化と同じレイヤ)。
2. サブクラスプロシージャ内で、`AppState` の「右ボタン D&D 中」フラグが
   立っているときだけ `WM_RBUTTONUP` を捕まえ、
   `floem::action::show_context_menu` でメニューを表示する。
3. ドロップ先ペイン特定は、ペインの PointerMove ハンドラで現在 hover 中の
   ペイン ID を `AppState` の signal に書き込み、サブクラスはそれを読む。
4. ドラッグ閾値超で「右ボタン D&D 中」フラグが立った場合、サブクラスは
   `WM_RBUTTONUP` を `DefSubclassProc` に渡さず処理を打ち切ることで、
   既存の `SecondaryClick` / `context_menu` 経路 (シェルメニュー) を抑制する。

サブクラスコールバックはメッセージポンプスレッド (= UI スレッド) で同期実行
されるため、floem の signal 操作・メニュー表示はそのまま行える。

## 代替案

### A. floem 0.2 を fork して context.rs に 1 行 patch

`secondary` 分岐に `apply_event(EventListener::PointerUp, ...)` を追加する
だけで済む。実装規模は最小。

- ✗ 上流追従が止まり、メンテナンス負担が継続する。
- ✗ Cargo.toml の依存を git/path に切替える必要があり、CI とビルド構成に
  影響が及ぶ。
- ✗ コミュニティへの patch 提案・取り込みに時間がかかる。

### B. 右ボタン D&D を諦め、コンテキストメニューに「移動先… / コピー先…」を追加

UX が変わるため、ADR 0010 で確定したスコープと矛盾する。Windows
エクスプローラの「右ドラッグ → 離した位置で確認」という操作感は再現できない。

### C. 採用: Win32 サブクラスで `WM_RBUTTONUP` を拾う

- ✓ FastFiler は Windows 専用 (`fastfiler-native`) のため、Win32 直叩きは許容範囲。
- ✓ 既に `ole_dnd` で HWND 取得・`RegisterDragDrop` 等の Win32 COM 利用例があり、
  同じパターンで実装できる。
- ✓ floem のバージョン更新時にも影響が局所化される (windows crate と
  サブクラス API のみ依存)。
- ✗ プラットフォーム依存コードがひとつ増える (ただし既に Win32 専用)。
- ✗ サブクラス解除忘れによるクラッシュリスクがあるため、起動時 1 回登録に
  限定し、ライフサイクルを単純化する。

## 影響

- `crates/fastfiler-native/src/win32/` (もしくは ole_dnd と同階層) に
  `right_drag_hook.rs` 等を追加。
- `AppState` に右ボタン D&D 用の state (source pane / paths / hover pane)
  を持たせる。
- `pane.rs` の PointerDown/PointerMove で state を更新、
  PointerUp 経由の secondary 分岐は **削除** (発火しないため)。
- `app_view.rs` の `perform_external_drop` は OLE 経路の MK_RBUTTON 判別を
  追加 (こちらは floem を介さないので影響なし)。
