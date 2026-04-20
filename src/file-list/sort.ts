// FileList のソート比較関数 (純粋ヘルパー、テスト容易)
import type { FileEntry, SortKey, SortDir } from "../types";

export interface SortOpts {
  key: SortKey;
  dir: SortDir;
  foldersFirst: boolean;
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

function extOf(name: string): string {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1).toLowerCase() : "";
}

export function makeFileComparator(opts: SortOpts): (a: FileEntry, b: FileEntry) => number {
  const sign = opts.dir === "asc" ? 1 : -1;
  return (a, b) => {
    if (opts.foldersFirst && a.kind !== b.kind) return a.kind === "dir" ? -1 : 1;
    switch (opts.key) {
      case "size": {
        const av = a.size ?? -1;
        const bv = b.size ?? -1;
        if (av !== bv) return (av - bv) * sign;
        break;
      }
      case "mtime": {
        const av = a.modified ?? 0;
        const bv = b.modified ?? 0;
        if (av !== bv) return (av - bv) * sign;
        break;
      }
      case "kind": {
        const ax = a.kind === "dir" ? "" : extOf(a.name);
        const bx = b.kind === "dir" ? "" : extOf(b.name);
        if (ax !== bx) return collator.compare(ax, bx) * sign;
        break;
      }
      case "name":
      default:
        break;
    }
    return collator.compare(a.name, b.name) * sign;
  };
}

export function sortFileEntries(list: readonly FileEntry[], opts: SortOpts): FileEntry[] {
  return [...list].sort(makeFileComparator(opts));
}
