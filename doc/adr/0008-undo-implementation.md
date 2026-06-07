# ADR 0008: Undo の実装方針 (履歴スコープ・復元手段・バルク粒度・フォーカス挙動)

- 日付: 2026-05-23
- ステータス: Accepted (2026-06-07 に乖離解消 — D1 グローバル履歴 / S2 部分失敗も実装)
- 関連: [ADR 0006 (Undo の対象を 3 種に限定)](./0006-undo-scope.md)

## コンテキスト

ADR 0006 で「Undo 対象は **リネーム / 移動 / ゴミ箱送り** の 3 種、in-memory N=20」までは確定済。
実装に入るにあたり、ADR 0006 で扱わなかった 4 つの設計論点を確定する必要があった。

1. 履歴をどこに持つか (グローバル / タブ / ペイン)
2. ゴミ箱送りの Undo (復元) をどう実現するか
3. 複数ファイルを一度に操作した場合の Undo 粒度
4. Undo 実行後のフォーカス挙動

## 決定

### D1. 履歴はグローバル (`AppState` に 1 本)

`Arc<Mutex<UndoManager>>` を `AppState` 直下に持つ。
`Ctrl+Z` は **どのタブ / どのペインで押されても、直近の操作 1 つを戻す**。

### D2. ゴミ箱復元は Windows シェル `IFileOperation::MoveItems` 経由

ゴミ箱 (`CSIDL_BITBUCKET`) を `IShellFolder` で開き、削除時の `元パス + 元ファイル名 + 削除時刻` で
対応する `IShellItem` を特定し、`IFileOperation` で元の場所へ Move する。
Windows エクスプローラの「元に戻す」と同じ正規ルート。

失敗時 (同名衝突 / 物理削除されている等) は:
- ステータスバーに「`<元パス>` に戻せませんでした: <理由>」を表示
- `Shell:RecycleBinFolder` を開くボタン (またはステータスから誘導) を提示

### D3. バルクは 1 アンドゥで丸ごと戻す

ユーザーの 1 アクション (例: 5 件選択 → Delete) は **`UndoOp::Bulk { items: Vec<...> }`** として
1 枠だけ消費する。部分失敗 (5 件中 3 件成功) の場合は **成功した 3 件のみ** を `items` に積む。
Undo 実行時も同様にベストエフォートで、戻せたものと戻せなかったものをステータスに集計表示する。

### D4. Undo 後はフォーカス追従しない

Undo は **副作用の小さい操作** に留め、現在のフォーカスペイン / タブを変更しない。
fs watcher の通知で影響範囲のペインは自動 reload される。
ステータスバーに `元に戻しました: <種別> (<件数>) → <代表パス>` を表示する。

## 検討した選択肢と理由

### D1 について
- **B. タブごと**: タブを閉じると履歴消失、複数タブで作業した際にどのタブで履歴が残るか直感に反する
- **C. ペインごと**: B より更に細かく、認知負荷が高い
- グローバルなら「直前にやったこと」というメンタルモデルそのままで、エクスプローラ / Finder / VSCode 等と一致

### D2 について
- **B. ゴミ箱内パスを推測して直接 move**: タイミング依存・脆い (同じ秒に他アプリが削除すると誤対応)
- **C. ゴミ箱を開くだけ**: ADR 0006 の「ゴミ箱送りも Undo 対象」が実質骨抜きになる
- **D. 二段階削除 (内部 trash → ゴミ箱)**: 「ゴミ箱に入ったように見えて入っていない」状態が発生し OS の挙動とズレる
- A はコスト高だが正攻路で、Windows 標準と完全に同じ動作になる

### D3 について
- **B. アイテム毎に分解 (バルクが N 枠消費)**: 20 ファイル削除 = N=20 枠を即座に消費し他履歴を流す。一般ユーザーの「1 操作 1 Undo」期待と乖離
- A は 1 操作 = 1 枠で、N=20 が「最近 20 個のユーザー操作」と一致

### D4 について
- **B. 操作元タブまで切替 + ペイン navigate + 戻したファイル選択**: 「Ctrl+Z でタブが勝手に切り替わる」副作用が大きい
- **C. 現フォーカスペインだけ navigate**: 編集中のフォルダが意図せず変わる
- A は副作用最小。「どこに戻ったか」はステータスバー通知で十分

## 追加の安全策 (実装方針)

### S1. Undo は **絶対に上書きしない**

逆方向の `rename` / `move` は **宛先 (=元の場所) が空であることを確認してから** 実行する。
既存実装の `move_path` は `fs::rename` 失敗時に `fs::copy + remove` にフォールバックして
上書きしてしまうため、Undo 経路では使わない。代わりに:

- `fastfiler-domain::file_ops::rename_path_no_overwrite(from: &Path, to: &Path) -> AppResult<()>`
- `fastfiler-domain::file_ops::move_path_no_overwrite(from: &Path, to: &Path) -> AppResult<()>`

を新設し、宛先が存在したら `AppError::Other("destination exists: ...")` で失敗扱いにする。
ゴミ箱復元も同様に「元位置に何かあれば失敗」とし、自動上書きしない。

### S2. 部分失敗は **失敗分だけ stack に戻す**

`UndoOp::Move { items: [a,b,c,d,e] }` を実行して `a/b/c` 成功 / `d/e` 失敗だった場合、
失敗した `[d, e]` を **新しい `UndoOp` として stack 末尾に push し直す** (= 次の `Ctrl+Z` で再試行可能)。
ステータスバーに「元に戻しました 3 件 / 失敗 2 件 (Ctrl+Z で再試行)」を表示。

完全失敗 (全件失敗) の場合は元の `UndoOp` をそのまま stack に戻し、`Ctrl+Z` 連打で詰まないようにする。

### S3. ゴミ箱識別キーを強化

`UndoOp::Trash { items }` の各 item は次の情報を全て保持する:

```rust
struct TrashedItem {
    original_path: PathBuf,
    file_name: OsString,
    size: u64,            // 削除前ファイルサイズ (dir は 0)
    modified: SystemTime, // 削除前最終更新時刻
    is_dir: bool,
    deleted_at: SystemTime, // 削除直後に記録した時刻 (FILETIME 精度)
}
```

ゴミ箱列挙時にこれら全フィールドで候補を絞り込む。候補が 0 件 or 2 件以上なら
**自動復元は行わず**、`Shell:RecycleBinFolder` を開く導線をステータスバーに出す。

### S4. `delete_to_trash` は Undo 経路では 1 件ずつ呼ぶ

既存 `delete_to_trash(Vec<String>)` は一括 API で項目単位の成否が取れない。
`delete_selected` は内部で **1 件ずつ `delete_to_trash(vec![p])` を呼んで結果を集計** し、
成功した分だけ `TrashedItem` を作って push する。

### S5. Mutex を保持したままファイル操作・signal 更新しない

```rust
let op = { app.undo_manager.lock().pop() }; // ロック内は取り出しのみ
if let Some(op) = op {
    let result = execute_undo(op); // ロック外で I/O
    app.status_msg_focused().set(...);
    if let Some(retry) = result.retry_op {
        app.undo_manager.lock().push(retry); // 再 push 時のみ再ロック
    }
}
```

### S6. Undo 実行は PathBuf スナップショットだけで完結させる

`rows` の index や `selected` には一切触れない。Undo は path ベースのみで動かす。
watcher 通知での reload と競合しない。

## 結果

- 実装は `fastfiler-domain::undo` モジュールに集約
  - `enum UndoOp { Rename { from, to }, Move { items }, Trash { items } }`
  - `struct UndoManager { stack: VecDeque<UndoOp> }` 固定容量 20
- ゴミ箱復元は `fastfiler-domain::file_ops::restore_from_trash(item: &TrashedItem)` を新設 (Windows only)
- 上書き禁止版の rename/move を新設し、Undo 経路はこれを使う
- アクション側 (`actions.rs` / `pane.rs` D&D / `clipboard_paste`) は **成功した分のみ** items に積み push
- `hotkeys.rs` の `undo` 分岐を実 Undo に置換
- Undo 不可時はステータスバーに `元に戻せる操作はありません` を表示 (ADR 0006)

## 追記 (2026-06-07) — GPUI 版の実装状況 (同日中に乖離解消)

- domain 側 (D2 `restore_from_trash` / S1 no-overwrite API / S3 `TrashedItem` 照合 /
  N=20) は本決定どおり実装・流用されている。
- **D1 (グローバル履歴) 解消**: `pane.rs` の static `undo_store()`
  (`OnceLock<Mutex<UndoManager>>`) に 1 本化。どのペインで Ctrl+Z しても直近 1 件を
  戻し、ペインを閉じても履歴は残る。ロック内は pop/push のみ (S5 準拠)。
- **S2 (部分失敗の再 push) 実装**: Move / Trash はアイテム別に実行し、失敗分だけを
  新しい op として積み直す (`元に戻しました N 件 / 失敗 M 件 (Ctrl+Z で再試行)`)。
  全件失敗とリネーム失敗は op をそのまま戻す。
- 移動の記録は**全件成功したジョブのみ** — `run_move` がアイテム別成否を返さないため、
  部分成功ジョブの記録は domain 拡張が必要な将来課題
  ([`plan/2026-06-07-adr-reconciliation.md`](../plan/2026-06-07-adr-reconciliation.md) 参照)。
