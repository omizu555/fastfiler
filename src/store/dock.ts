import type { DockSlot, PanelId } from "../types";
import { state, setState, persist, ensureDock } from "./core";

export function setPanelSlot(panel: PanelId, slot: DockSlot) {
  ensureDock();
  setState("workspace", "panelDock", panel, "slot", slot);
  if (slot !== "float" && slot !== "hidden") {
    setState("workspace", "panelDock", panel, "lastDockSlot", slot);
  }
  persist();
}

export function setPanelOrder(panel: PanelId, order: number) {
  ensureDock();
  setState("workspace", "panelDock", panel, "order", order);
  persist();
}

export function setPanelSize(panel: PanelId, size: number) {
  ensureDock();
  setState("workspace", "panelDock", panel, "size", Math.max(120, Math.min(800, Math.round(size))));
  persist();
}

export function setPanelFloatGeom(panel: PanelId, geom: { x: number; y: number; w: number; h: number }) {
  ensureDock();
  setState("workspace", "panelDock", panel, "floatGeom", geom);
  persist();
}

export function togglePanelVisible(panel: PanelId) {
  ensureDock();
  const cur = state.workspace.panelDock![panel];
  if (cur.slot === "hidden") setPanelSlot(panel, cur.lastDockSlot ?? "left");
  else setPanelSlot(panel, "hidden");
}

/** 指定 slot に属するパネル ID を order 昇順で返す */
export function panelsInSlot(slot: DockSlot): PanelId[] {
  const pd = state.workspace.panelDock;
  if (!pd) return [];
  const list: { id: PanelId; order: number }[] = [];
  if (pd.tabs.slot === slot) list.push({ id: "tabs", order: pd.tabs.order });
  if (pd.tree.slot === slot) list.push({ id: "tree", order: pd.tree.order });
  list.sort((a, b) => a.order - b.order);
  return list.map((x) => x.id);
}
