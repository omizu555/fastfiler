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

// パスからアイコンを推定 (タブ等の先頭表示用)
export function iconForPath(path: string, drives: DriveInfo[] | undefined | null): string {
  if (!path) return "💽";
  // UNC: \\server\share or //server/share
  if (/^[\\/]{2}[^\\/]/.test(path)) return "🌐";
  const m = path.match(/^([A-Za-z]):/);
  if (m) {
    const letter = `${m[1].toUpperCase()}:\\`;
    const d = drives?.find((x) => x.letter.toUpperCase() === letter);
    if (d) return driveIcon(d.kind);
  }
  return "💽";
}
