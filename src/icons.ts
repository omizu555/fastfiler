// v3.3: アイコンセット (+ v1.11 アイコンパック, v1.14 system / custom 対応)
import type { IconSet, IconPackId } from "./types";
import { iconForEntryPack } from "./icon-packs";
import { requestSystemIcon } from "./icon-system";
import { requestCustomIcon } from "./icon-custom";

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

export function iconForEntryWith(e: { kind: string; ext?: string | null; name?: string }, set: IconSet, pack?: IconPackId): string {
  // v1.11: pack 指定があれば優先
  if (pack && pack !== "default" && pack !== "system" && !pack.startsWith("custom:")) {
    return iconForEntryPack(e, pack);
  }
  // v1.14: system / custom は <img> 表示で別経路。ここでは fallback 絵文字を返す。
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

/**
 * v1.14: system / custom パックのアイコン dataURL を取得。
 * 未解決時 / 該当しないパックは "" を返す (呼び出し側で <span> 絵文字 fallback)。
 * absPath は system パックでファイル種別判定に使用。dir/drive/file の判定は entry.kind を優先。
 */
export function iconImageForEntry(
  e: { kind: string; ext?: string | null; name?: string },
  pack: IconPackId | undefined,
  absPath?: string,
): string {
  if (!pack) return "";
  if (pack === "system") {
    if (!absPath) return "";
    return requestSystemIcon(absPath, false);
  }
  if (pack.startsWith("custom:")) {
    const id = pack.slice("custom:".length);
    return requestCustomIcon(id, e);
  }
  return "";
}
