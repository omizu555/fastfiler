#!/usr/bin/env python3
"""Build inventory.json from source-map.json with a file->chapter map.

Every Rust item becomes one INV (1 construct = 1 row, concrete type => 0% macro).
`related_source_ids` links each INV to its SRC unit (for MECE traceability).
covered_by is left empty; coverage-check auto-fills it via name/file mention.
"""
from __future__ import annotations
import json
from pathlib import Path

CC = Path(".cc-rsg")
sm = json.loads((CC / "source-map.json").read_text(encoding="utf-8"))

# file basename -> primary chapter id
FILE_CHAPTER = {
    # domain
    "fs.rs": "ch-04", "file_ops.rs": "ch-04", "file_jobs.rs": "ch-04", "watcher.rs": "ch-04",
    "shell.rs": "ch-05", "shell_assoc.rs": "ch-05", "win_clipboard.rs": "ch-05",
    "icons.rs": "ch-05", "ole_dnd.rs": "ch-05",
    "search.rs": "ch-06", "everything.rs": "ch-06", "templates.rs": "ch-06",
    "user_commands.rs": "ch-06", "undo.rs": "ch-06", "ascii_tree.rs": "ch-06",
    "path_util.rs": "ch-06", "error.rs": "ch-06",
    "events.rs": "ch-03",
    "lib.rs": "ch-01",
    # gpui
    "main.rs": "ch-07", "app.rs": "ch-07",
    "pane.rs": "ch-08",
    "tree.rs": "ch-09", "text_input.rs": "ch-09",
    "theme.rs": "ch-10", "settings_store.rs": "ch-10", "hotkeys.rs": "ch-10",
    "persist.rs": "ch-11", "session.rs": "ch-11", "win32_single_instance.rs": "ch-11",
    "sink.rs": "ch-03",
    "build.rs": "ch-02",
}

inv = []
chapter_map: dict[str, list[str]] = {}
for i, u in enumerate(sm["units"], start=1):
    inv_id = f"INV-{i:03d}"
    base = u["path"].rsplit("/", 1)[-1]
    chapter = FILE_CHAPTER.get(base, "ch-12")
    typ = u["kind"].replace("rust_", "")
    inv.append({
        "id": inv_id,
        "type": typ,
        "name": u["name"],
        "file": u["path"],
        "line": u["line_range"][0],
        "covered_by": [],
        "related_source_ids": [u["id"]],
    })
    chapter_map.setdefault(chapter, []).append(inv_id)

(CC / "inventory.json").write_text(
    json.dumps({"units": inv}, ensure_ascii=False, indent=2), encoding="utf-8")
(CC / "chapter_inv_map.json").write_text(
    json.dumps(chapter_map, ensure_ascii=False, indent=2), encoding="utf-8")

print(f"inventory.json: {len(inv)} units")
types = {}
for it in inv:
    types[it["type"]] = types.get(it["type"], 0) + 1
print("by type:", types)
print("per chapter:", {k: len(v) for k, v in sorted(chapter_map.items())})
