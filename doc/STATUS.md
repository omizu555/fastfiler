# FastFiler 実装ステータス

最終更新: 2026-05-23 / #6 ツリーキーボード操作 完了

FastFiler の機能を **「実装済」「採用予定 (未実装)」「不採用 (明示的に持たない)」** の
3 区分で管理する。中核アイデンティティと判断軸は [`/CONTEXT.md`](../CONTEXT.md)、
個別の設計判断は [`adr/`](./adr/) を参照。

---

## 1. 実装済み (floem 版で動作中)

### コア — 一覧 / ナビゲーション
- [x] フォルダ表示 (`virtual_stack` による高速描画)
- [x] ソート切替 (名前 / 拡張子 / サイズ / 更新日時)
- [x] ファイル監視 (`notify` クレート) → 自動 reload
- [x] パンくず + 直接パス入力 (`Ctrl+L`) + ↑ / ← / → / ⟳
- [x] ドライブ一覧 (ツリーペイン上部)

### コア — タブ / ペイン
- [x] 縦型タブ (列数 1〜4 で選択可)
- [x] タブ追加 / 切替 / 並び替え (D&D)
- [x] タブ内ペインの **任意分割 (BSP)** — 横分割と縦分割の任意ネスト
- [x] スプリッタ (タブペイン幅 / ツリーペイン幅 ドラッグ可変)

### コア — ワークスペースツリー (基本部分)
- [x] ドライブ起点表示 (遅延展開)
- [x] フォーカスペイン追従 + 祖先自動展開 + スクロール追従
- [x] フォルダクリックでフォーカスペインに反映
- [x] パネル位置 `left` / `right` / `hidden` のドック切替 (`panel_dock_tree`)
- [x] パネル幅ドラッグ可変 (140〜600px、再起動後も保持)

### ファイル操作
- [x] 複数選択 (Ctrl / Shift クリック)
- [x] コピー / 切り取り / 貼り付け (同名衝突時は自動採番)
- [x] 削除 (`SHFileOperationW` 経由のごみ箱送り — クラッシュ耐性)
- [x] リネーム / 新規ファイル / 新規フォルダ (モーダル)
- [x] ペイン内 D&D (修飾キー / ボリュームで Move/Copy 自動判別、Ctrl=Copy / Shift=Move)
- [x] フォルダへドロップ (同階層)
- [x] エクスプローラ ⇄ FastFiler 双方向 D&D — 送信側 (FastFiler → エクスプローラ) は OLE `IDataObject` で実装済。受信側 (`IDropTarget`) も実装済 (`#D Phase 2`、ペイン単位ハイライト・修飾キーは内部 D&D と統一)
- [ ] 右ボタン D&D メニュー (ここに移動 / ここにコピー / キャンセル)
- [x] 新規ファイルテンプレート (内蔵 13 種 + ユーザー定義)
- [x] `.iso / .img / .vhd / .vhdx` のマウント (`ShellExecuteW` の `mount` 動詞)
- [x] **ユーザーコマンド** (`%APPDATA%\FastFiler\commands\commands.json`)
      — 右クリックメニューに任意の外部コマンドを追加可能
- [x] **Undo** (`Ctrl+Z`) — **リネーム / 移動 / ゴミ箱送り** の 3 種限定、
      in-memory N=20、起動間で保持しない — ADR 0006 / ADR 0008

### 検索
- [x] 内蔵インクリメンタル検索 (`Ctrl+F`、cur_path / name 部分一致)
- [x] **Everything HTTP API 連携** (200ms debounce / フルパス列表示)
- [x] **ペイン単位の独立検索** (toolbar 🔍 ボタン → 分割ペイン同時検索可)

### テーマ / 表示
- [x] ライト / ダーク + 5 プリセット (default / dracula / solarized-dark / solarized-light / nord / monokai)
- [x] アクセントカラー指定 (#rrggbb)
- [x] **テーマ即時反映** (`theme_rev` signal で UI 再構築)
- [x] **アイコンセット切替** (emoji / minimal / colored)
- [x] **インストール済み Windows フォント選択** (検索可能インラインリスト)
- [x] UI フォントサイズ

### 設定 (`%APPDATA%\FastFiler\settings.ron`)
- [x] ウインドウサイズ・位置・最大化状態
- [x] タブ列数 / タブペイン幅 / ツリーペイン幅
- [x] 開いていたタブ (パス + BSP 分割構成)
- [x] テーマ / プリセット / アクセント / アイコンセット / フォント / フォントサイズ
- [x] パネルドック位置 (タブ / ツリー個別に left / right / hidden)
- [x] 検索バックエンド (builtin / everything / everything ポート)

### ホットキー
- [x] 26 アクション (open / parent / refresh / rename / delete / cut / copy / paste /
      select-all / new-tab / close-tab / next-tab / prev-tab / pane-back /
      pane-forward / address-bar / open-settings 等)
- [x] 設定ダイアログから編集
- [x] `dispatch_action` 経由でルートレベル `KeyDown` ハンドラから配線

### ロギング
- [x] `%APPDATA%\FastFiler\fastfiler.log` + 旧分 `.1` ローテート
- [x] `flog!()` マクロで全モジュールから記録
- [x] D&D / delete / paste / tree follow 等を計装

---

## 2. 採用予定 (未実装 / 部分実装)

このセクションは「実装する意思が確定済み」のもの。優先度は概ね上から高い順。

### ワークスペースツリーの追加機能 (本体は §1 で実装済)
- [x] **`Ctrl+Shift+E` トグル配線** — `panel_dock_tree` を `hidden` ↔ 直前位置 でトグル
- [x] **キーボード操作** (↑↓ で選択移動 / → で展開 or 最初の子 / ← で折畳 or 親へ / Home/End / Enter でペイン反映 / Esc でフォーカス解除)
      — `tree_focused` / `tree_focused_path` を AppState に保持し、Ctrl+Shift+E は 3 状態サイクル化 (非表示→表示+focus / focus→非表示 / 非focus→focus)
- [x] **UNC サーバノード自動登録** (`\\server\share` を 🖥️ サーバ配下に集約、
      右クリックで「ツリーから削除」/「サーバごと削除」、`settings.ron` に永続化)

### タブ / ペイン強化
- [ ] **ペイン内ツリービュー** (`📋 / 🌲` 切替) — フォルダのみ表示
      (ファイルは混ぜない、ワークスペースツリーと操作モデルを統一)
      — ペイン単位の `view_mode` 導入が前提 (基盤)
- [x] **タブのロック** (中クリックでロック / 解除、Ctrl+W で閉じなくなる)
- [x] **タブアイコン** (UNC `🌐` / ドライブレター `C:` 等の先頭表示)
      — `pretty_title` が `C: name` / `🌐 share` を生成。タブのタイトル表示に反映済
- [ ] **ドック式パネル配置を 5 ヶ所に拡張** — 現状は `left` / `right` / `hidden` の 3 ヶ所のみ。
      `top` / `bottom` を追加するには `flex_row` 直列を縦横入れ子に再構築 +
      splitter の縦版が必要。ADR 0002 (各スロット 1 パネル) は維持

### Windows 統合
- [ ] **Shift+右クリックでシェル拡張メニュー** (`IContextMenu`) を表示 — ADR 0007
- [x] **OLE D&D Effect の細粒度制御** (IDEAS 旧 #21)
      — 内部 D&D は `Ctrl=Copy / Shift=Move` + ボリューム判定で実装済。外部 OLE は送信側 (`IDataObject` + `DoDragDrop`、Ctrl=Copy / それ以外=Move) と受信側 (`IDropTarget`、内部 D&D と完全同一の修飾キー判定) ともに実装済

### ファイル操作の安心感
- [x] **プログレスダイアログ** (大量コピー / 移動 / 削除)
      — 件数 ≥100 もしくは合計 ≥50MB で右下に表示。閾値超は Undo 不可、未満は同期 + Undo 対応

### UX / 開発支援
- [ ] **パネルプリセット** (レイアウト + 開いているタブをまとめて保存・呼出)
      — ドック 5 ヶ所化 (上記) が前提
- [ ] **Spring-loaded folder** (D&D 中に 0.7 秒ホバーで自動展開)
- [x] **パフォーマンス計測パネル** (設定 →「Debug」タブで列挙時間等を表示)
- [x] **ASCII ツリーエクスポート** (選択フォルダの構造を `tree` 風にコピー)

---

## 3. 不採用 (明示的に持たない / 削除予定)

各項目には根拠となる ADR またはユーザー判断を併記する。

| 機能 | 根拠 | 補足 |
|---|---|---|
| ペイン連動 (🔴Red / 🔵Blue) | [ADR 0001](./adr/0001-remove-pane-linking.md) | 温存コードも撤去予定 |
| 同期スクロール | [ADR 0001](./adr/0001-remove-pane-linking.md) | 連動と一体で不採用 |
| プラグイン機構 (JS / WASM) | [ADR 0003](./adr/0003-remove-plugin-system.md) | `domain/plugin.rs` 削除済 |
| 内蔵ターミナル | [ADR 0004](./adr/0004-no-builtin-terminal.md) | `domain/term.rs` 削除済 / `commands.json` で外部起動 |
| サムネイル一覧 | [ADR 0005](./adr/0005-no-media-preview.md) | `domain/thumbnail.rs` 削除済 |
| プレビューペイン | [ADR 0005](./adr/0005-no-media-preview.md) | `domain/preview.rs` 削除済 |
| Quick Look (Spacebar 全画面プレビュー) | IDEAS で「やらない」 | プレビュー全般を持たない方針と整合 |
| Undo (コピー操作) | [ADR 0006](./adr/0006-undo-scope.md) | 破壊的になるため対象外 |
| USN ジャーナル監視 | 本セッションで確定 | Everything HTTP API 連携で代替 |
| 夜モード自動 (時刻トリガー) | 本セッションで確定 | テーマは手動切替のみ |
| クイックアクセス (お気に入りサイドバー) | IDEAS で「やらない」 | タブで代替 |
| ホットキー チートシート モーダル | IDEAS で「やらない」 | 設定ダイアログのホットキータブで確認可 |
| フォルダ比較モード | IDEAS で「やらない」 | 専用ツールに委ねる |
| タグ / カラーマーク | IDEAS で「やらない」 | サイドカーファイル等のリスクを避ける |
| 「最近開いたフォルダ」/ `Ctrl+Shift+T` | IDEAS で「やらない」 | タブで代替 |
| ファイルサイズ単位カスタマイズ (KiB / KB 切替) | IDEAS で「やらない」 | 既定 (KiB) で固定 |
| プラグイン経由の音声 API | IDEAS で「やらない」 | プラグイン機構自体が不採用 |

---

## 4. 既知の細かい課題 (機能の取捨選択ではなくチューニング)

- [ ] 仮想スクロールのスクロール量チューニング (要計測)
- [ ] フッター内の設定 / ヘルプボタン (現状非表示)

---

## 5. アーキテクチャ / ビルド

- 構成: [`ARCHITECTURE.md`](./ARCHITECTURE.md)
- ビルド: [`BUILD.md`](./BUILD.md)
- ユーザーマニュアル: [`USAGE.md`](./USAGE.md)
