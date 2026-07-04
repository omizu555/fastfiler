# ADR 0013: GUI を GPUI (vendor) から iced 0.14 へ移行する

日付: 2026-07-04
状態: 採用 (Phase 0〜6 完了、Phase 7 切替作業中)

## 背景

GPUI 版 (crates/fastfiler-gpui) は機能一式が完成し安定稼働していたが、
構造的な負債を抱えていた:

- **vendor 依存**: GPUI は crates.io 未公開のため zed リポジトリの vendor 同梱 +
  `[patch.crates-io]` (async-task) が必要。toolchain / edition が拘束され、
  ビルドが重い (クリーンビルド数分)
- **ライセンス**: vendor GPUI のライセンス扱いが曖昧で、コアロジックの再利用に制約
- **状態の散在**: EntityId ベースの状態管理でグローバル static が増殖
  (テーマの Box::leak、D&D の static、設定 store)。UI とロジックが密結合で
  単体テストがほぼ不可能 (テスト 0 本)

## 決定

GUI を **iced 0.14.0 (ピン留め)** へ全面移行する。方式は書き直し
(been-there: ADR 0012 の floem→GPUI と同じ「凍結して並行実装」方式):

1. **fastfiler-core** (新設): フレームワーク非依存の Elm 型 update
   (`Msg → Vec<Effect>`、I/O 禁止)。domain にも依存しない (MIT OR Apache-2.0)
2. **fastfiler-iced**: 薄い皮 (入力→Msg 変換 / Effect 実行 / view)。
   一覧・ツリー・タブ・メニューは**カスタム widget 直描き** (行を子 widget に
   しない仮想リスト — Phase 0 スパイクで 10 万行 60fps を実証)
3. **fastfiler-gpui は Phase 7 の切替完了まで凍結保持** (比較基準)
4. domain は無改造で再利用 (追加のみ凍結)

## 根拠 (Phase 0 スパイクで GO 判定)

- S-1 IME: iced 0.14 が初の IME 対応版。日本語入力を実機確認
- S-2 仮想リスト: 直描き方式で 10 万行 60fps
- S-3 OLE 共存: `drag_and_drop=false` + 自前 RegisterDragDrop の共存を実証

## 結果 (Phase 6 完了時点)

- **メモリ健全性 (ADR 0012 の主目的) を実測で保証**: タブ+分割 50 開閉で
  panes/watchers/スレッド (58→58)/ハンドル (690→690) が完全ベースライン復帰
- **テスト 79 本** (core 75 + iced 4): 選択モデル / BSP / タブ規則 / 衝突解決 /
  メニュー木 / セッション往復 / ツリー / ホットキーが単体テスト化
- **ベンチ (release、フル機能状態)**: B-1 System32 (4,890 件) 起動→描画完了
  609〜671ms / B-2 合成 10 万件 592〜639ms (10 万件の増分 ≈ 0〜30ms)。
  wgpu 初期化 (~0.5s) が支配的で、一覧処理自体は瞬時
- 既存ユーザーファイルは無改造で読める (N-05): セッション/設定/ホットキー/
  テーマ/commands.json/テンプレート。iced 版は `iced_*.json` に分離書き込みし、
  初回起動時に `gpui_*.json` から自動移行 (元ファイルは読むだけ)
- 改善: テーマ再読込の Box::leak 解消 / D&D 状態の AppModel 集約 /
  メニュー木の一本化 / 検索結果操作の安全化 / 多重起動防止の堅牢化
  (タイトル一致 → 共有メモリ HWND)

## 意図的な逸脱 (実機照合で判断)

- 親フォルダへ戻ると元フォルダにカーソル追従 (エクスプローラ流。GPUI 版は無選択)
- 外部 D&D 受信のドロップゾーンはペイン単位 (行レベルは内部 D&D のみ — ADR 0009 準拠)
- テーマ 37 色キーは iced Palette (主要 6 色) への射影 (細部色は widget が直接参照可能な形で保持)

## 影響

- Phase 7 完了時: fastfiler-gpui / vendor/ / `[patch.crates-io]` を削除、
  既定 bin を fastfiler-iced へ切替、ライセンス再検討 (GPL 強制の解除可否)
- クリーンビルドが大幅に軽くなり、toolchain 拘束が消える
