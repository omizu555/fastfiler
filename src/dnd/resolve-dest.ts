// ペースト/ドロップ時の宛先パス解決と衝突回避
// - 同パス move は除外 (no-op)
// - 衝突時の挙動は policy で制御:
//     "rename"    : "name (2).ext" 形式で自動リネーム (既定)
//     "overwrite" : 既存ファイルを上書き (呼び出し側で事前削除が必要な場合あり)
//     "skip"      : 衝突する項目を除外
// - 同一バッチ内での重複も "rename" 時は解決
import { listDir } from "../fs";
import { joinPath, parentPath } from "../path-util";
import { uniqueNameWithExt } from "../file-list/name-utils";

export type DestOp = "copy" | "move";
export type ConflictPolicy = "rename" | "overwrite" | "skip";

export interface ResolvedItem {
  from: string;
  to: string;
  /** 衝突回避でリネームされた場合の最終名 */
  renamed: boolean;
  /** policy=overwrite で既存項目を上書きする場合 true (呼び出し側が事前削除する判定に使う) */
  overwrite?: boolean;
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
  policy: ConflictPolicy = "rename",
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
    const collides = existing.has(name);
    if (collides && policy === "skip") {
      continue;
    }
    let renamed = false;
    let overwrite = false;
    let finalName = name;
    if (collides) {
      if (policy === "overwrite") {
        overwrite = true;
      } else {
        finalName = uniqueNameWithExt(name, existing);
        renamed = finalName !== name;
      }
    }
    existing.add(finalName);
    result.push({ from: src, to: joinPath(dstDir, finalName), renamed, overwrite });
  }
  return result;
}

/**
 * 宛先ディレクトリ内で衝突する src パスのファイル名一覧を返す。
 * 自身/子孫や同パス move は除外して数える。
 */
export async function findConflicts(
  srcPaths: string[],
  dstDir: string,
  op: DestOp,
): Promise<string[]> {
  let existing: Set<string>;
  try {
    const list = await listDir(dstDir);
    existing = new Set(list.map((e) => e.name));
  } catch {
    return [];
  }
  const conflicts: string[] = [];
  for (const src of srcPaths) {
    if (isSelfOrDescendant(src, dstDir)) continue;
    const name = baseName(src);
    if (op === "move" && norm(src) === norm(joinPath(dstDir, name))) continue;
    if (existing.has(name)) conflicts.push(name);
  }
  return conflicts;
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
