// 右ボタン D&D: 押下→閾値超でゴースト追従、放したらコピー/移動/キャンセル メニュー表示
import { createSignal } from "solid-js";
import { joinPath } from "../path-util";
import { runFileJob } from "../jobs";
import { pushUndo, bumpRefreshPaths, pushToast } from "../store";
import { resolveDestinations, refreshTargets } from "./resolve-dest";
import type { UndoOp } from "../types";

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

function findDest(x: number, y: number): string | null {
  const els = document.elementsFromPoint(x, y);
  for (const el of els) {
    const e = el as HTMLElement;
    const row = e.closest?.("[data-rd-folder]") as HTMLElement | null;
    if (row) {
      const pp = row.getAttribute("data-rd-pane-path") ?? "";
      const name = row.getAttribute("data-rd-name") ?? "";
      if (pp && name) return joinPath(pp, name);
    }
    const pane = e.closest?.("[data-rd-pane-path]") as HTMLElement | null;
    if (pane) {
      const pp = pane.getAttribute("data-rd-pane-path");
      if (pp) return pp;
    }
  }
  return null;
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
    // 直後の contextmenu を抑止
    suppressNextContext = true;
    const dest = findDest(e.clientX, e.clientY);
    if (dest && dest !== a.payload.sourcePath + "::__cancel__") {
      setMenu({ x: e.clientX, y: e.clientY, payload: a.payload, destPath: dest });
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
  const items = await resolveDestinations(m.payload.paths, m.destPath, kind);
  if (items.length === 0) {
    pushToast("対象がありません (同じ場所への移動)", "info");
    return;
  }
  const renamedCount = items.filter((it) => it.renamed).length;
  const label = `${kind === "copy" ? "コピー" : "移動"} ${items.length}件 → ${m.destPath}`;
  const r = await runFileJob(kind, items.map(({ from, to }) => ({ from, to })), { label });
  if (r.ok) {
    const ops: UndoOp[] = items.map((it) =>
      kind === "copy"
        ? ({ kind: "copy", created: it.to } as UndoOp)
        : ({ kind: "move", from: it.from, to: it.to } as UndoOp));
    pushUndo(label, ops);
    bumpRefreshPaths(refreshTargets(items, m.destPath, kind === "move").concat(m.payload.sourcePath));
    const note = renamedCount > 0 ? ` (${renamedCount}件は名前変更)` : "";
    pushToast(`${kind === "copy" ? "コピー" : "移動"} ${items.length}件 完了${note}`, "info");
  } else if (!r.canceled) {
    console.error(`[right-drag] ${label} 失敗`);
    pushToast(`${kind === "copy" ? "コピー" : "移動"} 失敗`, "error");
  }
}
