# FastFiler

縦タブ（列数指定可）+ 任意分割ペイン + ペイン連動を備えた **Windows 向け高速ファイラ**。
Tauri 2 + Solid.js + Rust。アニメーションは全面廃止。

- 詳細設計: [`doc/plan.md`](./doc/plan.md)
- **使い方ガイド**: [`doc/USAGE.md`](./doc/USAGE.md)（操作・ホットキー・連動・プラグイン等を網羅）

## 必要環境

- Windows 10/11 + WebView2 Runtime
- Node.js 22+
- Rust 1.77+
- Visual Studio 2022 Build Tools (MSVC)

## 開発（ホットリロード起動）

```powershell
npm install
npm run tauri:dev
```

初回ビルドは Rust の依存解決で 5〜15 分ほどかかります。2 回目以降はインクリメンタル。

## 本番ビルド & インストーラ作成

```powershell
npm install
npm run tauri:build
```

成果物は `src-tauri/target/release/` 以下に作成されます。

| ファイル | 場所 | 用途 |
|---|---|---|
| `FastFiler.exe` | `src-tauri/target/release/` | スタンドアロン実行ファイル（ZIP 配布用） |
| `FastFiler_<ver>_x64-setup.exe` | `src-tauri/target/release/bundle/nsis/` | NSIS インストーラ（推奨配布形式） |

> 配布ターゲットは `src-tauri/tauri.conf.json` の `bundle.targets` で切替可能です。`["nsis", "msi"]` にすれば MSI も同時生成されます（WiX が無ければ初回はダウンロードされます）。

### 初回リリース手順

1. **バージョン更新**
   - `package.json` の `version`
   - `src-tauri/tauri.conf.json` の `version`
   - `src-tauri/Cargo.toml` の `version`
   （3 箇所が一致している必要があります）
2. **クリーンビルド**
   ```powershell
   Remove-Item -Recurse -Force dist, src-tauri\target\release\bundle -ErrorAction SilentlyContinue
   npm install
   npm run tauri:build
   ```
3. **動作確認**
   - 生成された `FastFiler_0.1.0_x64-setup.exe` をクリーン環境（または別ユーザー）でインストール
   - 初回起動 → ドライブ列挙 / タブ作成 / 設定保存 / アンインストールまで一通り確認
4. **GitHub Release 作成**
   - タグ: `v0.1.0`
   - タイトル: `FastFiler 0.1.0 (Initial release)`
   - 添付: `FastFiler_0.1.0_x64-setup.exe` と（任意で）`FastFiler.exe` を ZIP 化したもの
   - 説明欄に主要機能（縦タブ / 任意分割 / 連動 / Everything 連携 / プラグイン基盤）と動作要件（Windows 10/11 + WebView2）を記載
5. **README に最新リリースリンク**
   - 必要に応じてバッジ・ダウンロードリンクを追記

### コード署名（任意・将来対応）
未署名 EXE は SmartScreen 警告が出ます。コード署名証明書をお持ちの場合、`tauri.conf.json` の `bundle.windows.signCommand` に `signtool` 呼び出しを設定すると自動署名されます。初回リリースは未署名でも問題ありません。

### WebView2 ランタイム
Windows 11 / 最近の Windows 10 には標準同梱されていますが、古い環境向けに `bundle.windows.webviewInstallMode` を `downloadBootstrapper`（既定）にしておくと、インストーラ実行時に必要なら自動ダウンロードされます。


## 操作

| 操作 | キー / UI |
|---|---|
| 親フォルダへ | Backspace / ↑ ボタン |
| 開く / 階層移動 | Enter / ダブルクリック（ファイルは既定アプリで） |
| 再読込 | F5 |
| リネーム | F2（単一選択時） |
| ゴミ箱へ | Delete |
| 完全削除 | Shift+Delete |
| 切り取り / コピー / 貼り付け | Ctrl+X / Ctrl+C / Ctrl+V |
| 全選択 | Ctrl+A |
| 新規フォルダ | Ctrl+Shift+N |
| 範囲選択 / 追加選択 | Shift+Click / Ctrl+Click |
| 右クリック | アプリ内コンテキストメニュー |
| ペイン水平/垂直分割 | ツールバーの ⬌ / ⬍ |
| ペイン連動 | ツールバーで Red / Blue を選択（同色ペイン同士で同期） |
| 検索（再帰） | Ctrl+F もしくは ツールバー 🔍 |
| プレビュー切替 | Ctrl+P もしくは ヘッダ 👁 |
| プラグインパネル | Ctrl+Shift+P もしくは ヘッダ 🧩 |
| 設定ダイアログ | Ctrl+, もしくは ヘッダ ⚙ |
| タブサイドバー位置切替 | Ctrl+B もしくは ヘッダ 📑（左→右→非表示） |
| ツリーパネル表示切替 | Ctrl+Shift+E もしくは ヘッダ 🌲 |
| 新しいタブ / 閉じる | Ctrl+T / Ctrl+W |
| エクスプローラで表示 / プロパティ | 右クリックメニューから |

> ホットキーは設定ダイアログの「ホットキー」タブから自由にカスタマイズできます（押したキーの組み合わせをそのままキャプチャ）。

### 縦タブ
- 列数は設定ダイアログ「基本」で 1〜8 列を指定（即時反映）。
- タブはドラッグして並べ替え可能（ドロップ位置に青いバーが出ます）。
- **タブサイドバーの右端（または `tabsRight` 時は左端）をマウスでドラッグして幅を変更**できます。
- **Ctrl+B** で「左 → 右 → 非表示」を循環。Ctrl+Shift+E で **ワークスペースツリーパネル**（複数ドライブを並列表示）を表示。
- 構成・パス・選択・スクロール位置はすべて localStorage に保存され、再起動で復元。

### ペイン連動
- 各ペインのツールバーから「Red / Blue / 連動なし」を選択。
- 設定ダイアログ「連動」タブで、グループごとに **path / selection / scroll / sort** の伝搬を個別 ON/OFF。

### パフォーマンス
- ファイルリストは **行高 28px 固定の仮想スクロール**。10万件のフォルダでも開いた瞬間に描画され、スクロールも一定負荷。
- サムネイルは IntersectionObserver で「画面に入った行のみ」遅延ロード。
- アニメーションは全面廃止 (`transition: none`)。

### ドラッグ & ドロップ
- 行をドラッグして他のペインや別フォルダへドロップ：移動
- Ctrl を押しながらドロップ：コピー
- 行間や空白へのドロップは「現在のフォルダ」へ

### サムネイル / プレビュー
- 設定ダイアログで「サムネイル」を ON にすると IShellItemImageFactory ベースで生成（LRU + ディスクキャッシュ）。
- プレビューペインは画像 / テキスト / バイナリ（hex）を自動判定。

### プラグイン (v2.0)
- `%APPDATA%\fastfiler\plugins\<id>\manifest.json` を置くと自動検出。各プラグインは既定で **無効**。プラグインパネル (`Ctrl+Shift+P`) で明示的に有効化＋クリックでアクティブ化。
- iframe 内から `window.ff` SDK (`doc/plugins-sample/sdk.js`) で API 呼び出し:
  `fs.read.dir/text` / `fs.write.text` / `fs.mkdir` / `fs.rename` / `fs.copy` / `fs.move` / `fs.delete` / `fs.stat` / `shell.open` / `pane.getActive` / `pane.setPath` / `ui.notify` / `ui.contextMenu.register` / `storage.get/set`。
- イベント: `pane.changed` / `pane.selection.changed` / `plugin.activated` / `plugin.contextMenu.invoked`。
- サンプル: `doc/plugins-sample/hello-plugin/`, `doc/plugins-sample/context-menu-demo/`。
- 詳細は [`doc/USAGE.md`](./doc/USAGE.md) §14。

## ディレクトリ構成

```
E:\temp\Files\
├ doc\plan.md             # 全体計画
├ doc\USAGE.md            # 使い方ガイド
├ doc\plugins-sample\     # サンプルプラグイン
├ mock\                   # 旧 UI モック（参照用）
├ src\                    # フロントエンド (Solid + TS)
│  ├ App.tsx, main.tsx, store.ts, fs.ts, types.ts, plugin-host.ts
│  └ components\          # VerticalTabs / PaneTree / FileList / SettingsDialog /
│                         # ContextMenu / Thumbnail / PreviewPane / SearchPanel /
│                         # PluginPanel / ToastContainer / WorkspaceTreePanel
└ src-tauri\              # Rust バックエンド
   ├ Cargo.toml, tauri.conf.json, build.rs
   ├ capabilities\default.json
   └ src\
      ├ main.rs, lib.rs, error.rs
      ├ fs_service.rs    # 列挙 / stat / drives
      ├ file_ops.rs      # copy/move/rename/delete + ゴミ箱 (IFileOperation)
      ├ watcher.rs       # notify ベース監視
      ├ shell.rs         # ShellExecute / Reveal / プロパティ
      ├ thumbnail.rs     # IShellItemImageFactory + LRU + キャッシュ
      ├ preview.rs       # テキスト/バイナリプレビュー
      ├ search.rs        # ignore + ストリーミング配信
      └ plugin.rs        # manifest 読込 + capability ブリッジ
```

## 計画と実装状況

| Phase | 内容 | 状態 |
|---|---|---|
| 0 | Tauri 2 + Solid + TS scaffold | ✅ 完了 |
| 1 | FsService（列挙/監視/基本操作）+ ドライブ列挙 | ✅ |
| 2-a | 縦タブ列数指定 | ✅ |
| 2-b | ペイン任意分割 | ✅ |
| 2-c | LinkBus 連動（path/selection/scroll/sort） | ✅ |
| 3 | ファイル操作 / ゴミ箱 / 右クリック / D&D / Reveal / プロパティ | ✅ |
| 4 | サムネイル / プレビュー | ✅ |
| 5 | 検索 (ignore + ストリーミング) | ✅ |
| 6 | WebView プラグイン基盤 (capability) | ✅ |
| 7 | 設定ダイアログ / ホットキー / README | ✅ |
| v1.1 | Everything HTTP 検索バックエンド | ✅ |
| v1.2 | Ctrl+F フォーカス / タブ循環 / ペイン状態クリア | ✅ |
| v1.3 | タブのマウスドラッグ並べ替え | ✅ |
| v1.4 | ワークスペース配置切替 / ツリーパネル / サイドバー幅ドラッグ | ✅ |
| v1.5 | テーマ切替 (system/light/dark) / ペイン名 / ドライブ一覧 | ✅ |
| v1.6 | 単一インスタンス起動 / Ctrl+F フォーカス制御修正 | ✅ |
| v1.7 | プラグインパネル幅ドラッグ準備 / バグ修正 | ✅ |
| v2.0 | プラグイン強化 (capability 追加 / SDK / コンテキストメニュー / トースト / KV ストレージ / パネル幅可変) | ✅ |
| v2.1 | フォーカスペイン追従ツリー / 自動スクロール / pane-focused 表示 | ✅ |
| v2.2 | ネットワークドライブ対応 (種別アイコン / UNC ツールチップ / `\\server\share` 直接アクセス / breadcrumbs と祖先展開の UNC 化) | ✅ |
| 将来 | ネイティブ IContextMenu / OLE D&D / USN / D&D ドッキング / コード署名 | ⬜ |

## モック

旧 UI モックは `mock/` 内に残しています。`cd mock; npm run dev` でブラウザ単体で起動可能です。
