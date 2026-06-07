# 2026-06-07 ADR と実装の乖離調査・解消計画

doc 整理 (README 新設 / plan フォルダ化 / STATUS・RELEASE 統合) に伴い、
ADR 0001〜0012 を現行 GPUI 実装と突き合わせて監査した記録と、残る乖離の解消計画。

## 1. 監査結果 (2026-06-07 実施)

コード根拠付きで全 12 ADR を検証した。

| ADR | 判定 | 対処 (実施済み) |
|---|---|---|
| 0001 ペイン連動削除 | ✅ 一致 (連動コードなし) | なし |
| 0002 ドック 1 スロット 1 パネル | ❌ ドック機構自体が GPUI 版に存在しない | **Superseded** 化 + 追記 |
| 0003 プラグイン削除 | ✅ 一致 (`plugin.rs` なし、commands.json 運用中) | なし |
| 0004 内蔵ターミナルなし | ✅ 一致 (`term.rs` なし、サンプルに PowerShell/CMD/WT) | なし |
| 0005 メディアプレビューなし | ✅ 一致 (`thumbnail.rs` / `preview.rs` なし) | なし |
| 0006 Undo 対象 3 種 | ⚠️ **移動の Undo が未配線** (リネーム/ごみ箱のみ) | 追記 + 本計画 §2 |
| 0007 シェルメニュー Shift 限定 | ⚠️ 中核は実装済。通常メニューの一部項目 (プログラムから開く / プロパティ / Windows メニュー…) 未実装 | 追記 + 本計画 §3 |
| 0008 Undo 実装方針 | ⚠️ D1 違反: 履歴がグローバルでなく**ペイン単位** (`PaneView::undo`) | 追記 + 本計画 §2 |
| 0009 外部 D&D ペイン単位のみ | ❌ フォルダ行への直接ドロップ実装済み (GPUI で制約消滅) | **Superseded** 化 + 追記 |
| 0010 右ボタン D&D スコープ限定 | ❌ 「将来タスク」の外部送信も実装済み (右ボタン送信含む) | 完了 + 追記 |
| 0011 Win32 サブクラス | ❌ floem 固有の対処。GPUI 移行で不要化、コードも現存しない | **Superseded** 化 |
| 0012 GPUI 移行 | ⚠️ floem 版「残す」→削除済み。「未対応」リストはペイン内ツリー以外解消 | 追記 |

主な検証根拠:

- Undo: `pane.rs:251` (`undo: UndoManager` がペインのフィールド = ペイン単位履歴)、
  `pane.rs:690` (Trash push) / `pane.rs:945` (Rename push)、
  転送経路 `run_transfer` に `UndoOp::Move` の push なし。
  domain 側は ADR どおり (`undo.rs` N=20 / `file_ops.rs` の no-overwrite 系 +
  `restore_from_trash`)
- 外部送信: `pane.rs::maybe_start_ole_drag` → `ole_dnd::start_drag` (DoDragDrop)、
  `ole_dnd.rs:509` で `DragButton::Right` (MK_RBUTTON) 対応
- 行ドロップ: `pane.rs` render_row の `drag_over::<ExternalPaths>` + `on_drop`
- サブクラス: `SetWindowSubclass` / `WM_RBUTTONUP` は crates/ に 0 件

## 2. 残乖離の解消: Undo (将来作業・コード変更)

ADR 0006/0008 の決定に実装を寄せる。

1. **移動の Undo 配線**
   - `run_transfer` (D&D / Ctrl+X→V) の完了通知 (`fs:job:done`) で、移動だった場合に
     `UndoOp::Move { items }` を push する。items は**成功した分のみ** (ADR 0008 S2)
   - 逆方向は `move_path_no_overwrite` を使う (上書き禁止 — ADR 0008 S1)。実装済 API
   - 注意: ジョブは background 実行なので、undo 履歴への push は UI スレッド側
     (`on_job_done` 相当) で行う
2. **履歴のグローバル化** (ADR 0008 D1)
   - `UndoManager` を `FastFilerApp` (もしくは static `OnceLock<Mutex<UndoManager>>`) に
     1 本化し、`Ctrl+Z` はどのペインで押してもグローバル直近 1 件を戻す
   - ペイン削除で履歴が消える現状の問題も同時に解消される
3. 完了後、ADR 0006/0008 の追記を「解消済み」に更新し、USAGE.md の
   Undo 行に「移動」を追加する

見積り: 1 セッション規模。`fastfiler-domain` の変更は不要 (UI 配線のみ)。

## 3. 残乖離の判断待ち: 通常メニュー項目 (ADR 0007)

「プログラムから開く / プロパティ / Windows メニュー…」が ADR の骨子にあるが未実装。

- 現状 Shift+右クリック (シェルメニュー) で全て代替可能で、実用上の不便は出ていない
- **推奨: 実装しない方向で ADR 0007 の骨子を改訂** (メニューは現状の項目構成を正とする)。
  ただし「Windows メニュー…」1 項目だけは発見性の観点で追加価値があるかもしれない
  (Shift+右クリックを知らなくても辿り着ける) — 要望が出たら検討
- 対応するまで ADR 0007 の追記が現状を説明している

## 4. ドキュメント整理の記録 (2026-06-07 実施済み)

- `doc/plan/` を新設し計画ファイルを移動 (`plan-YYYY-MM-DD-*.md` → `plan/YYYY-MM-DD-*.md`)。
  以後、計画資料は `plan/YYYY-MM-DD-題名.md` で作成する
- `doc/README.md` 新設 (フォルダ案内 + 実装状況サマリ)
- `RELEASE.md` → `BUILD.md` §5 に統合して削除
- `STATUS.md` 削除 (`doc/README.md` のサマリに簡約 — 詳細一覧は陳腐化が速く USAGE と重複のため)
- IDEAS.md の陳腐化した実装済ラベルを GPUI 版の現実に合わせて修正
  (#2 Ctrl+L 未割当 / #3 再実装待ち / #9 システムアイコンに置換 など)
