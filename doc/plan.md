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

---

## 15. v1.5 改善計画

### 15.1 単一インスタンス化 (single-instance)
- **目的**: アプリを 2 重に起動しない。2 回目以降は既存ウィンドウをフォーカス＋復元する。
- **方針**: `tauri-plugin-single-instance` (v2) を導入
  - `src-tauri/Cargo.toml` に依存追加
  - `lib.rs` の Builder で `.plugin(tauri_plugin_single_instance::init(callback))` を呼び、コールバック内でメインウィンドウを unminimize + set_focus
  - 起動引数で渡されたパスがあれば、新規タブで開く（任意拡張）
- **検証**: ビルド後 `FastFiler.exe` を 2 つ起動 → 1 つ目が前面化されることを確認

### 15.2 メインペイン → ツリーパネルの追従
- **目的**: ペインで cd したら WorkspaceTreePanel もそのフォルダを自動展開＆ハイライトする。
- **現状**: クリック時 `applyPathToTargets` でツリー → ペインの一方向しかない。逆方向の自動展開無し。
- **方針**:
  - `WorkspaceTreePanel` 内に `createEffect` を追加し「アクティブ leaf pane の path」を購読
  - 変更検知時、`expanded` セットに祖先パス全てを追加（ドライブから順に）
  - 既存 `current` highlight はそのまま機能
- **副次**: ペイン内 TreeView (`TreeView.tsx`) の createEffect は既に同様の追従があるので変更不要

### 15.3 Backspace でドライブ一覧へ
- **目的**: ドライブルート（例 C:\）で Backspace を押したら「コンピュータ」相当の画面に遷移したい。
- **方針**:
  - 仮想パス `"::drives"` を導入（型上は string、Rust 側へは送らない）
  - `pane.path === "::drives"` のとき FileList の代わりに **DriveListView** を表示
    - listDrives の結果を大きめのアイコン＋ラベル＋空き容量で並べる
    - クリック / Enter で対応ドライブを setPanePath
  - parentPath の動作:
    - ドライブルートで呼ばれたら `"::drives"` を返す
    - `"::drives"` で呼ばれたら そのまま（それ以上戻れない）
  - パンくず・タイトル・ツリーパネル側で `"::drives"` 表示を「💻 PC」相当に整形
  - URL ナビゲーション欄入力でも `pc` `:drives` `computer` などのキーワードで遷移可能にする

### 15.4 テーマ切替 (OS / Dark / Light)
- **目的**: 白背景派ユーザーのため、3択で簡単に切り替え。
- **方針**:
  - `state.theme: "system" | "dark" | "light"` を追加（既定 system）
  - CSS は :root (=dark) と `:root[data-theme="light"]` のオーバーライドで実装
    - --bg, --bg2, --fg, --border, --muted, --accent, --row-hover, --row-selected の各変数を上書き
  - system のとき window.matchMedia("(prefers-color-scheme: light)") を購読し data-theme を切替
  - 設定ダイアログ「基本」最上部に追加 (ラジオ 3 択)
- **対象**: 全コンポーネント（既存ハードコード色 #2d5fa3 などは accent 変数化検討、ただし最低限 styles.css の variables 化で十分）

### 15.5 ペイン名（leaf pane のラベル）
- **目的**: 分割した各ペインに識別用の名前を付けたい（タブ・ツリーは対象外、フォルダ表示ペインのみ）。
- **方針**:
  - `PaneState.name?: string` を追加
  - ペインのツールバーにラベル領域を新設し、ダブルクリックで編集（input 化）
    - 表示優先度: pane.name > フォルダ名 (basename(path)) > "(空)"
  - 設定なし（null）のときは自動表示なので、ユーザーが付けたときだけ強調
  - localStorage 永続化（既存 paneState 構造に乗る）
  - ペイン分割時には親の name を継承しない（各ペイン独立）

### 15.6 Ctrl+F のトグル化
- **目的**: 開いているとき Ctrl+F 再押下で閉じる。
- **現状**: focusPaneSearch が「開く＋フォーカス」だけ。
- **方針**:
  - 新ヘルパー togglePaneSearchFocused(paneId):
    - 開いていたら閉じる、閉じていたら開く（シンプル案採用、フォーカス状態に関わらず）
  - PaneTree / FileList 経由で各ペインのこのアクションを呼ぶ

### 15.7 受入基準
- 多重起動 → 既存ウィンドウ前面化
- ペインで cd → ツリーパネルが該当パスまで自動展開
- Backspace 連打 → C:\ → ::drives → そこから Enter で別ドライブへ
- 設定 → テーマ Light に変更すると即時白背景化、再起動後も維持
- ペインヘッダーをダブルクリック → 編集 → 保存（タブ名とは独立）
- Ctrl+F 1 回目: 検索オープン＋フォーカス、2 回目: 閉じる、3 回目: 再オープン

### 15.8 todos
- v15-single-instance
- v15-tree-follow-pane
- v15-backspace-drives
- v15-theme-switch
- v15-pane-name
- v15-search-toggle

---

## 16. 追加改善計画（v1.6 向け）

実機操作で挙がった UX 改善要望をまとめる。マウス操作の強化と、ロックタブ・ナビゲーション履歴・通知 UI 周りの仕上げ。プラグインで実現可能な拡張も検証する。

### 16.1 ファイルペインの矩形範囲選択 (rubber-band)

**目的**: マウスドラッグで矩形を描き、複数ファイルを一括選択する。

**現状**: 行クリック / `Ctrl+クリック` / `Shift+クリック` のみ。空白部分からドラッグしても何も起こらない。

**仕様**:
- ファイルリストの **空白部分** で `mousedown` → `mousemove` で半透明の矩形オーバーレイを描画
- ドラッグ中に矩形と交差する行を選択（または既存選択に追加）
- 修飾キー:
  - 修飾なし → 既存選択を破棄して新規選択
  - `Ctrl` → 既存選択にトグル追加
  - `Shift` → 既存選択に追加（差し引きなし）
- 行 (`tr`) 上から始まったドラッグは矩形選択にせず、既存の D&D 開始扱い (`draggable=true`)
- スクロール: 矩形がリスト境界に達したら自動スクロール（簡易: setInterval で edge 判定）
- ドラッグ中も キーボード操作（PageUp/Down 等）はブロックしない

**実装ポイント**:
- [src/components/FileList.tsx](src/components/FileList.tsx) の `<table>` を内包する `.file-list` コンテナに `onMouseDown` を追加
- イベントターゲットが `tr` 上 (行内) ならスキップ。空白 (`tbody` 直下や `vpad`、テーブル外側) のみ開始
- 開始時刻と座標を保持。`window` に `mousemove` / `mouseup` を一時バインド
- 矩形 div は仮想スクロール領域の position:absolute で描画
- 行の bbox は `data-rd-name` を持つ `<tr>` を `getBoundingClientRect` で取得（仮想化されているのは画面内行のみなので軽量）
- スクロール外に出た行は判定対象外（実用上問題なし、必要なら item index ベースで補完）
- 既存の `setPaneSelection` / `togglePaneSelectionAdd`（無ければ新設）を経由して selection を更新

**受入基準**:
- 空白部分から左ドラッグで半透明矩形が描かれる
- ドラッグ範囲内の行が即時ハイライトされ、`mouseup` で確定する
- `Ctrl+ドラッグ` で既存選択に対してトグル追加できる
- 行から始まるドラッグは従来通り D&D が動作（矩形は出ない）
- 矩形が表示領域端に達するとリストが自動スクロールする

---

### 16.2 ペイン内 D&D で同ペインのフォルダへ移動・コピー

**現状**: フォルダ行は drop target として実装済み ([src/file-list/dnd.ts](src/file-list/dnd.ts)) で、別ペインからのドロップは動作する。同一ペイン内でのフォルダ行へのドロップも `handleDrop` で同パスかチェック後に成立する設計だが、**実機で動かない / フォルダ自身を掴んでも動かないケースがある**との報告。

**目的**:
- ファイルでもフォルダでも、同じペイン内の **別フォルダ行** にドロップしたら移動 / コピー (`Ctrl`) できる
- フォルダ自身も draggable で、別フォルダの中へ入れられる
- ペインの空白部分にドロップしたときは何もしない（または将来「現在フォルダへ」を割り当て可能）

**確認 / 修正項目**:
1. `<tr draggable={true}>` がフォルダ行でも有効か確認 → onDragStart の sel に当該フォルダ名が含まれることを確認
2. `onRowDragOver`: `entry.kind !== "dir"` で早期リターンしているため、ファイル行の上は drop 不可になっているのは正しい挙動
3. **同一ペイン内ドロップ判定**: `handleDrop` 内で `payload.sourcePath === destPath` (=同フォルダ) のときコピーのみ通している。今回は **destPath が現在のフォルダ内のサブフォルダ** なので payload.sourcePath !== destPath となるはずで、意図通り動く想定。動かない場合は dataTransfer.types に DRAG_MIME が乗っていない可能性 → `setData` 順を見直す
4. **フォルダ自身を自分自身に入れない** ガード: `items.from === items.to` または `to` が `from` 配下になるケースを `runFileJob` 前に検出して reject
5. ドラッグ中、自分自身および子孫フォルダ行は drop インジケータを抑制
6. **同ペイン D&D テスト**: 別ペインを開かずに、1 ペイン内で `A/file.txt` を `A/sub/` に落とせるか手動 + (可能なら) 自動テスト

**実装ポイント**:
- [src/file-list/dnd.ts](src/file-list/dnd.ts) `onRowDragOver` で「自分自身/選択中の項目」へのドロップを `dropEffect="none"` にする
- `handleDrop` の最初に `items.some(i => i.from === i.to || i.to.startsWith(i.from + "\\"))` を弾く
- `pushToast("自分自身には移動できません", "warn")` で通知

**受入基準**:
- 同ペイン内でファイルをサブフォルダ行にドラッグ → 移動 (Ctrl でコピー)
- 同ペイン内でフォルダをサブフォルダ行にドラッグ → 移動 (Ctrl でコピー)
- 自分自身にドロップしようとすると禁止カーソル＋トーストで通知
- 親フォルダを子フォルダにドロップしようとすると同様に拒否される

---

### 16.3 通知トーストをステータスバーへ移設

**目的**: 右下のフローティングトーストが視界を遮るので、画面下部の固定ステータスバー内に控えめに表示したい。

**現状**: `ToastContainer` は absolute 配置で右下に積み重なる。

**仕様**:
- ステータスバー右側に **最新 1 件** の短いメッセージを表示
- メッセージは fade-in、5 秒後 fade-out （現状の自動消去ロジック流用）
- 重要度 (info / warn / error) でアイコン or 色分け（小さく）
- アクション付き (例: 「↶ 取り消し」) のものはボタンも横に並べる
- バッジ:「未読の通知が複数あった場合」は `+N` の小バッジを付け、クリックでフローティングの履歴ポップオーバーを展開
- 設定で **toast.position: "statusbar" | "bottom-right"** を切替可能（既定は statusbar）

**実装ポイント**:
- [src/components/ToastContainer.tsx](src/components/ToastContainer.tsx) を `mode` prop 化、または **StatusBarToast** という別コンポーネントを新設
- [src/App.tsx](src/App.tsx) のステータスバー footer 内に StatusBarToast をマウント、フローティングは `state.settings.toastPosition === "bottom-right"` のときだけ
- 履歴ポップオーバーは既存トースト一覧をそのまま縦に表示する小ウィンドウ
- `state.settings.toast` を `{ position, autoDismissMs }` 型で導入

**受入基準**:
- コピー / 移動完了でステータスバー右に「コピー 3件 完了 ↶取り消し」が出る
- 5 秒で消える、複数発生したら最新が出て古い件数は `+N` で示される
- 設定で右下フローティングに戻せる

---

### 16.4 マウスサイドボタンによる戻る / 進む

**目的**: マウスの第 4 / 第 5 ボタン（サイドボタン）でフォルダ履歴を戻る・進む。

**現状**: ペイン履歴の概念がそもそも未実装。`setPanePath` は単に書き換えるだけ。

**設計**:
- `PaneState` に `history: string[]` と `historyIndex: number` を追加
- `setPanePath(paneId, path, opts?: { fromHistory?: boolean })`:
  - 通常: history を historyIndex まで切り詰めてから push、historyIndex を進める
  - `fromHistory: true` のときは push しない
- ヘルパー:
  - `navigateBack(paneId)` → historyIndex を -1 にして対応 path を `setPanePath(..., { fromHistory: true })`
  - `navigateForward(paneId)` → 逆
  - `canGoBack(paneId)`, `canGoForward(paneId)`
- 履歴の最大長: 64 件で先頭から破棄
- 各タブを跨ぐ履歴は持たず **ペインローカル**

**入力ハンドリング**:
- `window.addEventListener("mousedown", ...)`:
  - `e.button === 3` (XButton1) → focusedLeafPane に対して back
  - `e.button === 4` (XButton2) → 同 forward
  - `e.preventDefault()` でブラウザ既定 (history.back) を抑制
- ホットキー併設:
  - `Alt+Left` → back, `Alt+Right` → forward (`hotkeys.ts` に `pane-back` / `pane-forward` を追加)
- ペインのツールバーに `← / →` ボタンを追加（disabled 状態を `canGoBack/Forward` で連動）

**受入基準**:
- ペインで複数階層を移動した後、サイドボタンで前のフォルダに戻れる
- ロックタブ (16.5 参照) では back / forward でもパスを書き換えない（タブを動かさない）
- ペイン分割時、履歴は各ペイン独立
- 履歴上限を超えたら古いものから削除される

---

### 16.5 タブロックの強化

**目的**: ロックタブを「閲覧固定」として完全に動かなくする。フォルダクリックで誤って深く入ってしまう事故を防ぐ。

**現状**: ロック中は close を抑止するのみ。フォルダのダブルクリックや Backspace / 履歴操作で path は変わってしまう。

**仕様**:
ロック中タブでは **path を変える操作すべて** を遮断、または **新しいタブを開いてそちらで実行** する:

| 操作 | ロック中の挙動 |
|---|---|
| ファイルダブルクリック | 既存通り (関連付けで開く) |
| **フォルダダブルクリック** | **新規タブを開いて、そこへ navigate** |
| Enter (フォルダ選択) | 同上 |
| Backspace / 親フォルダ ↑ ボタン | 拒否 (toast でロック中である旨表示。必要なら新規タブへ) |
| パンくずクリック | 拒否 (同上) |
| アドレスバー編集確定 | 拒否 (同上) |
| ツリーパネル / ペイン内ツリーから選択 | 新規タブへ navigate |
| サイドボタン (back/forward) / `Alt+Left/Right` | 拒否 |
| 検索結果からのジャンプ (Enter) | 新規タブへ navigate |
| プラグインからの `setPanePath` | 拒否 (戻り値で警告)、または 新規タブへフォールバック |

**実装ポイント**:
- 中央集約のため `setPanePath(paneId, path)` の入口で「対象タブが locked か」をチェック
- locked なら `openInNewTab(path)` にフォールバック（または noop + toast）
- `openInNewTab(path)`: 既存の `openTab(path)` を呼ぶだけ。連動グループは **継承しない**（独立タブ）
- どの操作で「新規タブ」「拒否」かは細かく分岐したいので、新ヘルパー `navigateOrSpawn(paneId, path, opts?: { reason?: "user" | "plugin" })` を導入し、各呼び出し元から経由させる
- ロック中タブは見た目上、ペインのアドレスバー / 親フォルダボタン / パンくずを **disabled スタイル** にして触れないことを示す
- `toggleTabLock` でロック解除した瞬間、ペインの操作を有効化

**受入基準**:
- ロック中タブでフォルダをダブルクリック → 新規タブが開き、そのタブで該当フォルダが開く（ロック中タブは元のまま）
- Backspace / アドレスバー / パンくずで path を変えようとすると拒否＋ "ロック中のため変更できません" トースト
- ツリーパネルからロック中タブのペインへ反映しようとすると、新規タブが開く
- ロック解除後は通常操作で path が変わる
- 検索 / プラグインからの遷移も同じルール

---

### 16.6 プラグイン: 空白ダブルクリックで親フォルダへ + 右クリックジェスチャ

**目的**: プラグイン API のみで、以下のような機能を **公式に追加することなく** 実装可能か検証する。

**A. 空白部分ダブルクリックで親フォルダへ**
- ペイン空白の `dblclick` で `parentPath(currentPath)` を `setPanePath`
- ロックタブの場合は新規タブで開く（16.5 のフックを再利用）

**B. 右クリックジェスチャでナビゲーション**
- 右ボタンを押して **右へ短くスワイプ** → forward、**左へ** → back、**上へ** → 親フォルダ、**下へ** → drives
- 既存の右クリックメニューと衝突しないよう、移動距離が閾値 (16px) 未満なら通常コンテキストメニュー

**プラグイン API 追加要否の判定**:

現在の `ff` API で実現可能か:
- 必要: pane の HTML 要素 / イベントハンドラへのフック
- 既存:
  - `ff.on("pane.changed")` だけでは DOM イベントは取れない
  - `pane.getActive()` / `pane.setPath()` は揃っている

**結論**: **そのままでは無理**。以下のいずれかの拡張が必要。

#### 案1 (推奨): プラグインから「ホストイベントフック」を購読する API を追加
- [doc/plugins-sample/sdk.d.ts](doc/plugins-sample/sdk.d.ts) を拡張:
  ```ts
  ff.host.on("pane.dom.dblclick", (e: { paneId: string; target: "row"|"empty"|"header"; ... }) => void);
  ff.host.on("pane.dom.contextgesture", (e: { paneId: string; direction: "up"|"down"|"left"|"right"; distance: number }) => void);
  ```
- ホスト側 (FastFiler 本体) で DOM イベントを正規化してプラグインへ emit
- プラグインは購読し、`ff.pane.setPath` で navigate

#### 案2: 公式機能として 16.6A / 16.6B を本体実装する
- プラグイン拡張せずに本体に組み込み (オプションで ON/OFF)
- A は工数小、B はジェスチャ判定の調整必要

**方針**: **案1 + サンプルプラグイン提供** とする。
- 本体: ペインに必要な DOM イベントのフック点 (dblclick / pointerdown～pointerup の右ボタン) を新設し、プラグインへ broadcast
- サンプル: [doc/plugins-sample/](doc/plugins-sample/) に `pane-gestures` フォルダを追加し、上記 A / B を実装した参考プラグインを置く
- 既存プラグインの動作には影響しない（イベント追加のみ）

**実装ポイント**:
- [src/plugin-host.ts](src/plugin-host.ts) に新しいトピック:
  - `pane.dom.dblclick.empty` / `pane.dom.dblclick.row` / `pane.gesture`
- ペイン側で `onDblClick` / 右クリックドラッグの座標差分を計算して emit
- ジェスチャ判定パラメータ (閾値・有効化フラグ) は設定ダイアログ「プラグイン」タブで持つ、もしくはプラグイン側設定 (`storage`) に委ねる
- 安全策: プラグインからの `setPanePath` も 16.5 のロックフックを通す

**受入基準**:
- サンプルプラグインを有効化すると、空白ダブルクリックで親フォルダへ移動できる
- 右ボタンを押して右にスワイプ → 進む / 左 → 戻る / 上 → 親フォルダ
- スワイプ距離が閾値未満ならジェスチャ判定されず通常の右クリックメニューが出る
- ロック中タブでは新規タブにフォールバックする

---

### 16.7 実装優先度

| 順 | 項目 | 理由 |
|---|---|---|
| 1 | 16.4 マウスサイドボタン (履歴) | 16.5 のロック挙動の前提となる「履歴」概念を入れる |
| 2 | 16.5 タブロック強化 | 多くの操作経路に手を入れるため、早めに枠組みを作る |
| 3 | 16.2 ペイン内 D&D の確認・修正 | 既存実装の挙動確認＋ガード追加が中心、軽量 |
| 4 | 16.1 矩形範囲選択 | 単独の新機能、他に影響少 |
| 5 | 16.3 トーストをステータスバーへ | UI 側の変更のみ |
| 6 | 16.6 プラグイン拡張 | API 追加 + サンプル、最後に着手 |

### 16.8 受入基準サマリ

- [ ] 矩形ドラッグで複数ファイルを選択できる (16.1)
- [ ] 同ペイン内でファイル / フォルダをサブフォルダ行にドロップして移動・コピーできる (16.2)
- [ ] 自分自身/親→子の禁止移動が拒否される (16.2)
- [ ] 通知がステータスバーに小さく出るようになる、設定で従来位置に戻せる (16.3)
- [ ] マウスのサイドボタン / `Alt+Left/Right` でペイン履歴を行き来できる (16.4)
- [ ] ロックタブはあらゆる経路で path が変わらない、フォルダダブルクリックは新規タブで開く (16.5)
- [ ] サンプルプラグインで空白ダブルクリック→親、右ボタンスワイプで履歴/親が動く (16.6)

### 16.9 todos

- v16-rubber-band-select
- v16-pane-internal-dnd-fix
- v16-toast-statusbar
- v16-mouse-side-buttons
- v16-tab-lock-strict
- v16-plugin-pane-gesture-events
- v16-plugin-sample-gestures

