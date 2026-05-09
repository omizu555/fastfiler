# FastFiler 実装ステータス

最終更新: 2026-05-09 / lib/bin 分割 + 機能別フォルダ化リファクタ後

---

## 1. 実装済み (floem 版)

### コア
- [x] フォルダ表示 (virtual_stack による高速描画)
- [x] ソート切替 (名前 / 拡張子 / サイズ / 更新日時)
- [x] ファイル監視 (notify) → 自動 reload
- [x] パンくず + 直接パス入力 + ↑/←/→/⟳
- [x] ドライブ一覧 (ツリーペイン上部)

### タブ・ペイン
- [x] 縦型タブ (1〜4 列, 列数選択可)
- [x] タブ追加 / 切替 / 並び替え (D&D)
- [x] タブ内ペインの **任意分割 (BSP)** — 横分割と縦分割の任意ネスト
- [x] スプリッタ (タブペイン幅 / ツリーペイン幅 ドラッグ可変)

### ファイル操作
- [x] 複数選択 (Ctrl/Shift クリック)
- [x] コピー / 切り取り / 貼り付け (同名衝突時は自動採番)
- [x] 削除 (`SHFileOperationW` 経由のごみ箱送り — クラッシュ耐性)
- [x] リネーム / 新規ファイル / 新規フォルダ (モーダル)
- [x] ペイン内 D&D (移動)
- [x] フォルダへドロップ (同階層)
- [x] エクスプローラ ⇄ FastFiler 双方向 D&D

### 検索
- [x] 内蔵インクリメンタル検索 (Ctrl+F、cur_path / name 部分一致)
- [x] **Everything HTTP API 連携** (200ms debounce / フルパス列表示)
- [x] **ペイン単位の独立検索** (toolbar 🔍 ボタン → 分割ペイン同時検索可)

### テーマ / 表示
- [x] ライト / ダーク + 5 プリセット (default / dracula / solarized-dark / solarized-light / nord / monokai)
- [x] アクセントカラー指定 (#rrggbb)
- [x] **テーマ即時反映** (theme_rev signal で UI 再構築)
- [x] **アイコンセット切替** (emoji / minimal / colored)
- [x] **インストール済み Windows フォント選択** (検索可能インラインリスト)
- [x] UI フォントサイズ

### 設定 (`%APPDATA%\FastFiler\settings.ron`)
- [x] ウインドウサイズ・位置・最大化状態
- [x] タブ列数 / タブペイン幅 / ツリーペイン幅
- [x] 開いていたタブ (パス + BSP 分割構成)
- [x] テーマ / プリセット / アクセント / アイコンセット / フォント / フォントサイズ
- [x] ワークスペースレイアウト (tabsLeft/tabsRight/tabsHidden)
- [x] パネルドック位置 (タブ / ツリー個別に left/right/hidden)
- [x] 検索バックエンド (builtin / everything / everything ポート)

### ホットキー
- [x] 26 アクション (open/parent/refresh/rename/delete/cut/copy/paste/select-all/new-tab/close-tab/next-tab/prev-tab/pane-back/pane-forward/address-bar/open-settings 等)
- [x] 設定ダイアログから編集
- [x] dispatch_action 経由でルートレベル KeyDown ハンドラから配線

### ロギング
- [x] `%APPDATA%\FastFiler\fastfiler.log` + 旧分 `.1` ローテート
- [x] `flog!()` マクロで全モジュールから記録
- [x] D&D / delete / paste / tree follow 等を計装

---

## 2. 部分実装 / 既知の課題

- [ ] 仮想スクロールのスクロール量チューニング (要計測)
- [ ] ツリー展開時のフォルダ/ファイル種別表現 (現状フォルダのみ)
- [ ] フッター内の設定/ヘルプボタン (現状非表示)
- [ ] 連動動作・ツリーボタン・ペイン名 (UI 上 **非表示**化済 / コードは温存)

---

## 3. 未実装 (旧 Tauri 版にあった機能)

- [ ] サムネイル表示
- [ ] プレビューペイン
- [ ] ワークスペースツリー (お気に入りフォルダ)
- [ ] アイコンパック (PNG 系) 切替 UI
- [ ] プラグインホスト (`plugin-host.ts`)
- [ ] 統合ターミナル (将来は外部ターミナル起動として実装予定)
- [ ] シェル拡張コンテキストメニュー (`IContextMenu`)
- [ ] OLE D&D Effect (Move/Copy 切替) の細粒度制御

---

## 4. アーキテクチャ詳細

[`ARCHITECTURE.md`](./ARCHITECTURE.md) を参照。

## 5. ビルドと開発

[`BUILD.md`](./BUILD.md) を参照。
