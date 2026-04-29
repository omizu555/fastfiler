import type { DockSlot } from "../types";
import { state, setState, persist, ensureDock } from "./core";
import { setPanelSlot, togglePanelVisible } from "./dock";

export function cycleWorkspaceLayout() {
  // tabs パネルの slot を left → right → bottom → top → hidden で巡回
  ensureDock();
  const order: DockSlot[] = ["left", "right", "bottom", "top", "hidden"];
  const cur = state.workspace.panelDock!.tabs.slot;
  const idx = order.indexOf(cur as DockSlot);
  const next = order[(idx + 1) % order.length];
  setPanelSlot("tabs", next);
}

export function toggleWorkspaceTree() {
  togglePanelVisible("tree");
}

export function setWorkspaceTabsWidth(w: number) {
  const v = Math.max(120, Math.min(1200, Math.round(w)));
  setState("workspace", "tabsWidth", v);
  ensureDock();
  setState("workspace", "panelDock", "tabs", "size", v);
  persist();
}

export function setWorkspaceTreeWidth(w: number) {
  const v = Math.max(120, Math.min(1200, Math.round(w)));
  setState("workspace", "treeWidth", v);
  ensureDock();
  setState("workspace", "panelDock", "tree", "size", v);
  persist();
}

export function setSamePanelStack(v: boolean) {
  setState("workspace", "samePanelStack", v);
  persist();
}
