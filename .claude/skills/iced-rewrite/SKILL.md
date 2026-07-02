---
name: iced-rewrite
description: FastFiler の GPUI → iced 全面移行プロジェクトの作業規約。iced 移行・fastfiler-core/fastfiler-iced/fastfiler-win クレート・仮想リスト・OLE D&D・IME 対応などの移行作業に着手するとき、または「iced」「移行」「rewrite」「パリティ」に関する作業を頼まれたときに必ず最初に読む。
---

# iced-rewrite 作業スキル

FastFiler を GPUI から iced へ全面移植するプロジェクトの入口。
**どのセッションも、作業前にこの順で読むこと**:

1. `doc/plan/2026-07-02-iced-rewrite.md` — 計画書 (アーキテクチャ / フェーズ / リスク / 運用)
2. `doc/plan/2026-07-02-feature-inventory.md` — パリティ正典 (F-xx/U-xx/N-xx チェックリスト)
3. 担当フェーズの GitHub Issue (マイルストーン `iced-rewrite`) — Exit 条件と発見事項
4. 必要に応じて: `doc/USAGE.md` (挙動の一次情報) / `doc/spec/` (GPUI 版の逆生成仕様 16 章、
   `traceability.md` で code 逆引き) / `CONTEXT.md` (用語と価値の優先軸)

## 鉄則

- **domain は追加のみ** (`fastfiler-domain` の互換破壊禁止。計画書 §5.4 の 4 点だけ許可)。
- **`fastfiler-gpui` は凍結** (バグ修正以外触らない。Phase 7 まで削除しない — 比較基準 + 撤退先)。
- **迷ったら core へ** (純状態・純ロジック → `fastfiler-core`、HWND 必須 → `fastfiler-win`、
  描画と入力変換 → `fastfiler-iced`)。core に置いたものには単体テストを書く。
- **update は I/O 禁止**。副作用は `Effect` を返して iced 層で実行 (計画書 §5.3)。
- ADR の不採用機能 (プラグイン/ターミナル/プレビュー/ペイン連動) を復活させない。
  凍結機能 (計画書 §1 非ゴール) を実装しない — 縫い目 (§11) を塞がないだけでよい。
- 機能挙動に触れたら `doc/USAGE.md` を更新。重い決定は `doc/adr/` に 1 ファイル 1 決定。
- 進捗は計画書 §15 (実行ログ) に日付付きで追記。

## 複数セッション並行の規約

1. 着手前に GitHub Issue に**自分をアサイン + 着手コメント** (`gh issue edit N --add-assignee @me`)。
   既にアサインがある Issue には触らない。
2. フェーズ依存 (0 → 1 → 2 → 3 → {4,5,6 並行可} → 7) を守る。
3. 並行時は `git worktree add ../fastfiler-<topic> iced-rewrite` で分離し、
   トピックブランチ → `iced-rewrite` へマージ。
4. `doc/plan/*.md` と Issue コメントが共有記憶。セッション終了時に必ず書き残す。

## ビルド / 検証

```powershell
cargo build -p fastfiler-iced            # 開発ビルド
cargo run -p fastfiler-iced              # 起動
cargo test -p fastfiler-core -p fastfiler-domain   # 単体 + 統合テスト
cargo build -p fastfiler-gpui --release  # 比較基準 (凍結版)
```

- フェーズ完了時は Issue の Exit 条件チェックボックスを全て消化し、
  インベントリの該当 F-xx を GPUI 版と突き合わせて実機確認する。
- メモリ検証 (Phase 3 以降): タブ/ペインを 50 回開閉し、`PANES_ALIVE` 相当の計装で
  ベースライン復帰を確認 (計画書 §9 B-3)。スレッド/ハンドル数は タスクマネージャ or
  `Get-Process fastfiler | Select-Object Threads,HandleCount`。
- 性能検証: 計画書 §9 のベンチ表 (B-1〜B-5)。GPUI 版と同一マシン・同一フォルダで比較。

## コミット規約

現行踏襲 (日本語、conventional 風):
`feat(iced): …` / `feat(core): …` / `refactor(core): …` / `fix(win): …` / `docs(plan): …`

## 技術メモ (ハマりどころの先例)

- **HWND 取得**: `raw_window_handle::HasWindowHandle` をトレイト明示呼び出しで
  (GPUI 版 pane.rs:3791 `hwnd_of` の先例。iced/winit でも同手法)。
- **OLE 送信**: `DoDragDrop` は必ず専用スレッド + `AttachThreadInput` (UI スレッド id と
  結合しないと即終了する)。先例: GPUI 版 pane.rs:2341-2388。
- **OLE 受信**: winit 既定のドロップ登録 (`drag_and_drop`) を無効化してから
  `domain::ole_dnd::register_drop_target(hwnd, ..)`。右ボタンは `grfKeyState` の
  MK_RBUTTON で判別。
- **UI スレッド再入禁止**: ShellExecuteW / プロセス起動 / モーダル系 Win32 呼び出しは
  必ず別スレッド (domain shell.rs が STA ワーカーを持っている。無改造で使う)。
- **domain イベント**: `ChannelSink` (async-channel) → `iced::Subscription` 1 本に集約し、
  イベントへ PaneId を載せて update で振り分ける。ペイン close 時はリソース表から
  watcher を `unwatch` (リーク検証は B-3)。
- **デバウンス既定値**: watcher 再読込 150ms / セッション保存 800ms / ジョブ進捗 80ms
  (現行値を変えない)。
