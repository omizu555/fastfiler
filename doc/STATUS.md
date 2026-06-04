# 実装ステータス (GPUI 版)

最終更新: 2026-06-04 (floem → GPUI 移植完了時点。経緯は [ADR 0012](./adr/0012-migrate-floem-to-gpui.md)、
詳細な実装ログは [plan-2026-06-03-gpui-migration.md](./plan-2026-06-03-gpui-migration.md) §11)

FastFiler の機能を **「実装済」「採用予定 (未実装)」「不採用 (明示的に持たない)」** の
3 区分で管理する。中核アイデンティティと判断軸は [`/CONTEXT.md`](../CONTEXT.md)、
個別の設計判断は [`adr/`](./adr/) を参照。

> 旧 floem 版 v0.1.0 のステータスは git 履歴
> (`wip(floem): メモリ増殖調査の計装を保全` 以前) を参照。

## 実装済み ✅

### 中核 (CONTEXT.md の優先 1・2)
- 縦タブ (追加 / 切替 / 閉じる / Ctrl+Tab 巡回 / 見出しはフォルダ名追従)
- BSP 任意分割ペイン (↔ ↕ / 閉じる / ドラッグリサイズ / フォーカスペイン=青枠 / F6 巡回)
- 一覧の仮想化描画 (uniform_list — 数万件フォルダでも可視範囲のみ描画)
- **メモリ健全性**: タブ/ペイン開閉で `live panes` がベースラインへ復帰 (常時表示)

### 一覧・操作
- システムアイコン (SHGetFileInfo / 拡張子単位共有) / ソート列切替 (名前/サイズ/種類 ▲▼)
- 複数選択 (Ctrl/Shift クリック・Shift+矢印・Ctrl+A) + cursor/anchor モデル
- 開く (Enter/ダブルクリック=既定アプリ) / 親へ (Backspace) / 更新 (F5)
- ごみ箱削除 (Delete・複数対応) / リネーム (F2) / 新規フォルダ・ファイル (F7/F8)
  — いずれも **IME 対応テキスト入力** モーダル
- コピー / 切り取り / 貼り付け (Ctrl+C/X/V — CF_HDROP、**エクスプローラ相互運用**、
  ジョブ進捗を footer 表示)
- 右クリックメニュー (行 / 背景。貼り付けの活性判定付き)
- watcher による一覧自動更新 (notify、150ms デバウンス、選択は名前で復元)

### パネル・永続化
- ワークスペースツリー (ドライブ起点 / 遅延展開 / クリックでフォーカスペインに開く /
  幅ドラッグ / 表示トグル)
- セッション永続化 (タブ / 分割構成 / 比率 / 各ペインのフォルダ / フォーカス /
  ツリー表示・幅 / ウィンドウ位置サイズ → `%APPDATA%\FastFiler\gpui_session.json`)

### D&D
- ペイン間 D&D (選択全体 / プレビュー / 同一ボリューム=移動・異なる=コピー)
- 外部受信: エクスプローラ → FastFiler (GPUI ネイティブ `ExternalPaths`)

### ビルド・配布
- release ビルド 6.1MB (LTO + strip)、コンソール非表示
- zed フォルダ非依存 (vendor 自己完結、git 依存は async-task patch のみ)

## 未実装 (採用予定)

- D&D 外部送信 (FastFiler → Explorer、ADR 0010) / 右ボタン D&D (ADR 0011 相当)
- フォルダ行への直接ドロップ (内部 D&D)
- ペイン内ツリー / UNC サーバ・share ノード (CONTEXT.md 用語あり)
- 内蔵検索 + Everything 連携の UI (domain 側 API は流用可能な状態)
- ジョブのキャンセル UI (domain 側 `JobRegistry::cancel` はあり)
- Undo (domain 側 `undo.rs` はあり、UI 未配線)
- ユーザーコマンド commands.json / Shift+右クリックのシェルメニュー (ADR 0007)
- テーマ / ホットキーのカスタマイズ、exe アイコン埋め込み、多重起動防止
- アドレスバーのパス直接入力 / 履歴 (戻る・進む)
- 新規ファイルのテンプレート (domain 側 `templates.rs` はあり、UI 未配線)

## 不採用 (ADR 参照)

- プラグイン機構 (0003) / 内蔵ターミナル (0004) / メディアプレビュー (0005)
- ペイン連動 Red/Blue (0001) / ドックスロット複数パネル (0002)
- 旧 floem GUI 実装 (0012 — GPUI へ移行)
