/**
 * Windows パス操作ユーティリティ。
 *
 * UNC (`\\server\share\sub`) とドライブ (`C:\sub`) を統一的に扱う。
 * すべての関数は内部で `/` を `\` に正規化してから処理する。
 */

export const DRIVES_PATH = "::drives";

export function isDrivesPath(p: string): boolean {
  return p === DRIVES_PATH;
}

export function isUncPath(p: string): boolean {
  return p.startsWith("\\\\");
}

/** 比較用に正規化 (区切り統一 / 末尾 \\ 除去 / lowercase) */
export function normalizePath(p: string): string {
  return p.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();
}

/** base + name を `\` で連結。base が `\` で終わっていれば二重化しない */
export function joinPath(base: string, name: string): string {
  if (base.endsWith("\\") || base.endsWith("/")) return base + name;
  return base + "\\" + name;
}

/**
 * パスを root と segments に分解する。
 * - ドライブ:  `C:\foo\bar` → { root: "C:\", segments: ["foo","bar"] }
 * - UNC:      `\\srv\sh\a\b` → { root: "\\srv\sh", segments: ["a","b"] }
 * - UNC root: `\\srv\sh`     → { root: "\\srv\sh", segments: [] }
 * - その他:   そのまま root として返す
 */
export function splitPath(path: string): { root: string; segments: string[] } | null {
  if (!path || path.startsWith("::")) return null;
  const norm = path.replace(/\//g, "\\");
  if (isUncPath(norm)) {
    const parts = norm.slice(2).split("\\").filter(Boolean);
    if (parts.length < 2) return null; // \\server だけは扱わない
    const root = `\\\\${parts[0]}\\${parts[1]}`;
    return { root, segments: parts.slice(2) };
  }
  const m = norm.match(/^([A-Za-z]:)\\?(.*)$/);
  if (m) {
    const root = m[1] + "\\";
    const segments = m[2] ? m[2].split("\\").filter(Boolean) : [];
    return { root, segments };
  }
  return { root: norm, segments: [] };
}

/**
 * 親フォルダのパス。
 * - ドライブ root → DRIVES_PATH
 * - UNC \\srv\sh → DRIVES_PATH
 * - UNC \\srv\sh\sub → \\srv\sh
 */
export function parentPath(path: string): string {
  if (path === DRIVES_PATH) return DRIVES_PATH;
  const sp = splitPath(path);
  if (!sp) return path;
  if (sp.segments.length === 0) return DRIVES_PATH;
  if (sp.segments.length === 1) return sp.root;
  return joinPath(sp.root, sp.segments.slice(0, -1).join("\\"));
}

/**
 * 祖先パスのチェーン。root を含み、自分自身も含む。
 * 例: `\\srv\sh\a\b` → ["\\srv\sh", "\\srv\sh\a", "\\srv\sh\a\b"]
 *     `C:\foo\bar`  → ["C:\", "C:\foo", "C:\foo\bar"]
 */
export function ancestorChain(path: string): string[] {
  const sp = splitPath(path);
  if (!sp) return [];
  const out = [sp.root];
  let acc = sp.root;
  for (const seg of sp.segments) {
    acc = joinPath(acc, seg);
    out.push(acc);
  }
  return out;
}

/** UNC のサーバ部分 (`\\server`) を返す。UNC でなければ null */
export function uncServerOf(path: string): string | null {
  if (!isUncPath(path)) return null;
  const m = path.match(/^\\\\([^\\]+)/);
  return m ? `\\\\${m[1]}` : null;
}

/** breadcrumbs 用の {label, path} 配列。root は full path をラベルにする */
export function breadcrumbsOf(path: string): { label: string; path: string }[] {
  const sp = splitPath(path);
  if (!sp) return [{ label: path, path }];
  const out: { label: string; path: string }[] = [{ label: sp.root.replace(/\\$/, ""), path: sp.root }];
  let acc = sp.root;
  for (const seg of sp.segments) {
    acc = joinPath(acc, seg);
    out.push({ label: seg, path: acc });
  }
  return out;
}
