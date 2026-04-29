// v1.14: custom アイコンパック (icons-bundle/material 等の SVG パック)
import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

interface IconManifest {
  name: string;
  version?: string;
  author?: string;
  license?: string;
  defaults?: { folder?: string; folderOpen?: string; file?: string; drive?: string };
  byExt?: Record<string, string>;
  byName?: Record<string, string>;
  byFolderName?: Record<string, string>;
}

interface IconPackInfo {
  id: string;
  path: string;
  manifest: IconManifest;
}

const packs = new Map<string, IconPackInfo>();
// key: `${packId}|${rel}` → dataURL or pending
type CacheVal = { url: string } | { pending: true };
const fileCache = new Map<string, CacheVal>();
const [version, setVersion] = createSignal(0);
function bump() { setVersion((v) => v + 1); }

const [loaded, setLoaded] = createSignal(false);

export async function loadCustomPacks(): Promise<void> {
  try {
    const list = await invoke<IconPackInfo[]>("list_icon_packs");
    packs.clear();
    for (const p of list) {
      packs.set(p.id, p);
    }
    setLoaded(true);
    bump();
  } catch (e) {
    console.error("loadCustomPacks failed", e);
  }
}

export function customPacksLoaded(): boolean { return loaded(); }
export function listLoadedPacks(): IconPackInfo[] { return Array.from(packs.values()); }

function resolveRelPath(
  pack: IconPackInfo,
  e: { kind: string; ext?: string | null; name?: string },
): string | null {
  const m = pack.manifest;
  const name = e.name ?? "";
  if (e.kind === "dir") {
    if (m.byFolderName && name in m.byFolderName) return m.byFolderName[name];
    return m.defaults?.folder ?? null;
  }
  if (e.kind === "drive") {
    return m.defaults?.drive ?? m.defaults?.folder ?? null;
  }
  if (m.byName && name && name in m.byName) return m.byName[name];
  const ext = (e.ext ?? "").toLowerCase();
  if (ext && m.byExt && ext in m.byExt) return m.byExt[ext];
  return m.defaults?.file ?? null;
}

/**
 * custom パック アイコンを dataURL として返す。未解決時は空文字。
 * Solid の version() に依存するため、解決時に再描画される。
 */
export function requestCustomIcon(
  packId: string,
  e: { kind: string; ext?: string | null; name?: string },
): string {
  void version();
  if (!loaded()) return "";
  const pack = packs.get(packId);
  if (!pack) return "";
  const rel = resolveRelPath(pack, e);
  if (!rel) return "";
  const k = packId + "|" + rel;
  const v = fileCache.get(k);
  if (v && "url" in v) return v.url;
  if (v && "pending" in v) return "";
  fileCache.set(k, { pending: true });
  invoke<string>("read_icon_file", { pack: packId, rel })
    .then((url) => {
      if (typeof url === "string" && url.length > 0) {
        fileCache.set(k, { url });
      } else {
        fileCache.delete(k);
      }
      bump();
    })
    .catch((err) => {
      console.warn("read_icon_file failed", packId, rel, err);
      fileCache.delete(k);
    });
  return "";
}

export function clearCustomIconCache() {
  fileCache.clear();
  bump();
}
