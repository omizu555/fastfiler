// FS 抽象層
// Tauri 上では Rust コマンドを呼び、ブラウザ単独 (vite dev でブラウザを開いた場合)
// では空配列にフォールバックする。

import type {
  DriveInfo,
  FileEntry,
  PluginInfo,
  PreviewData,
  SearchHit,
  ThumbnailResult,
} from "./types";
import { recordPerf } from "./perf";

// Tauri 2 の判定: window.__TAURI_INTERNALS__ の存在
function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

type InvokeFn = <T = unknown>(
  cmd: string,
  args?: Record<string, unknown>,
) => Promise<T>;
type ListenFn = <T = unknown>(
  event: string,
  cb: (e: { payload: T }) => void,
) => Promise<() => void>;

let _invoke: InvokeFn | null = null;
let _listen: ListenFn | null = null;

async function invoke<T = unknown>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!_invoke) {
    const m = await import("@tauri-apps/api/core");
    _invoke = m.invoke as unknown as InvokeFn;
  }
  return _invoke!<T>(cmd, args);
}

export async function listenFsChange(
  cb: (payload: { path: string; kind: string }) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  if (!_listen) {
    const m = await import("@tauri-apps/api/event");
    _listen = m.listen as unknown as ListenFn;
  }
  return _listen!<{ path: string; kind: string }>("fs-change", (e) =>
    cb(e.payload),
  );
}

export async function listDir(path: string): Promise<FileEntry[]> {
  if (!isTauri()) return [];
  const t0 = performance.now();
  try {
    const entries = await invoke<FileEntry[]>("list_dir", { path });
    const sorted = entries.sort((a, b) => {
      if (a.kind !== b.kind) {
        if (a.kind === "dir") return -1;
        if (b.kind === "dir") return 1;
      }
      return a.name.localeCompare(b.name, undefined, { numeric: true });
    });
    recordPerf({
      kind: "list_dir",
      label: path,
      ms: performance.now() - t0,
      count: sorted.length,
    });
    return sorted;
  } catch (e) {
    console.warn("list_dir failed", path, e);
    recordPerf({
      kind: "list_dir",
      label: path + " (error)",
      ms: performance.now() - t0,
    });
    return [];
  }
}

export async function listDirs(
  path: string,
  includeHidden = true,
): Promise<FileEntry[]> {
  if (!isTauri()) return [];
  try {
    return await invoke<FileEntry[]>("list_dirs", { path, includeHidden });
  } catch (e) {
    console.warn("list_dirs failed", path, e);
    return [];
  }
}

export async function listDrives(): Promise<DriveInfo[]> {
  if (!isTauri())
    return [{ letter: "C:\\", label: "C:\\", kind: "fixed", remotePath: null }];
  try {
    return await invoke<DriveInfo[]>("list_drives");
  } catch {
    return [];
  }
}

export async function homeDir(): Promise<string> {
  if (!isTauri()) return "C:\\";
  try {
    return await invoke<string>("home_dir");
  } catch {
    return "C:\\";
  }
}

export async function watchDir(path: string): Promise<void> {
  if (!isTauri()) return;
  try {
    await invoke("watch_dir", { path });
  } catch (e) {
    console.warn(e);
  }
}

export interface DiskInfo {
  total: number;
  free: number;
  available: number;
}

export async function diskFree(path: string): Promise<DiskInfo | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<DiskInfo>("disk_free", { path });
  } catch {
    return null;
  }
}

export async function unwatchDir(path: string): Promise<void> {
  if (!isTauri()) return;
  try {
    await invoke("unwatch_dir", { path });
  } catch (e) {
    console.warn(e);
  }
}

export async function createDir(path: string): Promise<void> {
  await invoke("create_dir", { path });
}

export async function renamePath(from: string, to: string): Promise<void> {
  await invoke("rename_path", { from, to });
}

export async function deletePath(
  path: string,
  recursive = true,
): Promise<void> {
  await invoke("delete_path", { path, recursive });
}

export async function deleteToTrash(paths: string[]): Promise<void> {
  if (!isTauri()) return;
  await invoke("delete_to_trash", { paths });
}

export async function copyPath(from: string, to: string): Promise<void> {
  await invoke("copy_path", { from, to });
}

export async function movePath(from: string, to: string): Promise<void> {
  await invoke("move_path", { from, to });
}

export async function openWithShell(path: string): Promise<void> {
  if (!isTauri()) {
    console.warn("openWithShell ignored (not Tauri)");
    return;
  }
  await invoke("open_with_shell", { path });
}

export async function revealInExplorer(path: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("reveal_in_explorer", { path });
}

export async function showProperties(path: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("show_properties", { path });
}

// ----- Phase 4: thumbnails / preview -----
export async function getThumbnail(
  path: string,
  size = 96,
): Promise<ThumbnailResult | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<ThumbnailResult>("get_thumbnail", { path, size });
  } catch (e) {
    console.warn("thumbnail failed", path, e);
    return null;
  }
}

export async function readTextPreview(
  path: string,
  maxBytes?: number,
): Promise<PreviewData | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<PreviewData>("read_text_preview", {
      path,
      maxBytes: maxBytes ?? null,
    });
  } catch (e) {
    console.warn("preview failed", path, e);
    return null;
  }
}

// ----- Phase 5: search -----
export async function searchFiles(
  root: string,
  pattern: string,
  opts: {
    caseSensitive?: boolean;
    useRegex?: boolean;
    includeHidden?: boolean;
    maxResults?: number;
    backend?: "builtin" | "everything";
    everythingPort?: number;
    everythingScope?: boolean;
  } = {},
): Promise<number> {
  if (!isTauri()) return 0;
  return await invoke<number>("search_files", {
    root,
    pattern,
    caseSensitive: opts.caseSensitive ?? false,
    useRegex: opts.useRegex ?? false,
    includeHidden: opts.includeHidden ?? true,
    maxResults: opts.maxResults ?? 5000,
    backend: opts.backend ?? "builtin",
    everythingPort: opts.everythingPort ?? 80,
    everythingScope: opts.everythingScope ?? true,
  });
}

export async function everythingPing(port = 80): Promise<boolean> {
  if (!isTauri()) return false;
  try {
    return await invoke<boolean>("everything_ping", { port });
  } catch {
    return false;
  }
}

export async function searchCancel(): Promise<void> {
  if (!isTauri()) return;
  try {
    await invoke("search_cancel");
  } catch {
    /* ignore */
  }
}

export async function listenSearchHit(
  cb: (h: SearchHit) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  if (!_listen) {
    const m = await import("@tauri-apps/api/event");
    _listen = m.listen as unknown as ListenFn;
  }
  return _listen!<SearchHit>("search-hit", (e) => cb(e.payload));
}
export async function listenSearchDone(
  cb: (info: {
    job_id: number;
    total: number;
    canceled: boolean;
    backend: string;
    fallback: boolean;
    error?: string | null;
  }) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  if (!_listen) {
    const m = await import("@tauri-apps/api/event");
    _listen = m.listen as unknown as ListenFn;
  }
  return _listen!<{
    job_id: number;
    total: number;
    canceled: boolean;
    backend: string;
    fallback: boolean;
    error?: string | null;
  }>("search-done", (e) => cb(e.payload));
}

// ----- Phase 6: plugins -----
export async function listPlugins(): Promise<PluginInfo[]> {
  if (!isTauri()) return [];
  try {
    return await invoke<PluginInfo[]>("list_plugins");
  } catch {
    return [];
  }
}
export async function listPluginsWithStatus(): Promise<
  import("./types").PluginStatus[]
> {
  if (!isTauri()) return [];
  try {
    return await invoke("list_plugins_with_status");
  } catch {
    return [];
  }
}
export async function importPluginZip(zipPath: string): Promise<string> {
  if (!isTauri()) throw new Error("Tauri 環境でのみ利用可能");
  return await invoke<string>("import_plugin_zip", { zipPath });
}
export async function deletePlugin(id: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("delete_plugin", { id });
}
export async function pluginsDirPath(): Promise<string> {
  if (!isTauri()) return "";
  try {
    return await invoke<string>("plugins_dir_path");
  } catch {
    return "";
  }
}
export async function pluginInvoke(
  pluginId: string,
  capability: string,
  args: Record<string, unknown> = {},
): Promise<unknown> {
  if (!isTauri()) return null;
  return await invoke("plugin_invoke", { pluginId, capability, args });
}

// v4.0 (40a) Windows ネイティブ右クリックメニュー
export async function shellMenuShow(
  paths: string[],
  x: number,
  y: number,
): Promise<boolean> {
  if (!isTauri()) return false;
  return await invoke<boolean>("shell_menu_show", { paths, x, y });
}

// v4.0 (40b drag-out) ネイティブ OLE ドラッグ送信
export async function oleStartDrag(
  paths: string[],
  allowedEffects = 0x7,
): Promise<number> {
  if (!isTauri()) return 0;
  return await invoke<number>("ole_dnd_start_drag", { paths, allowedEffects });
}

export { DRIVES_PATH, isDrivesPath, joinPath, parentPath } from "./path-util";
export async function writeClipboardPaths(
  paths: string[],
  op: "copy" | "cut",
): Promise<void> {
  if (!isTauri()) return;
  try {
    await invoke("clipboard_write_paths", { paths, op });
  } catch (e) {
    console.warn("clipboard_write_paths failed", e);
  }
}
export function formatSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

export function formatDate(unixSec: number): string {
  if (!unixSec) return "";
  const d = new Date(unixSec * 1000);
  const yyyy = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  const hh = String(d.getHours()).padStart(2, "0");
  const mi = String(d.getMinutes()).padStart(2, "0");
  return `${yyyy}-${mm}-${dd} ${hh}:${mi}`;
}
