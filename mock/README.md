# FastFiler — UI モック

`doc/plan.md` の計画に基づく **縦タブ + 任意分割ペイン + ペイン連動** を体感する UI モックです。
スタックは Vite + Solid.js + TypeScript（後で Tauri に組み込み予定）。

## セットアップ

```bash
npm install
npm run dev
```

開いたブラウザ（既定 `http://localhost:5173`）で操作してください。

## モックで確認できること

| 機能 | 操作 |
|---|---|
| **縦タブの列数指定 (1〜4)** | 左サイドバー上部の「列: 1 2 3 4」ボタン |
| **タブ追加 / 切替 / 閉じる** | 「＋ 新規」/ タブクリック / × |
| **ペイン任意分割（水平/垂直、ネスト可）** | 各ペインのツールバーの ⬌ / ⬍ ボタン |
| **スプリッタのドラッグでサイズ調整** | グレーの仕切りをドラッグ |
| **ペイン連動 (LinkBus)** | ツールバーで「Red」または「Blue」を選択。同じグループのペイン同士で同期。Red=path/scroll、Blue=selection/sort |
| **パス入力 / 親フォルダ移動** | ↑ ボタン or パス欄に直接入力 |
| **選択（複数）** | クリック / Ctrl+クリック |
| **ダブルクリックで階層移動** | フォルダ行をダブルクリック |

`Downloads` を開くとモックの大量ファイル（80件）が見えます（仮想スクロールは Phase 1 で本実装予定）。

## ディレクトリ構成

```
src/
  App.tsx               # ルート
  main.tsx              # エントリ
  store.ts              # 状態 + LinkBus 連動ロジック
  mockFs.ts             # 仮想 FS（後で Tauri FsService に置換）
  types.ts              # 型定義
  components/
    VerticalTabs.tsx    # 縦タブ（列数指定 UI）
    PaneTree.tsx        # 任意分割ペインの再帰描画 + スプリッタ
    FileList.tsx        # ファイル一覧 + ペインツールバー
  styles.css            # ダークテーマ。アニメーション全廃
doc/
  plan.md               # 計画書
```

## 計画とのマッピング

- Phase 0（土台）✅ Vite + Solid + TS スケルトン
- Phase 1（FS コア）🟡 モック層（`mockFs.ts`）として実装、後で `windows-rs` の `FsService` に差し替え
- Phase 2（タブ・ペイン・連動）✅ UI 実装済み（縦タブ列数 / Split tree / LinkBus）
- Phase 3 以降（実ファイル操作・サムネ・検索・プラグイン）⬜ 未着手

## 次のステップ

1. Tauri 2 を被せて `mockFs.ts` を Tauri Command 経由の `FsService` に置換
2. 仮想スクロール（`@tanstack/solid-virtual` 等）導入で 10 万件対応
3. `IFileOperation` / `IContextMenu` / `IThumbnailProvider` の Rust ブリッジ実装
