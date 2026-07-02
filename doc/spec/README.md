# FastFiler 仕様書（リバースエンジニアリング生成）

本ディレクトリは、Rust 製 Windows ファイラ **FastFiler**（`fastfiler-domain` ＋ `fastfiler-gpui`）のソースコードから cc-rsg により逆生成した保守者向け仕様書である。

> **スナップショット注記 (2026-07-02)**: 対象はコミット `6513d73` 時点の **GPUI 版**実装。
> iced 移行 ([plan/2026-07-02-iced-rewrite.md](../plan/2026-07-02-iced-rewrite.md)) では
> 「コードレベルの内部挙動」の正典 (第3層) として参照する
> ([plan/2026-07-02-feature-inventory.md](../plan/2026-07-02-feature-inventory.md) の正典層構造を参照)。
> 生成時の作業ファイルは `.cc-rsg/` (未追跡) にある。

## 読み方

1. まず `00-metadata.md` で生成条件（対象コミット・目標）を確認する。
2. `01-overview.md` → `02-architecture.md` で全体像と層構造を掴む。
3. 関心領域に応じて各章へ進む。各記述には `[REF: path:line]` でソース参照が付く。
4. `traceability.md` で章・節とソースの対応を逆引きできる。
5. `99-unresolved.md` で SME 確認推奨の設計判断を確認する。

## 章一覧

| ファイル | 章 |
|---|---|
| 00-metadata.md | メタデータ |
| 01-overview.md | 第1章: 概要 |
| 02-architecture.md | 第2章: アーキテクチャ |
| 03-state-model.md | 第3章: 状態モデルとリアクティビティ |
| 04-domain-fs.md | 第4章: ドメイン層 — ファイルシステムとファイル操作 |
| 05-domain-shell.md | 第5章: ドメイン層 — Windows シェル統合 |
| 06-domain-services.md | 第6章: ドメイン層 — 検索・テンプレート・ユーザーコマンド・Undo |
| 07-gui-app.md | 第7章: GUI 層 — アプリシェルとレイアウト |
| 08-gui-pane.md | 第8章: GUI 層 — ペイン |
| 09-gui-tree-input.md | 第9章: GUI 層 — ワークスペースツリーとテキスト入力 |
| 10-theme-settings.md | 第10章: テーマと設定 |
| 11-persistence-session.md | 第11章: 永続化とセッション |
| 12-cross-cutting.md | 第12章: 横断的関心事 — 性能・セキュリティ・運用性 |
| 99-unresolved.md | 第99章: 未確定事項 |
| traceability.md | トレーサビリティ表 |

## 信頼度マーカーの凡例

- `[CONFIDENCE: HIGH|MED|LOW]` … 記述の確からしさ（コード確認の度合い）
- `[ASSUMED: ...]` … コードから直接読み取れず推測した根拠
- `[ASK SME]` … 設計意図・方針の確認が望ましい箇所（99-unresolved.md に集約）
