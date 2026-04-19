# 自作 Windows ファイルエクスプローラ 計画書

## 1. 概要 / Problem Statement
Windows 標準エクスプローラの動作の重さ・タブ機能の不足・拡張性のなさに不満があり、
**スピード最優先 / 縦タブ（多列指定可能） / ペイン任意分割・連動 / 拡張可能** な
独自ファイラーを自作する。

## 2. ゴール / Non-goals
### Goals
- Windows エクスプローラと同等の基本機能（コピー、移動、削除、リネーム、D&D、コンテキストメニュー、サムネイル、プレビュー、検索、属性ダイアログ、シェル拡張連携）
- 縦タブ UI（手動で 1〜4 列指定可）
- ペインを水平/垂直に任意分割でき、ペイン間の動作連動（同期スクロール、同期パス移動、ミラー操作など）を設定可能
- TypeScript で書ける WebView プラグイン拡張機構
- アニメーションを排し、応答速度を体感最優先で最適化

### Non-goals (初期)
- Linux / macOS 対応
- クラウドストレージのネイティブ統合（プラグインで実現）
- モバイル UI

## 3. 技術スタック
| 層 | 採用技術 | 理由 |
|---|---|---|
| シェル | **Rust + Tauri 2.x** | 軽量・低メモリ・ネイティブ性能 |
| UI | **TypeScript + Solid.js** (or Svelte) | 仮想DOM オーバーヘッド最小、再描画最速クラス |
| スタイル | UnoCSS / 素の CSS | ビルド軽量、アニメ無効化が容易 |
| Win32 連携 | `windows-rs` クレート | Shell API / IShellFolder / IContextMenu / IThumbnailProvider |
| ファイル監視 | `notify` クレート (ReadDirectoryChangesW) | 高速インクリメンタル更新 |
| 検索 | `ignore` + USN ジャーナル参照（任意） | NTFS 上で everything 風の高速検索 |
| プラグイン | Tauri WebView + 公開 IPC API | TS で UI/コマンド拡張 |
| 設定 | TOML (`serde`) | 人間可読・差分管理しやすい |

## 4. アーキテクチャ
```
┌─────────────────────────────────────────────┐
│ Frontend (Solid + TS) — UI / 縦タブ / ペイン │
│   ├ TabColumn(s)   ← 列数ユーザー指定        │
│   ├ PaneTree       ← 任意分割 (Split nodes)   │
│   └ Plugin Host    ← WebView iframe sandbox  │
└──────────────▲──────────────────────────────┘
               │ Tauri IPC (Command / Event)
┌──────────────┴──────────────────────────────┐
│ Core (Rust)                                 │
│   ├ FsService  : 列挙/CRUD/監視              │
│   ├ ShellBridge: IContextMenu, IShellLink…  │
│   ├ Thumbnail  : IThumbnailProvider + cache │
│   ├ SearchSvc  : USN/ignore                  │
│   ├ LinkBus    : ペイン連動イベントバス        │
│   └ PluginMgr  : 権限/IPC ルーティング        │
└─────────────────────────────────────────────┘
```

### ペイン連動 (LinkBus)
- 各ペインに `linkGroupId` を割り当て、同一グループ内で
  `path-change` / `selection` / `scroll` / `sort` のいずれを連動させるかを設定。
- イベント駆動。連動なし＝独立動作。

### 縦タブ
- 左サイドバーに `n` 列のタブ列をレンダリング（`grid-template-columns: repeat(n, 1fr)`）。
- 列数は設定 + ホットキーで動的変更。
- タブは「ペインのスナップショット」を保持し、復元可能。

## 5. 主要機能要件
- [x] ディレクトリ列挙の遅延 + 仮想スクロール（10万件でも即時）
- [x] 縦タブ（列数 1〜8 ユーザー指定、D&D 並べ替え、セッション復元）
- [x] 任意ペイン分割（水平/垂直、ネスト可、サイズ保存）
- [x] ペイン連動設定 UI（連動項目をチェックボックスで選択）
- [x] コピー/移動/削除/リネーム/新規作成（IFileOperation でゴミ箱対応）
- [x] ドラッグ&ドロップ（HTML5、Ctrl=コピー / 通常=移動）
- [x] コンテキストメニュー（アプリ内、シェル拡張は将来）
- [x] サムネイル（IShellItemImageFactory + LRU + ディスクキャッシュ）
- [x] プレビューペイン（画像/テキスト/バイナリ hex）
- [x] 検索（インクリメンタル、substring/regex、ストリーミング、キャンセル）
- [x] 属性ダイアログ（SHObjectProperties）
- [x] ホットキー全カスタム可（設定ダイアログでキャプチャ編集、localStorage 永続化）
- [x] WebView プラグイン API（capability + postMessage）
- [x] 設定ダイアログ（基本 / 連動 / ホットキー の 3 タブ）

## 6. 非機能要件
- 起動 < 200ms（cold）/ < 80ms（warm）
- ディレクトリ表示開始 < 30ms（1万件）
- アニメーション・トランジション一切なし（CSS で `transition: none !important`）
- メモリ常駐 < 80MB（プラグインなし時）
- 入力レイテンシ < 16ms

## 7. フェーズ計画（順序のみ・期日記載なし）

### Phase 0: 土台
- Tauri プロジェクト初期化、Solid セットアップ
- IPC 雛形、ロギング、設定ローダ

### Phase 1: コア列挙 & 表示
- FsService (列挙・監視・並び替え)
- 仮想スクロール対応のファイルリストビュー
- 単一ペイン・単一タブで「歩ける」状態

### Phase 2: タブ & ペイン
- 縦タブ（列数指定、D&D、セッション保存）
- ペイン任意分割（Split tree）
- LinkBus 実装 + 連動設定 UI

### Phase 3: ファイル操作 ✅
- [x] copy / move / rename / mkdir
- [x] ゴミ箱 (IFileOperation + FOFX_RECYCLEONDELETE)
- [x] アプリ内コンテキストメニュー
- [x] HTML5 D&D（移動 / Ctrl=コピー）
- [x] エクスプローラで表示 / プロパティダイアログ (SHObjectProperties)
- [ ] (将来) ネイティブ IContextMenu / OLE D&D / IShellLink

### Phase 4: 表示拡張 ✅
- [x] サムネイル (IShellItemImageFactory + LRU + ディスクキャッシュ)
- [x] プレビューペイン (画像 / テキスト / バイナリ hex)
- [x] 属性ダイアログ (Phase 3 の SHObjectProperties で兼用)

### Phase 5: 検索 ✅
- [x] ignore ベースの再帰検索（substring / regex / case 切替 / 隠しファイル切替）
- [x] tauri::AppHandle.emit によるストリーミング配信
- [x] ジョブキャンセル
- [ ] (将来) USN ジャーナル高速モード

### Phase 6: プラグイン基盤 ✅
- [x] %APPDATA%\fastfiler\plugins\<id>\manifest.json 自動検出
- [x] iframe sandbox + capability ベース API ブリッジ
- [x] 公開 API: fs.read.dir / fs.read.text / shell.open
- [x] サンプルプラグイン (doc/plugins-sample/hello-plugin)

### Phase 7: 仕上げ ✅
- [x] 設定ダイアログ (Ctrl+,)：基本 / 連動 / ホットキー 3 タブ
- [x] 仮想スクロール（行高 28px 固定、ResizeObserver で viewport 追従）
- [x] 縦タブ D&D 並べ替え
- [x] 連動チャネルチェックボックス編集
- [x] ホットキー全カスタム可（キーキャプチャ + 初期値リセット）
- [x] 分割比率 / スクロール位置の永続化
- [x] README.md 更新
- [ ] (将来) MSIX / NSIS 配布パッケージ / OLE D&D / IContextMenu / TOML エクスポート

## 8. リスクと対策
| リスク | 対策 |
|---|---|
| WebView の入力レイテンシ | Solid + 仮想化 + GPU 合成抑制、prod ビルドで実測 |
| シェル拡張の COM スレッドモデル | 専用 STA スレッドで COM 呼び出しを集約 |
| IFileOperation の権限昇格 | UAC マニフェスト + 失敗時の再試行 UI |
| サムネイル取得の遅延 | バックグラウンドキュー + LRU キャッシュ |
| プラグインのセキュリティ | iframe sandbox + capability ベースの API 公開 |

## 9. 決定事項（本セッションで確定）
- 言語/FW: **Rust + Tauri**
- スコープ: **Windows エクスプローラ フル同等 + シェル拡張連携**
- 縦タブ列数: **手動指定（1〜4 列）**
- ペイン: **任意分割 + 連動設定**
- 拡張: **WebView プラグイン（TS）**

## 10. 未決事項（次に詰めるとよい点）
- UI フレームワーク: Solid.js か Svelte か（パフォーマンス検証で決定）
- 既定の連動項目セット
- プラグインマーケット形態（ローカルフォルダ配布で十分か）
- テーマ（ダーク/ライト/カスタム CSS の許容範囲）

---

## 11. 追加改善計画（v1.1 向け）

リリース後の実機確認で挙がった改善要望を反映する。優先度順に記載。

### 11.1 ファイル/フォルダ移動 D&D 拡張
**現状**: ファイル → 別ペイン / 別タブへのドロップは可能。
**追加**: 以下の D&D も対応する。

| ドラッグ元 | ドロップ先 | 動作 |
| --- | --- | --- |
| ファイル/フォルダ行 | 同ペイン内のフォルダ行 | そのフォルダへ **移動**（Ctrl で コピー） |
| ファイル/フォルダ行 | パンくずリストの中間ノード | その階層へ **移動 / コピー** |
| ファイル/フォルダ行 | 縦タブ | 既に実装（再確認） |
| ファイル/フォルダ行 | 別ペイン | 既に実装（再確認） |

**実装ポイント**:
- 行コンポーネントに `dragenter` / `dragover` / `dragleave` ハンドラを追加し、フォルダ行のときのみ青ハイライト
- `dataTransfer.types` で内部 D&D を識別 (`application/x-fastfiler-files`)
- パンくずバーに同様のドロップゾーンを実装
- Ctrl 修飾でコピー、無修飾で移動（既存の Shift 衝突に注意）

### 11.2 ツリービューペイン（初期実装）
**目的**: 左側にフォルダ階層ツリーを常時表示し、深い階層への素早いジャンプとスコープ把握を可能にする。

**仕様**:
- ペイン種別として **`tree`** を追加（既存は `list` のみ）。`PaneNode.kind = "leaf"` の `view: "list" | "tree"` を導入
- 縦タブのとなり、もしくは各タブの初期レイアウトとして「左にツリー / 右にリスト」の **2 ペイン分割テンプレート** を提供（New Tab 時のオプション）
- ツリーノードは遅延展開（クリック展開）。仮想スクロール対応
- Red/Blue **連動チャネルに乗せる**: ツリーで選んだフォルダがリストペインに反映（path 同期）
- 表示対象: ドライブ → フォルダのみ（ファイルは表示しない）
- お気に入り（Pin）対応：右クリック → ピン留めでツリー上部に固定
- Backend: `Fs::list_dirs(path)` を新設（ファイルを除外して返す軽量版）

**UI 案**:
```
┌─────────┬──────────────────────┐
│ ▾ C:    │ Documents/           │
│  ▸ Users│ ┌─────┬─────┬────┐   │
│  ▸ Win… │ │name │size │date│   │
│ ▸ D:    │ └─────┴─────┴────┘   │
│ ★ Proj  │   foo.txt   12KB ... │
└─────────┴──────────────────────┘
```

**初期実装** とするが、スコープ最小化のため:
- Phase A: ツリー表示 + 単一選択 + path 連動
- Phase B: ピン留め / D&D ドロップ先化 / 複数選択

### 11.3 検索高速化（Everything DB 連携）
**現状**: `Ctrl+F` の検索は対象ペインのフォルダを Rust で再帰列挙するため、`C:\` ルートなど大きい範囲では遅い。
**改善**: ローカルにインストールされた **Everything のインデックス** を利用して即時検索を可能にする。

**方針**（優先順に試す）:

#### A. Everything IPC API（推奨）
Everything はバックグラウンド常駐時に **IPC（Win32 メッセージ / Named Pipe / HTTP）** で外部からクエリ可能。
- `Everything_SetSearchW` / `Everything_QueryW` / `Everything_GetResultFileNameW` などを使う
- もしくは Everything HTTP Server を有効化して `http://127.0.0.1:PORT/?search=xxx&json=1` を叩く（最も実装が簡単）
- Rust 側: `reqwest` で HTTP 呼び出し → 既存の検索ストリームに流し込む
- フォールバック: Everything が起動していなければ従来の再帰列挙に切替

#### B. Everything.db 直接読み（参考: `C:\Users\o_miz\AppData\Local\Everything\Everything.db`）
- DB は **Everything 独自バイナリ形式**（公開仕様なし）。バージョン依存リスクが高く、外部ツールでの直接パースは非推奨
- 既存の OSS 解析実装も限定的。将来的に検証するが初期実装からは外す

**実装スコープ**:
1. 設定ダイアログ「基本」タブに **検索バックエンド** 選択肢を追加
   - `built-in` (既定 / 再帰列挙)
   - `everything` (Everything HTTP/IPC)
2. Everything HTTP Server (既定 ポート未設定 → ユーザーが Everything 側で有効化が必要) のポート番号を設定
3. Rust 側 `search_svc` に `EverythingClient` を追加
   - クエリ生成: 範囲フォルダを `path:"C:\foo"` プレフィックス付与で絞り込み
   - 結果を `SearchHit { path, name, size, mtime }` にマッピング
   - 既存の `search_stream` チャネルへ emit
4. 失敗時は内蔵検索へフォールバック（ステータスバーに通知）
5. 検索パネルに「Everything: ON / OFF」のインジケータ表示

**注意**:
- Everything HTTP Server は既定で OFF。ユーザーには「Everything → Tools → Options → HTTP Server」を有効化するよう **USAGE.md にも追記**
- 認証なし運用は localhost 限定であることを明示
- regex モード時は Everything 側の `regex:` プレフィックスにマッピング

### 11.4 実装優先度
1. 11.3 検索高速化 (Everything 連携) — 体感の改善が最も大きい
2. 11.1 行/パンくずへの D&D 拡張 — 工数軽め
3. 11.2 ツリービューペイン — 設計含めもう一段大きい

### 11.5 受入基準
- [x] フォルダ行に D&D で別ファイルを落とすと移動できる（Ctrl でコピー）
- [x] パンくず中間ノードへの D&D で対象階層に反映される
- [x] ツリービューを持つタブを新規作成でき、ノードクリックで右ペインのリストが連動する
- [x] 設定で「Everything」を選ぶと、Everything 起動中は検索結果が体感即時で返る
- [x] Everything 未起動時は内蔵検索へ自動フォールバックし、ステータスバーで通知される

### 11.6 実装結果（v1.1 完了）
- **検索**: `src-tauri/src/everything.rs` 新設（HTTP クライアント）。設定 → `検索` タブで backend 切替・ポート・スコープ・接続テスト。失敗時 builtin に自動フォールバック
- **D&D**: 行はフォルダ判定ハイライト → 既存 `handleDrop` で移動 / Ctrl コピー。アドレスバーをパンくず化し各セグメントを drop target に
- **ツリー**: `src/components/TreeView.tsx` 新設、`PaneState.view = "list" | "tree"`、`Fs::list_dirs` 利用、ペイン toolbar の 🌲 / treeview-head の 📋 で切替。連動グループに乗せて左ツリー / 右リストレイアウト構成可能

---

## 12. 追加改善計画（v1.2 向け）

v1.1 実機確認後に挙がった UX 微調整。粒度は小さいが操作感に直結するため早期対応する。

### 12.1 Ctrl+F 起動時に検索ボックスへ自動フォーカス
**現状**: `Ctrl+F` で SearchPanel が開くが、入力フォーカスは FileList のままで、結局マウスで検索ボックスをクリックする必要がある。

**改善**:
- `SearchPanel.tsx` の input 要素に `ref` を持たせ、`createEffect` でパネルが開かれた瞬間に `el.focus()` & `el.select()` を実行
- 既に値があるときは select、空のときは focus のみ
- `Ctrl+F` を再度押した場合：パネルが開いていれば入力欄を再 focus、閉じていれば開く

**実装ポイント**:
- FileList 側のホットキー処理 (`hotkeys.ts` の `search`) は今まで通り `setSearchMode(true)` を呼ぶ。
- SearchPanel が `searchMode()` の変化 / open prop を `createEffect` で観測し、開いた瞬間に input ref を focus。
- もし既に open 状態で再度 `Ctrl+F` が来た場合は、SearchPanel 内に外部からトリガーできる signal（あるいは `state.searchFocusTick`）を用意し、bump で再 focus。

**受入基準**:
- [ ] FileList で `Ctrl+F` → 即座に検索ボックスにキャレットが入り、そのままタイプできる
- [ ] 既に開いている状態で `Ctrl+F` → 既存テキストが全選択されて再入力可能
- [ ] `Esc` でパネルを閉じると、フォーカスは元の FileList に戻る

---

### 12.2 タブ間移動ホットキー
**現状**: ホットキー定義に `next-tab` / `prev-tab` がなく、マウスでタブをクリックするしかない。

**改善**: 標準的なタブ移動キーを追加。

| アクション | 既定キー | 備考 |
| --- | --- | --- |
| `next-tab` | `Ctrl+Tab` | 次のタブへ循環 |
| `prev-tab` | `Ctrl+Shift+Tab` | 前のタブへ循環 |
| `next-tab-alt` | `Ctrl+PageDown` | 一般的な代替 |
| `prev-tab-alt` | `Ctrl+PageUp` | 同上 |
| `goto-tab-N` | `Ctrl+1`〜`Ctrl+8` | 指定インデックス、`Ctrl+9` は最後のタブ |

**実装ポイント**:
- `types.ts` の `HotkeyAction` 型に `next-tab` / `prev-tab` / `goto-tab-1`..`goto-tab-9` を追加
- `hotkeys.ts` の defaultHotkeys / hotkeyLabels に追加
- `App.tsx` のグローバル keydown ハンドラで分岐、`store.ts` に `setActiveTabIndex(idx)` / `cycleTab(+1|-1)` ヘルパーを追加
- ブラウザ既定の `Ctrl+Tab` を確実に奪うため `e.preventDefault()` を呼ぶ
- 設定ダイアログ「ホットキー」タブにも自動的に列挙される

**受入基準**:
- [ ] `Ctrl+Tab` / `Ctrl+Shift+Tab` でタブ循環
- [ ] `Ctrl+PageDown` / `Ctrl+PageUp` でも同等動作
- [ ] `Ctrl+1`..`Ctrl+8` で n 番目のタブ、`Ctrl+9` で最後のタブにジャンプ
- [ ] 設定ダイアログのホットキー一覧に現れて、リバインド可能

---

### 12.3 検索パネル / プレビュー等のオーバーレイ状態を「タブ単位」に
**現状**: `searchMode` / プレビュー表示などはコンポーネントローカル state（`createSignal`）で持っている。タブを切替えても閉じないし、しかも別タブでも開きっぱなしに見える原因になる（同一コンポーネントが残るとローカル state が引き継がれる）。

**仕様**:
- これらの一時 UI 状態は **「移動元のタブにとどまり、移動先では消えている」** のが期待動作。
- つまり タブごとに：
  - 検索パネル open / 検索クエリ / オプション
  - プレビュー panel open
  - プラグイン panel open
- を保持する。

**実装方針**:
- `TabState`（store.ts 内）に以下を追加:
  ```ts
  ui: {
    searchOpen: boolean;
    searchQuery: string;
    searchOptions: { regex: boolean; caseSensitive: boolean };
    previewOpen: boolean;
    pluginOpen: boolean;
  }
  ```
- 既存タブの persist マイグレーション: `ui` が undefined なら既定値を補完。
- `setTabUi(tabId, partial)` ヘルパーを追加。
- SearchPanel / PreviewPanel / PluginPanel は **props もしくは store** からタブ単位の state を読み書きする（ローカル `createSignal` を廃止）。
- `Ctrl+F` ハンドラは「アクティブタブの `ui.searchOpen` を true に + focus tick を bump」。
- アクティブタブが切り替わったら、表示する SearchPanel もそのタブの state に追従するため、自動的に「移動元には残り、移動先で開いていなければ閉じている」ように見える。

**設計上の注意**:
- SearchPanel コンポーネント自体を **タブ単位でマウント / アンマウント** する構造（タブごとに別インスタンス）にしておくと、ローカル state を残したい場合にも自然。
- 既に `App.tsx` で `<For each={tabs}>` でタブ単位に PaneTree を出しているので、その隣にタブ単位の overlay 群を置く形にする。
- 「ペイン単位」ではなく「タブ単位」で持つ。複数ペインのある同一タブ内では同じ検索パネルを共有（現状踏襲）。

**受入基準**:
- [ ] タブ A で `Ctrl+F` → 検索パネルが開く
- [ ] タブ B に切り替えると検索パネルは閉じている（B では未起動）
- [ ] タブ A に戻ると、検索パネルがクエリも含めて開いた状態のまま復元される
- [ ] タブを閉じればその UI 状態も破棄される
- [ ] プレビュー / プラグインパネルも同じ挙動

---

### 12.4 実装優先度
1. 12.1 検索ボックス自動フォーカス（最小工数で体感大）
2. 12.2 タブ移動ホットキー
3. 12.3 タブ単位 UI 状態（store 構造変更を伴うため最後）

---

## 13. 追加改善計画（v1.3 向け）

### 13.1 タブのマウス D&D 並べ替え（不具合修正）
**現状**: `VerticalTabs.tsx` には既にタブ用の HTML5 D&D 実装（`draggable=true` / `onDragStart` / `onDragOver` / `onDrop`）が入っており、`reorderTab(fromId, toIndex)` も用意されているが、実機ではマウスでタブを掴んで動かせない。

**原因（最有力）**:
- Tauri 2 のウィンドウは既定で **OS ドラッグドロップハンドラ (`dragDropEnabled: true`)** が有効。
- これが Win32 レベルで `WM_DRAGENTER` / `WM_DROP` 等を奪うため、WebView 内の HTML5 D&D の `dragstart`/`dragover`/`drop` が発火しない / `dataTransfer` が機能しないことがある。
- アプリ内ファイル D&D（行 ⇄ ペイン ⇄ タブ ⇄ パンくず）は全て **WebView 内完結** で十分なので、OS 連携の D&D は無効化して良い。

**実装**:
1. `src-tauri/tauri.conf.json` の windows[0] に `"dragDropEnabled": false` を追加
2. 動作確認:
   - タブを掴んで上下にドラッグ → 挿入位置インジケータ (`drop-before`) が表示される
   - ドロップで `reorderTab` が呼ばれ、順序が更新・persist される
   - 既存の行/ペイン/パンくず/ファイル D&D も依然動作する
3. 副作用確認:
   - エクスプローラから FastFiler のウィンドウ上にファイルをドロップ → 当面は無視される (要件外)
   - 必要なら将来 Tauri の `webview.on_drop()` をフロントの listenSearchHit と同様に listen→ コピー処理に流す（この PR では対応しない）

**追加 UX 改善（同フェーズで実施）**:
- ドラッグ中のタブを **半透明** 表示にする（`.vtab.dragging { opacity: .4 }`）
- ドロップ位置インジケータをよりはっきり (2px → 3px / アクセントカラー)
- タブ間に細い隙間を設けて、上半分／下半分判定を視覚的に補助
- ドラッグ中は `cursor: grabbing`、通常時 `cursor: grab` を `.vtab` に付与

**受入基準**:
- [ ] タブをマウスで掴むとドラッグ開始（カーソルが grabbing に変化）
- [ ] 別タブの上にホバーすると挿入位置にインジケータが表示
- [ ] ドロップでタブ順が変更される（再起動後も保持）
- [ ] アクティブタブ・連動グループ設定はドロップ後も維持される
- [ ] 行ファイルの別ペイン/パンくず/タブへの D&D が壊れていない

### 13.2 用語の整理（ドキュメント）
"タブ" の指す対象が複数あるため、USAGE.md / plan.md の見出しを以下に統一:
- 「**タブ移動 (キーボード)**」… `Ctrl+Tab` 等のフォーカス切替
- 「**タブの並べ替え**」… マウスで掴んで順序変更
- 「**タブ間移動 (D&D)**」… 行ファイルを別タブに落として移動

---

## 14. 追加改善計画（v1.4 向け）

### 14.1 ワークスペースのドッキング配置
**現状のレイアウト**（左→右）:
```
[タブサイドバー] [メインペイン (タブ詳細)] [プレビュー] [プラグイン]
```
タブ ↔ メインペインの位置を入れ替えたい、加えて将来的には **ドラッグで上下左右に貼り付け** たい。

**段階的に実装**:

#### Phase A — レイアウトプリセット切替 (まず実装)
状態として **`workspaceLayout`** を持ち、ヘッダーまたは設定ダイアログから切替:
- `tabsLeft` (既定) — `[Tabs] [Main] [Tree?] [Preview] [Plugin]`
- `tabsRight` — `[Tree?] [Main] [Tabs] [Preview] [Plugin]`
- `tabsTop` — タブを横並びでメインの上に（横タブ）
- `tabsBottom` — 同じく下に
- `tabsHidden` — タブサイドバー自体を表示しない（`Ctrl+Tab` 等のキーボード操作のみで運用）

各パネル個別の表示/非表示トグルもヘッダーに：
- 👁 タブ表示 ON/OFF
- 🌲 ツリー表示 ON/OFF
- 🔍 プレビュー表示 ON/OFF (既存)
- 🧩 プラグイン表示 ON/OFF (既存)

設定ダイアログ「基本」タブに **レイアウト** セクションを追加して同じ操作を提供。

**実装ポイント**:
- `state.workspace`: `{ layout: "tabsLeft"|"tabsRight"|"tabsTop"|"tabsBottom"|"tabsHidden", showTree: boolean, tabsWidth: number, treeWidth: number }`
- `App.tsx` の `app-body` の grid-template を `workspace.layout` に応じて生成
- 各サイドバーは `pointerdown` 縁ドラッグで幅変更可（既存 splitter 流用検討）
- ホットキー: `Ctrl+B`（タブサイドバー表示切替）/ `Ctrl+Shift+E`（ツリー表示切替）（VSCode 風）

#### Phase B — ドラッグ＆ドロップでドッキング (将来)
VSCode の Activity Bar / Side Panel のように、各パネルを **タイトルバーごと掴んで** 任意の縁にドロップして再配置。
- 中央ペインの 4 辺（上下左右）と中央にドロップゾーンを表示（オーバーレイ）
- 落とした位置で `workspace.layout` を更新
- 工数大なので Phase A の運用感を見てから着手

---

### 14.2 ワークスペースレベルのツリーペイン
**現状**: ツリー表示は **ペイン内の表示モード切替**（リスト ⇄ ツリー）として実装。タブ詳細ペインそのものをツリーに置き換える形。

**追加**: それとは別に、**ワークスペース全体で常時表示できる "ツリーパネル"** を新設。Explorer 左ペインのような使い心地。

| 既存 | 新設 |
|---|---|
| `PaneState.view = "tree"` でペイン内をツリー化（行単位の表示モード） | **WorkspaceTreePanel** — タブと並列に並ぶ独立パネル |
| 1 ペイン = 1 ツリー | パネル全体で 1 ツリー、選択をアクティブペインに反映 |
| そのペインのみに影響 | アクティブタブ／指定連動グループへ反映 |

**仕様**:
- ヘッダー or 設定で **ツリーパネル ON/OFF**（既定 OFF）
- ルート: ドライブ一覧 (`[C:\] [D:\] ...`) を一括表示、各ノードを遅延展開
- 選択ノードクリック → アクティブタブの「左端 (or 全)」ペインへパス反映
- ピン留め（よく使うフォルダを上部固定）
- 幅は左右ドラッグでリサイズ、最小 160px / 最大 600px
- タブサイドバーと配置プリセット (`14.1`) で並ぶ
- 既存の per-pane `view: "tree"` モードはそのまま維持（混乱しないよう設定タブで「ツリーパネルとペイン内ツリーは別物」と明記）

**実装**:
- 既存の `TreeView.tsx` を **コンポーネント分離** し、ルート path・選択クリック時の挙動 (`onSelect`) を props 化
  - 既存使用箇所 (PaneTree 経由) はそのまま動作
  - 新規 `WorkspaceTreePanel.tsx` でドライブ列挙 → 各ドライブを Tree のルートとして並列展開
- `App.tsx` の `<Show when={state.workspace.showTree}>` で表示
- 連動: ツリー選択時の挙動を `apply: "active-pane" | "link-red" | "link-blue"` で選択可能（設定 → 連動タブに項目追加）

---

### 14.3 実装優先度・受入基準

#### 優先順
1. **14.1 Phase A** レイアウトプリセット (tabs の左右入替・各パネル表示切替) — 最小工数で要望を満たす
2. **14.2 ワークスペースツリーパネル** (Phase A 完了後に乗せる)
3. **14.1 Phase B** ドラッグ&ドロップドッキング — 将来検討

#### 受入基準
- [ ] 設定 or ヘッダーから「タブを右側」「タブ非表示」が切替えられ、再起動後も保持
- [ ] ヘッダー / 設定でツリーパネル ON/OFF、幅を任意に変更できる
- [ ] ツリーパネルのフォルダクリックで、アクティブタブのペインがそのパスを開く
- [ ] 既存の per-pane ツリー表示・per-pane 検索状態などが影響を受けない
- [ ] `Ctrl+B` でタブサイドバー、`Ctrl+Shift+E` でツリーパネルの表示を切替できる
- [ ] レイアウト変更時にペインの内容（path / 選択 / スクロール）が保持される

#### 用語整理（USAGE 反映）
- **タブサイドバー** — 縦タブの一覧パネル（位置可変・非表示可）
- **メインペイン** — タブの詳細を表示する中央領域（必須・常時表示）
- **ツリーパネル** — ワークスペース全体のツリービュー（オプション）
- **ペイン内ツリー** — タブ詳細内で 1 ペインだけツリー表示する既存機能（別物）
