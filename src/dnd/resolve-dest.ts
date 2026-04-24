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

/**
 * フォルダ自身/子孫へのドロップ判定 (無限再帰コピー防止)。
 *   src=C:\A, dst=C:\A     → true
 *   src=C:\A, dst=C:\A\B   → true
 *   src=C:\A, dst=C:\Aa    → false (前方一致だけだと誤検出するため区切り判定)
 */
function isSelfOrDescendant(src: string, dst: string): boolean {
  const a = norm(src);
  const b = norm(dst);
  if (a === b) return true;
  return b.startsWith(a + "\\") || b.startsWith(a + "/");
}

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
    // 自身 or 子孫への drop は禁止 (無限再帰コピー/移動を防ぐ)
    if (isSelfOrDescendant(src, dstDir)) {
      continue;
    }
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
