import { createSignal } from "solid-js";
import type { DockSlot, PanelId } from "./types";
import { setPanelSlot, setPanelOrder, panelsInSlot, state } from "./store";

interface DragState {
  panel: PanelId;
  hoverSlot: DockSlot | null;
  hoverInsertBefore: PanelId | null;
}

const [drag, setDrag] = createSignal<DragState | null>(null);
export const dragState = drag;

export function startPanelDrag(panel: PanelId) {
  setDrag({ panel, hoverSlot: null, hoverInsertBefore: null });
  document.body.classList.add("dragging-panel");
}

export function setHoverSlot(slot: DockSlot | null, insertBefore: PanelId | null = null) {
  const d = drag();
  if (!d) return;
  if (d.hoverSlot === slot && d.hoverInsertBefore === insertBefore) return;
  setDrag({ ...d, hoverSlot: slot, hoverInsertBefore: insertBefore });
}

export function endPanelDrag(commit: boolean) {
  const d = drag();
  setDrag(null);
  document.body.classList.remove("dragging-panel");
  if (!commit || !d || !d.hoverSlot) return;
  const slot = d.hoverSlot;
  const panel = d.panel;
  const insertBefore = d.hoverInsertBefore;

  // 既存メンバーの中で挿入位置 (order) を計算
  const existing = panelsInSlot(slot).filter((p) => p !== panel);
  const newOrder: PanelId[] = [];
  if (insertBefore && existing.includes(insertBefore)) {
    for (const p of existing) {
      if (p === insertBefore) newOrder.push(panel);
      newOrder.push(p);
    }
  } else {
    newOrder.push(...existing, panel);
  }

  setPanelSlot(panel, slot);
  newOrder.forEach((p, i) => setPanelOrder(p, i));
}

export function isDragging(): boolean {
  return drag() !== null;
}

export function panelDock(panel: PanelId) {
  return state.workspace.panelDock?.[panel];
}
