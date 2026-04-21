// システムフォント一覧の取得 (Local Font Access API)。失敗時は curated fallback。
let cache: string[] | null = null;
let inflight: Promise<string[]> | null = null;

const FALLBACK = [
  "Yu Gothic UI",
  "Yu Gothic",
  "Meiryo",
  "Meiryo UI",
  "MS Gothic",
  "MS UI Gothic",
  "MS PGothic",
  "Segoe UI",
  "Arial",
  "Tahoma",
  "Verdana",
  "Calibri",
  "Cascadia Mono",
  "Cascadia Code",
  "Consolas",
  "Courier New",
  "Lucida Console",
];

interface QueryLocalFontsApi {
  queryLocalFonts?: () => Promise<{ family: string }[]>;
}

export async function loadSystemFonts(): Promise<string[]> {
  if (cache) return cache;
  if (inflight) return inflight;
  inflight = (async () => {
    try {
      const w = window as unknown as QueryLocalFontsApi;
      if (typeof w.queryLocalFonts === "function") {
        const list = await w.queryLocalFonts();
        const fams = Array.from(new Set(list.map((f) => f.family))).sort((a, b) =>
          a.localeCompare(b, "ja"),
        );
        if (fams.length > 0) {
          cache = fams;
          return fams;
        }
      }
    } catch (e) {
      console.warn("queryLocalFonts failed:", e);
    }
    cache = [...FALLBACK];
    return cache;
  })();
  try {
    return await inflight;
  } finally {
    inflight = null;
  }
}

export function fallbackFonts(): string[] {
  return [...FALLBACK];
}
