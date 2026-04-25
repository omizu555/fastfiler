// v1.11: アイコン パック (拡張子別アイコン)
//
// パック ID:
//   default  — 既定 (汎用 📄 / 📁、iconSet=emoji と同等)
//   emoji    — 拡張子別の絵文字 (iconSet=colored 強化版)
//   material — Material Design Icons 風 (シンプルな代表アイコンを絵文字で代用)
//   vscode   — VSCode/Seti UI 風 (色付き絵文字寄せ)
//   mono     — モノクロ記号
//
// 軽量化のため SVG ではなく絵文字 + 一部 Unicode シンボルで構成。
// パック依存は string 1 文字 (or 絵文字 + 修飾子) のみ。

import type { FileEntry, IconPackId } from "./types";

type ExtMap = Record<string, string>;

// ---- emoji (リッチ) ----
const EMOJI_EXT: ExtMap = {
  // 画像
  png: "🖼️", jpg: "🖼️", jpeg: "🖼️", gif: "🖼️", webp: "🖼️", bmp: "🖼️",
  ico: "🖼️", tif: "🖼️", tiff: "🖼️", svg: "🎨", psd: "🎨", ai: "🎨",
  // 動画
  mp4: "🎬", mov: "🎬", avi: "🎬", mkv: "🎬", webm: "🎬", flv: "🎬", wmv: "🎬", m4v: "🎬",
  // 音声
  mp3: "🎵", wav: "🎵", flac: "🎵", ogg: "🎵", m4a: "🎵", aac: "🎵", wma: "🎵", opus: "🎵",
  // ドキュメント
  pdf: "📕", doc: "📘", docx: "📘", xls: "📗", xlsx: "📗", csv: "📊",
  ppt: "📙", pptx: "📙", txt: "📝", md: "📝", rtf: "📝", odt: "📝", ods: "📊",
  // アーカイブ
  zip: "🗜️", "7z": "🗜️", rar: "🗜️", tar: "🗜️", gz: "🗜️", bz2: "🗜️", xz: "🗜️",
  iso: "💿", img: "💿", dmg: "💿", vhd: "💿", vhdx: "💿",
  // コード
  js: "📜", ts: "📜", tsx: "📜", jsx: "📜", py: "🐍", rs: "🦀", go: "🐹",
  java: "☕", kt: "📜", swift: "📜", c: "📜", cpp: "📜", h: "📜", hpp: "📜",
  cs: "📜", rb: "💎", php: "📜", sh: "📜", ps1: "📜", bat: "📜", lua: "📜",
  html: "🌐", htm: "🌐", css: "🎨", scss: "🎨", less: "🎨", sass: "🎨",
  json: "📋", yaml: "📋", yml: "📋", xml: "📋", toml: "📋", ini: "⚙️",
  // 実行 / バイナリ
  exe: "⚙️", msi: "⚙️", dll: "⚙️", app: "⚙️", deb: "📦", rpm: "📦",
  // フォント
  ttf: "🔤", otf: "🔤", woff: "🔤", woff2: "🔤",
  // データベース
  db: "🗄️", sqlite: "🗄️", sql: "🗄️",
  // その他
  log: "📃", lock: "🔒", env: "🔐",
};

// ---- material 風 (簡易: 種別カテゴリで絵分け) ----
const MATERIAL_EXT: ExtMap = {
  // 画像 (青系)
  png: "🟦", jpg: "🟦", jpeg: "🟦", gif: "🟦", webp: "🟦", bmp: "🟦", svg: "🟦", ico: "🟦",
  // 動画 (赤)
  mp4: "🟥", mov: "🟥", avi: "🟥", mkv: "🟥", webm: "🟥",
  // 音声 (紫)
  mp3: "🟪", wav: "🟪", flac: "🟪", ogg: "🟪",
  // ドキュメント
  pdf: "📕", doc: "📘", docx: "📘", xls: "📗", xlsx: "📗", csv: "📊",
  ppt: "📙", pptx: "📙", txt: "📝", md: "📝",
  // アーカイブ (茶)
  zip: "🟫", "7z": "🟫", rar: "🟫", tar: "🟫", gz: "🟫",
  iso: "💿", img: "💿", vhd: "💿", vhdx: "💿",
  // コード (黄)
  js: "🟨", ts: "🟦", tsx: "🟦", jsx: "🟨", py: "🟦", rs: "🟧", go: "🟦",
  java: "🟧", c: "🟦", cpp: "🟦", cs: "🟪", rb: "🟥", php: "🟪",
  html: "🟧", css: "🟦", json: "🟨", yaml: "🟪", yml: "🟪", xml: "🟧",
  sh: "⬛", ps1: "🟦", bat: "⬛",
  // 実行
  exe: "⚙️", msi: "⚙️", dll: "⚙️",
};

// ---- vscode/Seti 風 (拡張子で色をつけるイメージ) ----
const VSCODE_EXT: ExtMap = {
  // 言語
  ts: "🇹", tsx: "🇹", js: "🇯", jsx: "🇯",
  py: "🐍", rs: "🦀", go: "🐹", java: "☕", c: "🇨", cpp: "🇨", cs: "©️",
  rb: "💎", php: "🐘", swift: "🦅", kt: "🇰", lua: "🌙",
  // Web
  html: "🌐", htm: "🌐", css: "🎨", scss: "🎨", sass: "🎨", less: "🎨",
  vue: "🟢", svelte: "🟠",
  // データ
  json: "🔣", yaml: "📐", yml: "📐", xml: "📐", toml: "📐", ini: "📐",
  md: "Ⓜ️", rst: "Ⓜ️",
  // 画像/動画/音声
  png: "🖼️", jpg: "🖼️", jpeg: "🖼️", gif: "🖼️", webp: "🖼️", svg: "🖌️",
  mp4: "🎞️", mov: "🎞️", mkv: "🎞️", webm: "🎞️",
  mp3: "🎶", wav: "🎶", flac: "🎶",
  // ドキュメント
  pdf: "📕", doc: "📘", docx: "📘", xls: "📗", xlsx: "📗", ppt: "📙", pptx: "📙", txt: "📃",
  // アーカイブ / イメージ
  zip: "📦", "7z": "📦", rar: "📦", tar: "📦", gz: "📦",
  iso: "💿", img: "💿", vhd: "💿", vhdx: "💿",
  // 実行
  exe: "🚀", msi: "🚀", dll: "🧩", bat: "🐚", sh: "🐚", ps1: "🐚",
  // 設定
  env: "🔐", lock: "🔒", gitignore: "🚫",
};

// ---- mono (モノクロ Unicode) ----
const MONO_EXT: ExtMap = {
  png: "▤", jpg: "▤", jpeg: "▤", gif: "▤", webp: "▤", bmp: "▤", svg: "▤", ico: "▤",
  mp4: "▶", mov: "▶", avi: "▶", mkv: "▶", webm: "▶",
  mp3: "♪", wav: "♪", flac: "♪", ogg: "♪",
  pdf: "▼", doc: "▼", docx: "▼", xls: "▼", xlsx: "▼", ppt: "▼", pptx: "▼",
  txt: "≡", md: "≡", csv: "≡",
  zip: "◫", "7z": "◫", rar: "◫", tar: "◫", gz: "◫",
  iso: "◉", img: "◉", vhd: "◉", vhdx: "◉",
  js: "›", ts: "›", py: "›", rs: "›", go: "›", c: "›", cpp: "›", cs: "›",
  html: "‹", css: "‹", json: "‹", xml: "‹", yaml: "‹", yml: "‹",
  exe: "★", msi: "★", dll: "★", bat: "★", ps1: "★", sh: "★",
};

const FOLDER_BY_PACK: Record<IconPackId, string> = {
  default: "📁",
  emoji: "📁",
  material: "📁",
  vscode: "🗂️",
  mono: "▸",
};

const FILE_BY_PACK: Record<IconPackId, string> = {
  default: "📄",
  emoji: "📄",
  material: "📄",
  vscode: "📄",
  mono: "·",
};

const PACK_MAP: Record<IconPackId, ExtMap> = {
  default: {},
  emoji: EMOJI_EXT,
  material: MATERIAL_EXT,
  vscode: VSCODE_EXT,
  mono: MONO_EXT,
};

export function iconForEntryPack(
  e: { kind: string; ext?: string | null; name?: string },
  pack: IconPackId,
): string {
  if (e.kind === "dir") return FOLDER_BY_PACK[pack] ?? "📁";
  const ext = (e.ext ?? "").toLowerCase();
  // 拡張子なし special-case (.gitignore 等は ext が空でも name で判定)
  if (!ext && e.name?.startsWith(".")) {
    const tail = e.name.slice(1).toLowerCase();
    const found = PACK_MAP[pack][tail];
    if (found) return found;
  }
  return PACK_MAP[pack][ext] ?? FILE_BY_PACK[pack] ?? "📄";
}

export function fallbackIconPack(entry: Pick<FileEntry, "kind" | "ext" | "name">, pack: IconPackId): string {
  return iconForEntryPack(entry as { kind: string; ext?: string | null; name?: string }, pack);
}
