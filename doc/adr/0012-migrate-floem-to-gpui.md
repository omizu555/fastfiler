# ADR 0012: GUI を floem から GPUI (vendor) へ移行する

日付: 2026-06-04
状態: 採用

## 背景

floem 版 (crates/fastfiler-native) は v0.1.0 で機能一式が完成していたが、
**タブ/ペインの開閉でメモリが増え続ける**問題を抱えていた。調査の結果、原因は
floem のリアクティブ scope / effect / 監視スレッドのライフサイクル管理にあった:

- `dyn_container` 再構築 (タブ開閉・切替) のたびに `create_signal_from_channel` が
  監視スレッドを再 spawn し、effect/scope/スレッドがリーク (`ui/pane.rs` 旧実装)
- `virtual_stack` のアイテム scope は明示 dispose が必要で漏れやすい
- untethered `Scope::new()` の寿命管理が不明確 (`core/tree_model.rs` 旧実装)

これらは floem の「手動 scope 管理」モデルと戦い続ける限り再発しやすい構造的問題
だった。

## 決定

1. **GUI 層を Zed の GPUI へ全面移植する** (`crates/fastfiler-gpui`)。
   - 状態は `Entity<T>` (参照カウント + 決定的 drop)。タブ/ペインを閉じると
     `Entity<PaneView>` の drop を起点に watcher / sink / 非同期ループ / 購読が
     **連鎖解放**される。`PANES_ALIVE` カウンタ (UI の `live panes`) で実機確認可能。
   - 一覧/ツリーは `uniform_list` による可視範囲のみの仮想化描画。
2. **`fastfiler-domain` (約 4,300 行) は無改造で全面再利用する。**
   `EventSink` trait を境界に GUI 非依存だったため、
   `EventSink → async-channel → cx.spawn` のブリッジ (sink.rs) 1 枚で接続できた。
3. **GPUI は zed フォルダを参照せず、`vendor/` に完全移植 (vendor) する。**
   FastFiler と zed は別 Git リポジトリのため (詳細は `vendor/README.md`)。
   Windows ビルドに必要な 18 クレートのみ。取り込み元 zed コミット `6d72acdb99`。

## 結果

- **メモリ問題は構造的に解決**: タブ/ペイン/分割の開閉で `live panes` が
  ベースラインへ戻ることを実機で確認。
- floem 版 v0.1 の中核機能を GPUI 版で再現済み:
  縦タブ / BSP 分割 + ドラッグリサイズ / 仮想化一覧 / アイコン / watcher 自動更新
  (デバウンス付) / キーボード操作 / 開く/削除/リネーム/新規 (IME 対応入力) /
  コピー・切り取り・貼り付け + 進捗 / 複数選択 / 右クリックメニュー /
  ワークスペースツリー / セッション永続化 / D&D (内部 + 外部受信)。
- **想定リスクの消滅**: 外部 D&D 受信は GPUI がネイティブ対応 (`ExternalPaths`,
  gpui_windows が IDropTarget 実装済み) のため、floem 版で必要だった
  win32 サブクラス (ADR 0011) と HWND 取得作業は不要になった。
- 旧 floem 版 (`crates/fastfiler-native`) は当面ワークスペースに残す
  (削除はパリティ最終確認後に別途判断)。

## 未対応 (今後)

- FastFiler → Explorer への D&D 外部送信 (ADR 0010 の範囲のまま)
- 右ボタン D&D / フォルダ行への直接ドロップ
- UNC サーバ/share ノード、ペイン内ツリー
- ユーザーコマンド (commands.json)、Shift+右クリックのシェルメニュー (ADR 0007)
- テーマ/ホットキーのカスタマイズ、exe アイコン埋め込み、多重起動防止

## 進捗記録

移植の経緯・実装メモは `doc/plan-2026-06-03-gpui-migration.md` §11 を参照。
