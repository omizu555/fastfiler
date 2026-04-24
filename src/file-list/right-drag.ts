// 右ボタン D&D: 押下→閾値超でゴースト追従、放したらコピー/移動/キャンセル メニュー表示
import { createSignal } from "solid-js";
import { joinPath } from "../path-util";
import { performDrop } from "../dnd";

export interface RightDragPayload {
  paths: string[];
  sourcePath: string;
  label: string;
}

interface ActiveDrag {
  x: number;
  y: number;
  payload: RightDragPayload;
}

interface PendingMenu {
  x: number;
  y: number;
  payload: RightDragPayload;
  destPath: string;
}

const [active, setActive] = createSignal<ActiveDrag | null>(null);
const [menu, setMenu] = createSignal<PendingMenu | null>(null);

export const rightDragActive = active;
export const rightDragMenu = menu;
export const closeRightDragMenu = () => setMenu(null);

let pending: { sx: number; sy: number; payload: RightDragPayload } | null = null;
let suppressNextContext = false;
const THRESHOLD2 = 5 * 5;

export function beginRightDragCandidate(payload: RightDragPayload, x: number, y: number) {
  pending = { sx: x, sy: y, payload };
  suppressNextContext = false;
}

function findDest(x: number, y: number): { paneId: string | null; path: string | null } {
  const els = document.elementsFromPoint(x, y);
  let resultPath: string | null = null;
  let resultPane: string | null = null;
  for (const el of els) {
    const e = el as HTMLElement;
    const row = e.closest?.("[data-rd-folder]") as HTMLElement | null;
    if (row && !resultPath) {
      const pp = row.getAttribute("data-rd-pane-path") ?? "";
      const name = row.getAttribute("data-rd-name") ?? "";
      if (pp && name) resultPath = joinPath(pp, name);
    }
    const pane = e.closest?.("[data-pane-id]") as HTMLElement | null;
    if (pane && !resultPane) {
      resultPane = pane.getAttribute("data-pane-id");
      if (!resultPath) {
        const pp = pane.getAttribute("data-rd-pane-path");
        if (pp) resultPath = pp;
      }
    }
    if (resultPath && resultPane) break;
  }
  return { paneId: resultPane, path: resultPath };
}

function onMove(e: MouseEvent) {
  if (pending && !active()) {
    const dx = e.clientX - pending.sx;
    const dy = e.clientY - pending.sy;
    if (dx * dx + dy * dy > THRESHOLD2) {
      setActive({ x: e.clientX, y: e.clientY, payload: pending.payload });
    }
  } else if (active()) {
    const a = active()!;
    setActive({ ...a, x: e.clientX, y: e.clientY });
  }
}

function onUp(e: MouseEvent) {
  if (e.button !== 2) return;
  if (active()) {
    const a = active()!;
    setActive(null);
    suppressNextContext = true;
    const dest = findDest(e.clientX, e.clientY);
    if (dest.path) {
      setMenu({ x: e.clientX, y: e.clientY, payload: a.payload, destPath: dest.path });
    }
  }
  pending = null;
}

function onContextCapture(e: MouseEvent) {
  if (suppressNextContext) {
    e.preventDefault();
    e.stopPropagation();
    suppressNextContext = false;
  }
}

let installed = false;
export function ensureRightDragInstalled() {
  if (installed) return;
  installed = true;
  window.addEventListener("mousemove", onMove);
  window.addEventListener("mouseup", onUp);
  window.addEventListener("contextmenu", onContextCapture, true);
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      pending = null;
      setActive(null);
      setMenu(null);
    }
  });
}

export async function executeRightDrag(kind: "move" | "copy") {
  const m = menu();
  if (!m) return;
  setMenu(null);
  await performDrop({
    paths: m.payload.paths,
    destPath: m.destPath,
    op: kind,
    sourceDir: m.payload.sourcePath,
    logTag: "[right-drag]",
  });
}

