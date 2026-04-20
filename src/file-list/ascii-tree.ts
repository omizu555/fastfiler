// ASCII ツリー出力 (フォルダ階層を └─/├─ で表現)
import { listDir } from "../fs";
import { joinPath } from "../path-util";
import type { FileEntry } from "../types";

export interface AsciiTreeOpts {
  maxDepth: number;
  includeFiles: boolean;
  includeHidden: boolean;
}

export async function buildAsciiTree(rootPath: string, opts: AsciiTreeOpts): Promise<string> {
  const rootName = rootPath.split(/[\\/]/).pop() || rootPath;
  const lines: string[] = [rootName];

  const walk = async (path: string, prefix: string, depth: number) => {
    if (depth > opts.maxDepth) return;
    let items: FileEntry[];
    try {
      items = await listDir(path);
    } catch {
      return;
    }
    const filtered = items
      .filter((e) => opts.includeFiles || e.kind === "dir")
      .filter((e) => opts.includeHidden || !e.hidden)
      .sort((a, b) => {
        if (a.kind !== b.kind) return a.kind === "dir" ? -1 : 1;
        return a.name.localeCompare(b.name);
      });
    for (let i = 0; i < filtered.length; i++) {
      const e = filtered[i];
      const last = i === filtered.length - 1;
      const branch = last ? "└─ " : "├─ ";
      lines.push(prefix + branch + e.name + (e.kind === "dir" ? "/" : ""));
      if (e.kind === "dir") {
        await walk(joinPath(path, e.name), prefix + (last ? "   " : "│  "), depth + 1);
      }
    }
  };

  await walk(rootPath, "", 1);
  return lines.join("\n");
}

// "4" / "4f" 形式の入力をパース
export function parseDepthInput(s: string): { maxDepth: number; includeFiles: boolean } | null {
  const m = s.trim().match(/^(\d+)(f?)$/i);
  if (!m) return null;
  const n = parseInt(m[1], 10);
  if (n < 1 || n > 8) return null;
  return { maxDepth: n, includeFiles: !!m[2] };
}
