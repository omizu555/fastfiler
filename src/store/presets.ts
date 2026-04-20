import type { WorkspacePreset, WorkspacePresetSnapshot } from "../types";
import { defaultPaneUi } from "../types";
import { state, setState, persist, nid } from "./core";

function snapshotWorkspace(): WorkspacePresetSnapshot {
  return JSON.parse(JSON.stringify({
    tabs: state.tabs,
    activeTabId: state.activeTabId,
    panes: state.panes,
    workspace: state.workspace,
  })) as WorkspacePresetSnapshot;
}

export function savePreset(name: string): WorkspacePreset {
  const preset: WorkspacePreset = {
    id: nid("preset"),
    name: name.trim() || "(無題)",
    savedAt: Date.now(),
    snapshot: snapshotWorkspace(),
  };
  setState("presets", (xs) => [...xs, preset]);
  persist();
  return preset;
}

export function deletePreset(id: string) {
  setState("presets", (xs) => xs.filter((p) => p.id !== id));
  persist();
}

export function renamePreset(id: string, name: string) {
  setState("presets", (xs) => xs.map((p) => (p.id === id ? { ...p, name: name.trim() || p.name } : p)));
  persist();
}

export function applyPreset(id: string) {
  const p = state.presets.find((x) => x.id === id);
  if (!p) return;
  setState("tabs", p.snapshot.tabs);
  setState("activeTabId", p.snapshot.activeTabId);
  setState("panes", p.snapshot.panes);
  setState("workspace", p.snapshot.workspace);
  for (const pid of Object.keys(p.snapshot.panes)) {
    if (!state.paneUi[pid]) setState("paneUi", pid, defaultPaneUi());
  }
  const firstPane = Object.keys(p.snapshot.panes)[0] ?? null;
  setState("focusedPaneId", firstPane);
  persist();
}

export function exportPresetsJson(): string {
  return JSON.stringify({ kind: "fastfiler.presets", version: 1, presets: state.presets }, null, 2);
}

export function importPresetsJson(text: string, mode: "merge" | "replace" = "merge"): number {
  const obj = JSON.parse(text) as { kind?: string; version?: number; presets?: WorkspacePreset[] };
  if (obj.kind !== "fastfiler.presets" || !Array.isArray(obj.presets)) {
    throw new Error("不正なプリセット JSON です");
  }
  const remapped = obj.presets.map((p) => ({
    ...p,
    id: nid("preset"),
    savedAt: typeof p.savedAt === "number" ? p.savedAt : Date.now(),
  }));
  if (mode === "replace") setState("presets", remapped);
  else setState("presets", (xs) => [...xs, ...remapped]);
  persist();
  return remapped.length;
}
