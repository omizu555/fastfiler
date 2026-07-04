#!/usr/bin/env python3
"""Phase 2: assemble wbs.json from the chapter map + chapter definitions."""
from __future__ import annotations
import json
from pathlib import Path

CC = Path(".cc-rsg")
cmap = json.loads((CC / "chapter_inv_map.json").read_text(encoding="utf-8"))

DEFS = [
    ("ch-01", "第1章: 概要", "01-overview.md",
     ["crates/fastfiler-domain/src/lib.rs", "crates/fastfiler-gpui/src/main.rs",
      "crates/fastfiler-gpui/Cargo.toml", "crates/fastfiler-domain/Cargo.toml", "README.md"]),
    ("ch-02", "第2章: アーキテクチャ", "02-architecture.md",
     ["crates/fastfiler-domain/Cargo.toml", "crates/fastfiler-gpui/Cargo.toml",
      "crates/fastfiler-domain/src/lib.rs", "crates/fastfiler-gpui/src/main.rs",
      "crates/fastfiler-gpui/build.rs", "Cargo.toml"]),
    ("ch-03", "第3章: 状態モデルとリアクティビティ", "03-state-model.md",
     ["crates/fastfiler-gpui/src/app.rs", "crates/fastfiler-gpui/src/pane.rs",
      "crates/fastfiler-gpui/src/tree.rs", "crates/fastfiler-domain/src/events.rs",
      "crates/fastfiler-gpui/src/sink.rs"]),
    ("ch-04", "第4章: ドメイン層 — ファイルシステムとファイル操作", "04-domain-fs.md",
     ["crates/fastfiler-domain/src/fs.rs", "crates/fastfiler-domain/src/file_ops.rs",
      "crates/fastfiler-domain/src/file_jobs.rs", "crates/fastfiler-domain/src/watcher.rs"]),
    ("ch-05", "第5章: ドメイン層 — Windows シェル統合", "05-domain-shell.md",
     ["crates/fastfiler-domain/src/shell.rs", "crates/fastfiler-domain/src/shell_assoc.rs",
      "crates/fastfiler-domain/src/win_clipboard.rs", "crates/fastfiler-domain/src/icons.rs",
      "crates/fastfiler-domain/src/ole_dnd.rs"]),
    ("ch-06", "第6章: ドメイン層 — 検索・テンプレート・ユーザーコマンド・Undo", "06-domain-services.md",
     ["crates/fastfiler-domain/src/search.rs", "crates/fastfiler-domain/src/everything.rs",
      "crates/fastfiler-domain/src/templates.rs", "crates/fastfiler-domain/src/user_commands.rs",
      "crates/fastfiler-domain/src/undo.rs", "crates/fastfiler-domain/src/ascii_tree.rs",
      "crates/fastfiler-domain/src/path_util.rs", "crates/fastfiler-domain/src/error.rs"]),
    ("ch-07", "第7章: GUI 層 — アプリシェルとレイアウト", "07-gui-app.md",
     ["crates/fastfiler-gpui/src/main.rs", "crates/fastfiler-gpui/src/app.rs"]),
    ("ch-08", "第8章: GUI 層 — ペイン", "08-gui-pane.md",
     ["crates/fastfiler-gpui/src/pane.rs"]),
    ("ch-09", "第9章: GUI 層 — ワークスペースツリーとテキスト入力", "09-gui-tree-input.md",
     ["crates/fastfiler-gpui/src/tree.rs", "crates/fastfiler-gpui/src/text_input.rs"]),
    ("ch-10", "第10章: テーマと設定", "10-theme-settings.md",
     ["crates/fastfiler-gpui/src/theme.rs", "crates/fastfiler-gpui/src/settings_store.rs",
      "crates/fastfiler-gpui/src/hotkeys.rs"]),
    ("ch-11", "第11章: 永続化とセッション", "11-persistence-session.md",
     ["crates/fastfiler-gpui/src/persist.rs", "crates/fastfiler-gpui/src/session.rs",
      "crates/fastfiler-gpui/src/win32_single_instance.rs"]),
    ("ch-12", "第12章: 横断的関心事 — 性能・セキュリティ・運用性", "12-cross-cutting.md",
     ["crates/fastfiler-gpui/src/pane.rs", "crates/fastfiler-gpui/src/app.rs",
      "crates/fastfiler-gpui/src/persist.rs", "crates/fastfiler-domain/src/user_commands.rs",
      "crates/fastfiler-domain/src/shell.rs", "crates/fastfiler-domain/src/ole_dnd.rs",
      "crates/fastfiler-domain/src/watcher.rs"]),
]

chapters = []
for cid, title, fn, key_files in DEFS:
    chapters.append({
        "chapter_id": cid,
        "chapter_title": title,
        "file_name": fn,
        "kind": "standard",
        "depth_mode": "comprehensive",
        "key_files": key_files,
        "assigned_inventory_ids": cmap.get(cid, []),
        "status": "pending",
    })

for cid, title, fn in [
    ("ch-00-metadata", "メタデータ", "00-metadata.md"),
    ("ch-99-unresolved", "第99章: 未確定事項", "99-unresolved.md"),
    ("ch-traceability", "トレーサビリティ表", "traceability.md"),
]:
    chapters.append({
        "chapter_id": cid,
        "chapter_title": title,
        "file_name": fn,
        "kind": "reserved",
        "depth_mode": "comprehensive",
        "key_files": [],
        "assigned_inventory_ids": [],
        "status": "pending",
    })

wbs = {
    "template": {"name": "claude-custom-desktop-gui", "version": "0.1.0"},
    "depth_mode": "comprehensive",
    "chapters": chapters,
}
(CC / "wbs.json").write_text(json.dumps(wbs, ensure_ascii=False, indent=2), encoding="utf-8")
print(f"wbs.json: {len(chapters)} chapters")
for c in chapters:
    print(f"  {c['file_name']:28s} {c['kind']:9s} inv={len(c['assigned_inventory_ids'])}")
