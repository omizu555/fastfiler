// ペースト/ドロップ時の宛先パス解決と衝突回避
// - 同パス move は除外 (no-op)
// - 宛先ディレクトリ内の既存名と衝突 → "name (2).ext" 形式にリネーム
// - 同一バッチ内での重複も解決 (複数を同フォルダに同時 paste した場合)
import { listDir } from "../fs";
import { joinPath, parentPath } from "../path-util";
import { uniqueNameWithExt } from "../file-list/name-utils";

export type DestOp = "copy" | "move";

export interface ResolvedItem {
  from: string;
  to: string;
  /** 衝突回避でリネームされた場合の最終名 */
  renamed: boolean;
}

const norm = (p: string) => p.replace(/[\\/]+$/, "").toLowerCase();
const baseName = (p: string) =>
  p.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "untitled";

export async function resolveDestinations(
  srcPaths: string[],
  dstDir: string,
  op: DestOp,
): Promise<ResolvedItem[]> {
  let existing: Set<string>;
  try {
    const list = await listDir(dstDir);
    existing = new Set(list.map((e) => e.name));
  } catch {
    existing = new Set();
  }
  const result: ResolvedItem[] = [];
  for (const src of srcPaths) {
    const name = baseName(src);
    const finalDst0 = joinPath(dstDir, name);
    if (op === "move" && norm(src) === norm(finalDst0)) {
      continue;
    }
    let renamed = false;
    let finalName = name;
    if (existing.has(name)) {
      finalName = uniqueNameWithExt(name, existing);
      renamed = finalName !== name;
    }
    existing.add(finalName);
    result.push({ from: src, to: joinPath(dstDir, finalName), renamed });
  }
  return result;
}

export function refreshTargets(
  items: ResolvedItem[],
  dstDir: string,
  includeSources: boolean,
): string[] {
  const set = new Set<string>([dstDir]);
  if (includeSources) {
    for (const it of items) set.add(parentPath(it.from));
  }
  return [...set];
}
