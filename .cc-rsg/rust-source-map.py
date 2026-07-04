#!/usr/bin/env python3
"""
cc-rsg rust-source-map.py  (project-local, Rust-aware)

The shipped scripts/source-map.py has no Rust support and emits Windows
backslash paths (which break build-trace.py's REF suffix-matching). This
project-local generator replaces it for the FastFiler (Rust) codebase.

Strategy: tile each .rs file by its COLUMN-0 (top-level) items. Methods
inside `impl` blocks are folded into their impl unit (no brace matching →
robust). Each unit's line_range = [start, next_col0_item_start - 1], last
unit ends at EOF. Paths are emitted POSIX-style relative to the repo root
so `[REF: crates/.../foo.rs:NN]` resolves cleanly.

Output schema matches scripts/source-map.py (source-map.json) so
build-trace.py / coverage-check.py consume it unchanged.

Usage:
    python .cc-rsg/rust-source-map.py \
        --root . \
        --include crates/fastfiler-domain/src crates/fastfiler-gpui/src \
        --extra crates/fastfiler-gpui/build.rs \
        --output .cc-rsg/source-map.json
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

# Column-0 item start. Optional visibility / qualifiers, then the keyword.
ITEM_RE = re.compile(
    r"^(?:pub(?:\([^)]*\))?\s+)?"
    r"(?:default\s+)?"
    r"(?:unsafe\s+)?"
    r"(?:async\s+)?"
    r"(?:extern\s+(?:\"[^\"]*\"\s+)?)?"
    r"(struct|enum|trait|impl|fn|const|static|type|mod|union|macro_rules!)\b(.*)$"
)


@dataclass
class Unit:
    line: int  # 1-indexed start
    kind: str
    name: str
    signature: str


def fingerprint(text: str) -> str:
    return "sha1:" + hashlib.sha1(text.encode("utf-8", "replace")).hexdigest()[:16]


def parse_name(kind: str, rest: str) -> str:
    rest = rest.strip()
    if kind == "macro_rules!":
        m = re.match(r"\s*([A-Za-z_][A-Za-z0-9_]*)", rest)
        return m.group(1) if m else "<macro>"
    if kind == "impl":
        # impl<...> Trait for Type {  /  impl Type {  /  impl<...> Type {
        body = re.sub(r"^\s*<[^>]*>", "", rest)  # drop generic params right after impl
        body = body.split("{", 1)[0]
        body = body.split(" where", 1)[0].strip()
        mfor = re.search(r"\bfor\s+([A-Za-z0-9_:<>, '&]+)$", body)
        if mfor:
            trait_part = body[: mfor.start()].strip()
            type_part = mfor.group(1).strip()
            return f"impl {trait_part} for {type_part}".strip()
        return f"impl {body}".strip()
    if kind in ("const", "static"):
        m = re.match(r"\s*(?:mut\s+)?([A-Za-z_][A-Za-z0-9_]*)", rest)
        return m.group(1) if m else "<const>"
    if kind == "fn":
        m = re.match(r"\s*([A-Za-z_][A-Za-z0-9_]*)", rest)
        return m.group(1) if m else "<fn>"
    # struct / enum / trait / type / mod / union
    m = re.match(r"\s*([A-Za-z_][A-Za-z0-9_]*)", rest)
    return m.group(1) if m else f"<{kind}>"


def extract_units(lines: list[str]) -> tuple[list[Unit], list[int]]:
    """Return (emitted_units, all_boundary_start_lines).

    `mod` items are kept as tiling boundaries but NOT emitted as units:
    they are either module declarations (`pub mod x;`) or `#[cfg(test)] mod
    tests { ... }` blocks. Inner test fns are indented so they are never
    column-0 units; dropping the `mod` unit cleanly removes test code from
    the production MECE denominator while still bounding the preceding unit.
    """
    units: list[Unit] = []
    boundaries: list[int] = []
    for i, line in enumerate(lines):
        # column-0 only: the line must not start with whitespace
        if line[:1] in (" ", "\t"):
            continue
        m = ITEM_RE.match(line)
        if not m:
            continue
        kind = m.group(1)
        boundaries.append(i + 1)
        if kind == "mod":
            continue  # boundary only, not an emitted unit
        kw = "macro" if kind == "macro_rules!" else kind
        name = parse_name(kind, m.group(2))
        units.append(Unit(line=i + 1, kind=f"rust_{kw}", name=name, signature=line.strip()[:240]))
    return units, boundaries


def collect_files(root: Path, includes: list[str], extras: list[str]) -> list[Path]:
    files: list[Path] = []
    for inc in includes:
        d = (root / inc)
        if d.is_dir():
            files.extend(sorted(d.rglob("*.rs")))
    for ex in extras:
        p = root / ex
        if p.is_file():
            files.append(p)
    # de-dup, skip anything under target/
    seen = set()
    out: list[Path] = []
    for f in files:
        if "target" in f.parts:
            continue
        rp = f.resolve()
        if rp in seen:
            continue
        seen.add(rp)
        out.append(f)
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=".")
    ap.add_argument("--include", nargs="*", default=[])
    ap.add_argument("--extra", nargs="*", default=[])
    ap.add_argument("--output", default=".cc-rsg/source-map.json")
    args = ap.parse_args(argv)

    root = Path(args.root).resolve()
    files = collect_files(root, args.include, args.extra)

    all_units: list[dict] = []
    by_kind: dict[str, int] = {}
    next_id = 0
    files_scanned = 0

    for f in files:
        try:
            text = f.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            text = f.read_text(encoding="utf-8", errors="replace")
        lines = text.splitlines()
        rel = f.resolve().relative_to(root).as_posix()  # POSIX path, repo-relative
        units, boundaries = extract_units(lines)
        if not units:
            # still count the file as scanned; emit a coarse file-level unit
            files_scanned += 1
            next_id += 1
            all_units.append({
                "id": f"SRC-{next_id:04d}",
                "path": rel,
                "line_range": [1, max(len(lines), 1)],
                "kind": "rust_file",
                "name": f.name,
                "signature": (lines[0].strip()[:240] if lines else f.name),
                "fingerprint": fingerprint(text),
            })
            by_kind["rust_file"] = by_kind.get("rust_file", 0) + 1
            continue

        files_scanned += 1
        for u in units:
            start = u.line
            # end = (next boundary strictly after start) - 1, else EOF
            nxt = next((b for b in boundaries if b > start), None)
            end = (nxt - 1) if nxt is not None else len(lines)
            if end < start:
                end = start
            block = "\n".join(lines[start - 1:end])
            next_id += 1
            all_units.append({
                "id": f"SRC-{next_id:04d}",
                "path": rel,
                "line_range": [start, end],
                "kind": u.kind,
                "name": u.name,
                "signature": u.signature,
                "fingerprint": fingerprint(block),
            })
            by_kind[u.kind] = by_kind.get(u.kind, 0) + 1

    out = {
        "schema_version": "0.1.0",
        "target_root": root.name,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "stats": {
            "files_scanned": files_scanned,
            "files_excluded": 0,
            "units_total": len(all_units),
            "by_kind": by_kind,
        },
        "units": all_units,
    }
    outp = Path(args.output)
    outp.parent.mkdir(parents=True, exist_ok=True)
    outp.write_text(json.dumps(out, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"rust-source-map: {len(all_units)} units from {files_scanned} files -> {outp}")
    print(f"  by_kind: {by_kind}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
