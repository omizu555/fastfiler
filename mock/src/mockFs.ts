import type { FileEntry } from "./types";

const TREE: Record<string, FileEntry[]> = {
  "C:\\": [
    { name: "Program Files", kind: "dir", size: 0, modified: "2025-12-01" },
    { name: "Users", kind: "dir", size: 0, modified: "2026-01-15" },
    { name: "Windows", kind: "dir", size: 0, modified: "2025-11-20" },
    { name: "temp", kind: "dir", size: 0, modified: "2026-04-18" },
  ],
  "C:\\Users": [
    { name: "Public", kind: "dir", size: 0, modified: "2025-10-01" },
    { name: "o_miz", kind: "dir", size: 0, modified: "2026-04-18" },
  ],
  "C:\\Users\\o_miz": [
    { name: "Desktop", kind: "dir", size: 0, modified: "2026-04-18" },
    { name: "Documents", kind: "dir", size: 0, modified: "2026-04-10" },
    { name: "Downloads", kind: "dir", size: 0, modified: "2026-04-17" },
    { name: "notes.md", kind: "file", size: 4321, modified: "2026-04-18", ext: "md" },
  ],
  "C:\\temp": [
    { name: "Files", kind: "dir", size: 0, modified: "2026-04-18" },
    { name: "scratch.txt", kind: "file", size: 128, modified: "2026-04-15", ext: "txt" },
  ],
  "C:\\temp\\Files": [
    { name: "doc", kind: "dir", size: 0, modified: "2026-04-18" },
    { name: "src", kind: "dir", size: 0, modified: "2026-04-18" },
    { name: "package.json", kind: "file", size: 512, modified: "2026-04-18", ext: "json" },
    { name: "README.md", kind: "file", size: 2048, modified: "2026-04-18", ext: "md" },
  ],
  "C:\\temp\\Files\\doc": [
    { name: "plan.md", kind: "file", size: 7867, modified: "2026-04-18", ext: "md" },
  ],
  "C:\\temp\\Files\\src": [
    { name: "components", kind: "dir", size: 0, modified: "2026-04-18" },
    { name: "App.tsx", kind: "file", size: 1024, modified: "2026-04-18", ext: "tsx" },
    { name: "main.tsx", kind: "file", size: 256, modified: "2026-04-18", ext: "tsx" },
    { name: "styles.css", kind: "file", size: 2048, modified: "2026-04-18", ext: "css" },
  ],
  "C:\\Users\\o_miz\\Downloads": Array.from({ length: 80 }, (_, i) => ({
    name: `large_dataset_${String(i).padStart(3, "0")}.bin`,
    kind: "file" as const,
    size: 1024 * (i + 1),
    modified: "2026-04-15",
    ext: "bin",
  })),
};

export function listDir(path: string): FileEntry[] {
  const entries = TREE[path];
  if (!entries) return [];
  return [...entries].sort((a, b) => {
    if (a.kind !== b.kind) return a.kind === "dir" ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
}

export function joinPath(base: string, name: string): string {
  if (base.endsWith("\\")) return base + name;
  return base + "\\" + name;
}

export function parentPath(path: string): string {
  const idx = path.lastIndexOf("\\");
  if (idx <= 2) return path.substring(0, 3);
  return path.substring(0, idx);
}
