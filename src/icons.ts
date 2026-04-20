// v3.3: アイコンセット
import type { IconSet } from "./types";
import type { FileEntry } from "./types";

const COLORED_EXT_MAP: Record<string, string> = {
  // 画像
  png: "🖼", jpg: "🖼", jpeg: "🖼", gif: "🖼", webp: "🖼", bmp: "🖼", svg: "🎨", ico: "🖼", tif: "🖼", tiff: "🖼",
  // 動画
  mp4: "🎬", mov: "🎬", avi: "🎬", mkv: "🎬", webm: "🎬", flv: "🎬", wmv: "🎬",
  // 音声
  mp3: "🎵", wav: "🎵", flac: "🎵", ogg: "🎵", m4a: "🎵", aac: "🎵",
  // ドキュメント
  pdf: "📕", doc: "📘", docx: "📘", xls: "📗", xlsx: "📗", ppt: "📙", pptx: "📙", txt: "📝", md: "📝", rtf: "📝",
  // アーカイブ
  zip: "🗜", "7z": "🗜", rar: "🗜", tar: "🗜", gz: "🗜", bz2: "🗜", xz: "🗜",
  // コード
  js: "📜", ts: "📜", tsx: "📜", jsx: "📜", py: "🐍", rs: "🦀", go: "🐹", java: "☕", c: "📜", cpp: "📜",
  h: "📜", hpp: "📜", cs: "📜", rb: "💎", php: "📜", sh: "📜", ps1: "📜", bat: "📜",
  html: "🌐", htm: "🌐", css: "🎨", scss: "🎨", less: "🎨", json: "📜", yaml: "📜", yml: "📜", xml: "📜", toml: "📜",
  // 実行
  exe: "⚙", msi: "⚙", dll: "⚙", app: "⚙", deb: "⚙", rpm: "⚙",
};

export function iconForEntry(e: { kind: string; ext?: string | null }): string {
  const set = (typeof window !== "undefined" && (window as any).__ff?.state?.iconSet) as IconSet | undefined;
  return iconForEntryWith(e, set ?? "emoji");
}

export function iconForEntryWith(e: { kind: string; ext?: string | null }, set: IconSet): string {
  if (e.kind === "dir") {
    if (set === "minimal") return "▸";
    if (set === "colored") return "📁";
    return "📁";
  }
  if (set === "minimal") return "·";
  if (set === "colored") {
    const ext = (e.ext ?? "").toLowerCase();
    return COLORED_EXT_MAP[ext] ?? "📄";
  }
  return "📄";
}

// FileEntry 互換ヘルパ (Thumbnail フォールバック用)
export function fallbackIcon(entry: Pick<FileEntry, "kind" | "ext">, set: IconSet): string {
  return iconForEntryWith(entry as { kind: string; ext?: string | null }, set);
}
