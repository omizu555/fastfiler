# メタデータ

本仕様書は cc-rsg（Reverse Spec Generator）により既存コードベースから逆生成された。

## 生成情報

- 生成日時 (UTC): 2026-06-28T13:32:23Z
- 対象コミット: `6513d73355edede02b991189231ce60ebfeb2d12`
- cc-rsg テンプレート: claude-custom-desktop-gui v0.1.0
- depth_mode: comprehensive
- 出力言語: ja

## Phase 0 で確定した目標

- 主要読者: maintenance_developer
- 読者の行動: code_change
- 粒度: medium
- 重視する観点: functional_correctness, performance, operational, security
- 既存ドキュメントの扱い: coexist
- 対象スコープ: crates/fastfiler-domain, crates/fastfiler-gpui（除外: vendor, target）

## 章構成

- 標準章: 12（01〜12）
- 予約章: 00-metadata / 99-unresolved / traceability

## 注記

- 本書のタイムスタンプは生成時に `date -u` で取得した実 UTC 値である。
- Question Bank（73件）は Phase 5 でコード根拠の推論により回答済み。設計意図に関わる項目は 99-unresolved.md に SME 確認推奨として一覧化した。
