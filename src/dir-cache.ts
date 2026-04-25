// v1.10: ディレクトリ一覧の localStorage キャッシュ。
// 起動時/フォルダ移動時に「前回の listDir 結果」を即座に描画して体感速度を上げる。
// 実物の listDir 結果が来たら上書きするため、差分検知や TTL は持たない (シンプル優先)。

import type { FileEntry } from "./types";

const STORAGE_KEY = "fastfiler:dircache:v1";
const MAX_ENTRIES = 32; // LRU 上限 (1 ディレクトリあたり数 KB を想定)

interface Cache {
  // path → entries (LRU 順、末尾が最新)
  order: string[];
  data: Record<string, FileEntry[]>;
}

let mem: Cache | null = null;

function load(): Cache {
  if (mem) return mem;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const v = JSON.parse(raw) as Cache;
      if (v && Array.isArray(v.order) && v.data) {
        mem = v;
        return mem;
      }
    }
  } catch {
    // ignore
  }
  mem = { order: [], data: {} };
  return mem;
}

let saveTimer: number | null = null;
function scheduleSave(): void {
  if (saveTimer !== null) return;
  saveTimer = window.setTimeout(() => {
    saveTimer = null;
    if (!mem) return;
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(mem));
    } catch {
      // クォータ超過などは握り潰す (キャッシュなので失敗しても致命ではない)
    }
  }, 500);
}

const norm = (p: string) => p.replace(/[\\/]+$/, "").toLowerCase();

export function getCachedListing(path: string): FileEntry[] | null {
  const c = load();
  const k = norm(path);
  const v = c.data[k];
  return v ?? null;
}

export function setCachedListing(path: string, entries: FileEntry[]): void {
  const c = load();
  const k = norm(path);
  c.data[k] = entries;
  // LRU 更新
  const idx = c.order.indexOf(k);
  if (idx >= 0) c.order.splice(idx, 1);
  c.order.push(k);
  while (c.order.length > MAX_ENTRIES) {
    const dropped = c.order.shift();
    if (dropped) delete c.data[dropped];
  }
  scheduleSave();
}

export function clearCachedListing(path: string): void {
  const c = load();
  const k = norm(path);
  if (c.data[k]) {
    delete c.data[k];
    const idx = c.order.indexOf(k);
    if (idx >= 0) c.order.splice(idx, 1);
    scheduleSave();
  }
}

export function flushDirCacheImmediate(): void {
  if (saveTimer !== null) {
    window.clearTimeout(saveTimer);
    saveTimer = null;
  }
  if (!mem) return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(mem));
  } catch {
    // ignore
  }
}
