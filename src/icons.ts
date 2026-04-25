// v3.3: アイコンセット (+ v1.11 アイコンパック)
import type { IconSet, IconPackId } from "./types";
import type { FileEntry } from "./types";
import { iconForEntryPack } from "./icon-packs";

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

export function iconForEntry(e: { kind: string; ext?: string | null; name?: string }): string {
  const st = (typeof window !== "undefined" && (window as any).__ff?.state) as
    | { iconSet?: IconSet; iconPack?: IconPackId }
    | undefined;
  const pack = st?.iconPack ?? "default";
  if (pack !== "default") return iconForEntryPack(e, pack);
  return iconForEntryWith(e, st?.iconSet ?? "emoji");
}

export function iconForEntryWith(e: { kind: string; ext?: string | null; name?: string }, set: IconSet, pack?: IconPackId): string {
  // v1.11: pack 指定があれば優先
  if (pack && pack !== "default") return iconForEntryPack(e, pack);
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
export function fallbackIcon(entry: Pick<FileEntry, "kind" | "ext" | "name">, set: IconSet, pack?: IconPackId): string {
  return iconForEntryWith(entry as { kind: string; ext?: string | null; name?: string }, set, pack);
}
