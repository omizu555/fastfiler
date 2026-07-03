# LESSONS — セッション間で受け継ぐ学び

書き方: 日付見出しの下に「1 行の教訓 + (必要なら) 根拠・該当箇所」。新しい日付を上に。
**一般化できる知見だけ**を書く (単発の作業メモはフェーズ Issue のコメントへ)。
SKILL.md の規約へ昇格させたら、その行末に「→ SKILL 反映済み」を付ける。

観点の例: iced/winit の癖、Win32 連携のハマりどころ、ビルド/テストの時短、
性能改善の効き目 (数値付き)、やって失敗したアプローチ。

---

## 2026-07-03

- **iced/winit 入力の癖** (Phase 1 レビューで確定):
  - `keyboard::listen()` は**フォーカス喪失中の ModifiersChanged を届けない** →
    追跡している修飾キーは `window::Event::Focused/Unfocused` でリセットする
    (Alt+Tab 復帰後の stale Ctrl+クリック事故)。
  - logical key は CapsLock/Shift の影響を受ける ("a" が "A" で届く) →
    ショートカット比較は `eq_ignore_ascii_case`。
  - マウスイベントに modifiers は乗らない → App が ModifiersChanged を追跡して焼き込む。
  - 自前ダブルクリック判定は「同一行 + 500ms」だけでは不十分 — **一覧の世代 (load_gen)
    も同一性に含める** (フォルダ移動直後の同座標クリック誤爆)。
- **パリティは正典 (doc/spec + GPUI ソース) と機械的に突き合わせる**: Phase 1 の
  レビューで選択モデルの齟齬 4 件 (PageUp/Dn=固定±10 / 未設定カーソル=-1 扱い /
  Esc はカーソルも解除 / Ctrl+A は anchor=0) を検出。実装前に GPUI の該当メソッドを
  読むほうが安い。
- **iced 0.14 API の実地知見** (Phase 0 スパイクで確認):
  - エントリは `iced::application(boot, update, view)` — boot は `Fn() -> (State, Task<Msg>)`。
    builder に `.subscription(fn)` / `.window(Settings)` / `.window_size()` / `.title()`。
  - **`update()` は UI スレッド (winit イベントループ) で走る** → COM/OLE の登録は
    update 内で安全。`!Send` な登録ハンドルは thread_local に保持 (fastfiler-win/drop_target.rs)。
  - HWND は `window::open_events()` → `window::run(id, |w| w.window_handle()...)` +
    `fastfiler_win::window_interop::hwnd_from_raw` が**正規経路** (`window_handle()` は
    Window の supertrait メソッドなので import 不要)。
    ⚠ `window::raw_id` の u64 も現行 winit では HWND 値だが、それは**非公開の内部表現
    依存** (WindowId カウンタ化の提案あり) — 恒久コードで使わない (レビュー指摘)。
  - `window::frames()` 購読は Frame メッセージ → update → 再描画の**自走ループ**になる
    (計測・アニメに好適。放置すると常時再描画なので本番では条件付きにする)。
  - カスタム Widget: 0.13 の `on_event` は 0.14 で **`update()`** に改名。`layout(&mut self, ..)`。
    描画は `iced::advanced::Renderer::fill_quad` + `text::Renderer::fill_text`
    (`Text { content: String, align_x: text::Alignment, align_y: alignment::Vertical, .. }`)。
    Quad は `snap: bool` フィールドあり。
  - 日本語は `advanced-shaping` feature を有効化 (既定の basic shaping にしない)。
- **API 調査はレジストリのソース直読が最速確実**:
  `~/.cargo/registry/src/index.crates.io-*/iced-0.14.0/` を grep (docs.rs より速く、
  cargo fetch 後は必ず手元にある)。
- **clippy は `--no-deps` 必須**: `-p` 指定でも path 依存 (凍結中の domain) まで lint されて
  -D warnings が落ちる。→ SKILL 反映済み
- **仮想リストの直描き設計は実証済み**: 10 万行 worst case で 60fps 完全張り付き
  (p95 18.15ms)。行 widget を作らない方針 (計画書 §6) のまま Phase 1 へ。
- domain の `register_drop_target` は**最初から winit 対策済み** (Revoke→Register、
  ole_dnd.rs:832)。`drag_and_drop=false` と併用で二重の保険。
- **OLE Drop 時に MK_RBUTTON は取れない** (実機確認 2026-07-03): ボタンはドロップ前に
  離されるため `Drop()` の grfKeyState は 0x00。右ボタン D&D 判別は
  **DragEnter/DragOver の keys をラッチ**して Drop で参照する (Phase 5 の DragState 要件)。
- **OLE D&D の作法** (レビューで確定、fastfiler-win/drop_target.rs に実装済み):
  register 前に `is_ole_available()` を検査 (MTA 衝突で init_ole が失敗し得る) /
  ウィンドウ close 時に `revoke(hwnd)` (HWND 再利用の巻き添え解除防止) /
  希望 DROPEFFECT は **allowed マスク内から選ぶ** (マスク外は NONE に丸められ拒否) /
  終了時は revoke_all → shutdown_ole → exit。
- **windows crate が 3 バージョン並存**: domain+win 0.58 / gpui 0.61 / winit(iced) 経由
  0.62。§10-5 の統一時は 0.62 が winit 支配下で動かせない点を考慮。
- リリースビルドは workspace profile (lto=true, codegen-units=1) のため数分かかる。
  イテレーションは debug、計測だけ release。
- セルフレビュー機構を導入。実施後の `date > .claude/.selfreview-stamp` を忘れると
  Stop フックにブロックされる (仕様。無限ループはしない)。
- この環境に `jq` は無い。JSON の検証は PowerShell の `ConvertFrom-Json` を使う。
  curl は `--ssl-no-revoke` が必要 (社内網の失効チェック問題。cargo は設定済み)。
