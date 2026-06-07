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

## 2. 残乖離の解消: Undo — **実施済み (2026-06-07)**

ADR 0006/0008 の決定に実装を寄せた (`pane.rs` のみ、domain 無変更):

1. **移動の Undo 配線** — `run_transfer_now` で衝突リネーム解決後の items から
   `(job_id, Vec<MoveItem>)` を `pending_move_undo` に記録し、`fs:job:done` の
   `ok && !canceled` で `UndoOp::Move` を履歴へ push。
   逆方向は `move_path_no_overwrite` (上書き禁止 — ADR 0008 S1)
2. **履歴のグローバル化** (ADR 0008 D1) — static `undo_store()`
   (`OnceLock<Mutex<UndoManager>>`) に 1 本化。ペインを閉じても履歴が残る
3. **S2 (部分失敗の再 push)** — undo_last をアイテム別集計に書き換え、
   失敗分だけ新しい op として積み直す

**残る制約 (将来課題)**: `file_jobs::run_move` はアイテム別成否を返さないため、
移動の記録は**全件成功ジョブのみ**。部分成功ジョブも記録するには JobDone に
per-item 結果を載せる domain 拡張が必要。また OLE 外部送信の移動 (Explorer 側が
移動先を決める) は移動先を追跡できないため対象外。

## 3. 通常メニュー項目 (ADR 0007) — **結論: 実装しない (2026-06-07 ユーザー判断)**

「プログラムから開く / プロパティ / Windows メニュー…」は追加しない。

- プロパティ等は Shift+右クリックのシェルメニューに既に出ており代替十分
- ユーザーコマンド (`commands.json`) 経由のプロパティ表示は**不適**:
  プロパティシートは呼び出しプロセスの生存中しか表示されず、外部プロセス起動
  (PowerShell の COM `InvokeVerb("properties")` 等) では即座に消える。
  Sleep で延命するハックは常駐プロセスが残るため非推奨
- ADR 0007 の骨子は「現行実装を正とする」と改訂済み

## 4. ドキュメント整理の記録 (2026-06-07 実施済み)

- `doc/plan/` を新設し計画ファイルを移動 (`plan-YYYY-MM-DD-*.md` → `plan/YYYY-MM-DD-*.md`)。
  以後、計画資料は `plan/YYYY-MM-DD-題名.md` で作成する
- `doc/README.md` 新設 (フォルダ案内 + 実装状況サマリ)
- `RELEASE.md` → `BUILD.md` §5 に統合して削除
- `STATUS.md` 削除 (`doc/README.md` のサマリに簡約 — 詳細一覧は陳腐化が速く USAGE と重複のため)
- IDEAS.md の陳腐化した実装済ラベルを GPUI 版の現実に合わせて修正
  (#2 Ctrl+L 未割当 / #3 再実装待ち / #9 システムアイコンに置換 など)
