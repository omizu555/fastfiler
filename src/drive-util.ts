import type { DriveInfo, DriveKind } from "./types";

export function driveIcon(kind: DriveKind): string {
  switch (kind) {
    case "network":
      return "🌐";
    case "removable":
      return "💾";
    case "cdrom":
      return "💿";
    case "ram":
      return "⚡";
    case "fixed":
    case "unknown":
    default:
      return "💽";
  }
}

export function driveTitle(d: DriveInfo): string {
  const parts: string[] = [d.letter];
  if (d.label) parts.push(d.label);
  if (d.kind === "network" && d.remotePath) parts.push(`(${d.remotePath})`);
  else if (d.kind !== "fixed" && d.kind !== "unknown") parts.push(`[${d.kind}]`);
  return parts.join("  ");
}

export function driveDisplayLabel(d: DriveInfo): string {
  // ツリー / カードの本文表示用 (簡潔に)
  if (d.label) return `${d.letter} (${d.label})`;
  if (d.kind === "network" && d.remotePath) return `${d.letter} (${d.remotePath})`;
  return d.letter;
}

// パスからアイコン (またはドライブ文字) を推定 (タブ等の先頭表示用)
// ドライブレターがある場合は文字そのものを返し、UNC のみネットワークアイコン
export function iconForPath(path: string, _drives?: DriveInfo[] | null): string {
  if (!path) return "";
  if (/^[\\/]{2}[^\\/]/.test(path)) return "🌐";
  const m = path.match(/^([A-Za-z]):/);
  if (m) return `${m[1].toUpperCase()}:`;
  return "";
}
