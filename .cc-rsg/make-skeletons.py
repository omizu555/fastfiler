#!/usr/bin/env python3
"""Phase 2: create near-empty chapter skeletons under .cc-rsg/drafts/.

Strict skeleton contract: meta comment + JA title + (standard only) a
Sources Read placeholder. <= 5 non-blank body lines. No tables, no [REF:],
no Mermaid, no prose — those are Phase 3 outputs.
"""
from pathlib import Path

D = Path(".cc-rsg/drafts")
D.mkdir(parents=True, exist_ok=True)

STD = [
    ("01-overview.md", "第1章: 概要",
     "Overview - what FastFiler is, core identity, scope, non-goals, crate split"),
    ("02-architecture.md", "第2章: アーキテクチャ",
     "Architecture - workspace/crate structure, domain/gui boundary, vendored GPUI, build"),
    ("03-state-model.md", "第3章: 状態モデルとリアクティビティ",
     "State model & reactivity - Entity tree, TabState/PaneNode BSP, reactivity, memory lifecycle, EventSink bridge"),
    ("04-domain-fs.md", "第4章: ドメイン層 — ファイルシステムとファイル操作",
     "Domain: filesystem & file ops - fs/file_ops/file_jobs/watcher"),
    ("05-domain-shell.md", "第5章: ドメイン層 — Windows シェル統合",
     "Domain: Windows shell integration - shell/shell_assoc/win_clipboard/icons/ole_dnd"),
    ("06-domain-services.md", "第6章: ドメイン層 — 検索・テンプレート・ユーザーコマンド・Undo",
     "Domain: search/templates/user_commands/undo - search/everything/templates/user_commands/undo/ascii_tree/path_util/error"),
    ("07-gui-app.md", "第7章: GUI 層 — アプリシェルとレイアウト",
     "GUI: app shell & layout - main/app (tabs, BSP layout, resize, settings, tree integration)"),
    ("08-gui-pane.md", "第8章: GUI 層 — ペイン",
     "GUI: pane - pane.rs (listing/selection/ops/modal/menu/D&D/search/undo/watcher)"),
    ("09-gui-tree-input.md", "第9章: GUI 層 — ワークスペースツリーとテキスト入力",
     "GUI: workspace tree & text input - tree/text_input"),
    ("10-theme-settings.md", "第10章: テーマと設定",
     "Theming & settings - theme/settings_store/hotkeys"),
    ("11-persistence-session.md", "第11章: 永続化とセッション",
     "Persistence & session - persist/session/win32_single_instance"),
    ("12-cross-cutting.md", "第12章: 横断的関心事 — 性能・セキュリティ・運用性",
     "Cross-cutting: performance / security / operability"),
]

RESERVED = [
    ("00-metadata.md", "メタデータ",
     "Phase 6 writes goal.json snapshot / generation timestamp / commit hash / template selection here"),
    ("99-unresolved.md", "第99章: 未確定事項",
     "Phase 6 aggregates abandoned entries from questions.json here"),
    ("traceability.md", "トレーサビリティ表",
     "Phase 6 writes the chapter/section -> source mapping table here"),
]

for fn, title, meta in STD:
    (D / fn).write_text(
        f"<!-- meta: {meta} -->\n\n# {title}\n\n## Sources Read\n\n(Phase 3 で記入予定)\n",
        encoding="utf-8")

for fn, title, meta in RESERVED:
    (D / fn).write_text(
        f"<!-- meta: {meta} -->\n\n# {title}\n\n(Phase 6 で記入予定)\n",
        encoding="utf-8")

print(f"created {len(STD)} standard + {len(RESERVED)} reserved skeletons in {D}")
