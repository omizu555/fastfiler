// FileList の D&D ハンドラ群
// v1.7.1: HTML5 D&D を復活し、アプリ内 (同一/別ペイン/フォルダ行) のドロップを処理。
// Shift+ドラッグで OS ドラッグ (oleStartDrag) を起動 → エクスプローラ等へ出せる。
// 通常ドラッグはアプリ内で完結。Ctrl 併用でコピー、それ以外は移動。
// 外部 (エクスプローラ) からのドロップは App.tsx の OLE Drop ターゲットが受ける。
import { createSignal } from "solid-js";
import type { FileEntry, PaneState, UndoOp } from "../types";
import {
  state,
  setPaneSelection,
  pushUndo,
  bumpRefreshPaths,
  pushToast,
} from "../store";
import { oleStartDrag } from "../fs";
import { joinPath } from "../path-util";
import { resolveDestinations, refreshTargets } from "./resolve-dest";
import { runFileJob } from "../jobs";

export const DRAG_MIME = "application/x-fastfiler";

// ---- アプリ内ドラッグ中のパス記録 (Tauri D&D drop で内部/外部を識別するため) ----
// `dragDropEnabled: true` 環境では HTML5 drop が抑制されるため、自プロセス内の
// ドロップも Tauri D&D drop 経由で受ける。drop の paths がここに保存した集合と
// 一致したら「内部 D&D」として処理し、違えば「外部 (エクスプローラ等)」扱い。
let _internalDragPaths: string[] | null = null;
let _internalDragSource: string | null = null;

export function getInternalDragPaths(): { paths: string[]; sourcePath: string } | null {
  if (!_internalDragPaths || !_internalDragSource) return null;
  return { paths: _internalDragPaths, sourcePath: _internalDragSource };
}

export function clearInternalDrag(): void {
  _internalDragPaths = null;
  _internalDragSource = null;
}

/** drop 時の paths が internal source と一致するか (順不同・同一集合) */
export function isInternalDropPaths(dropped: string[]): boolean {
  if (!_internalDragPaths) return false;
  if (dropped.length !== _internalDragPaths.length) return false;
  const a = new Set(_internalDragPaths.map((p) => p.toLowerCase()));
  return dropped.every((p) => a.has(p.toLowerCase()));
}

// 安全装置: ドラッグ後一定時間で自動クリア (drop されなかったケース)
function scheduleAutoClear(): void {
  setTimeout(() => clearInternalDrag(), 30_000);
}

// ---- 外部D&D (エクスプローラ等からのドラッグ) グローバル状態 ----
const [_extDragPaneId, _setExtDragPaneId] = createSignal<string | null>(null);
const [_extDragRowName, _setExtDragRowName] = createSignal<string | null>(null);

export const extDragPaneId = _extDragPaneId;
export const extDragRowName = _extDragRowName;

export function setExtDragOver(paneId: string, rowName: string | null): void {
  _setExtDragPaneId(paneId);
  _setExtDragRowName(rowName);
}

export function clearExtDragOver(): void {
  _setExtDragPaneId(null);
  _setExtDragRowName(null);
}

export interface DragPayload {
  paths: string[];
  sourcePath: string;
}

export interface DndCtx {
  paneId: string;
  pane: () => PaneState;
  refetch: () => void;
}

async function runInternalDnd(
  paths: string[],
  destPath: string,
  op: "copy" | "move",
  sourcePath: string,
  refetch: () => void,
) {
  const items = await resolveDestinations(paths, destPath, op);
  if (items.length === 0) {
    pushToast("対象がありません (同じ場所への移動)", "info");
    return;
  }
  const renamedCount = items.filter((it) => it.renamed).length;
  const label = `${op === "copy" ? "コピー" : "移動"} ${items.length}件 → ${destPath}`;
  const r = await runFileJob(op, items.map(({ from, to }) => ({ from, to })), { label });
  if (r.ok) {
    const ops: UndoOp[] = items.map((it) =>
      op === "copy"
        ? ({ kind: "copy", created: it.to } as UndoOp)
        : ({ kind: "move", from: it.from, to: it.to } as UndoOp),
    );
    pushUndo(label, ops);
    bumpRefreshPaths(refreshTargets(items, destPath, op === "move").concat(sourcePath));
    const note = renamedCount > 0 ? ` (${renamedCount}件は名前変更)` : "";
    pushToast(`${op === "copy" ? "コピー" : "移動"} ${items.length}件 完了${note}`, "info");
  } else if (!r.canceled) {
    console.error(`[dnd] ${label} 失敗`);
    pushToast(`${op === "copy" ? "コピー" : "移動"} 失敗`, "error");
  }
  refetch();
}

// ---- v1.7.4: pointer ベースのアプリ内 D&D エンジン ----
// dragDropEnabled: true 環境では HTML5 drop も Tauri D&D drop も自プロセス内ドロップを
// 拾えないため、mousedown / mousemove / mouseup を window レベルで監視して自前で
// アプリ内 D&D を実現する。Shift+ドラッグは従来通り HTML5 dragstart 経由で OS ドラッグ。
const PointerDndDragThreshold = 5; // px
let pointerInstalled = false;
const refetchByPaneId = new Map<string, () => void>();

export function registerPaneRefetch(paneId: string, fn: () => void): () => void {
  refetchByPaneId.set(paneId, fn);
  return () => {
    if (refetchByPaneId.get(paneId) === fn) refetchByPaneId.delete(paneId);
  };
}

interface PointerCandidate {
  paths: string[];
  sourcePath: string;
  sourcePaneId: string;
  startX: number;
  startY: number;
}

let pointerCand: PointerCandidate | null = null;
let pointerActive = false;

function findRowFromPoint(x: number, y: number): {
  paneEl: HTMLElement | null;
  paneId: string | null;
  folderRow: HTMLElement | null;
  folderName: string | null;
  panePath: string | null;
} {
  const els = document.elementsFromPoint(x, y) as HTMLElement[];
  const folderRow =
    (els.find((el) => el.dataset && el.dataset.rdFolder === "1") as HTMLElement | undefined) ??
    null;
  const paneEl =
    (els.find((el) => el.dataset && el.dataset.paneId) as HTMLElement | undefined) ?? null;
  return {
    paneEl,
    paneId: paneEl?.dataset.paneId ?? null,
    folderRow,
    folderName: folderRow?.dataset.rdName ?? null,
    panePath: paneEl?.dataset.rdPanePath ?? null,
  };
}

function endPointerDrag(): void {
  pointerCand = null;
  if (pointerActive) {
    pointerActive = false;
    document.body.style.cursor = "";
    clearExtDragOver();
  }
}

function onPointerMove(ev: MouseEvent): void {
  if (!pointerCand) return;
  const dx = ev.clientX - pointerCand.startX;
  const dy = ev.clientY - pointerCand.startY;
  if (!pointerActive) {
    if (Math.abs(dx) + Math.abs(dy) < PointerDndDragThreshold) return;
    pointerActive = true;
    document.body.style.cursor = ev.ctrlKey ? "copy" : "move";
  }
  // 視覚フィードバック更新
  document.body.style.cursor = ev.ctrlKey ? "copy" : "move";
  const hit = findRowFromPoint(ev.clientX, ev.clientY);
  if (hit.paneId) {
    setExtDragOver(hit.paneId, hit.folderName);
  } else {
    clearExtDragOver();
  }
}

async function onPointerUp(ev: MouseEvent): Promise<void> {
  if (!pointerCand) return;
  const cand = pointerCand;
  const wasActive = pointerActive;
  endPointerDrag();
  if (!wasActive) return; // ただのクリック → 何もしない
  // 着地点を解決
  const hit = findRowFromPoint(ev.clientX, ev.clientY);
  let dest: string | null = null;
  if (hit.folderRow && hit.panePath && hit.folderName) {
    dest = joinPath(hit.panePath, hit.folderName);
  } else if (hit.paneId) {
    dest = hit.panePath; // ペイン空白 → そのペインのカレント
  }
  if (!dest) return;
  // 同一ペインの空白に dropped (= ソース親フォルダと同じ) → 何もしない
  if (!hit.folderRow && dest === cand.sourcePath && hit.paneId === cand.sourcePaneId) {
    return;
  }
  const op: "copy" | "move" = ev.ctrlKey ? "copy" : "move";
  const refetch =
    (hit.paneId ? refetchByPaneId.get(hit.paneId) : undefined) ??
    refetchByPaneId.get(cand.sourcePaneId) ??
    (() => {});
  await runInternalDnd(cand.paths, dest, op, cand.sourcePath, refetch);
}

function onPointerDownCapture(ev: MouseEvent): void {
  if (ev.button !== 0) return;
  if (ev.shiftKey) return; // Shift+ドラッグは HTML5 OS ドラッグへ
  const target = ev.target as HTMLElement | null;
  if (!target) return;
  const row = target.closest("tr[data-rd-name]") as HTMLElement | null;
  if (!row) return;
  const paneEl = target.closest("[data-pane-id]") as HTMLElement | null;
  if (!paneEl) return;
  const panePath = row.dataset.rdPanePath ?? paneEl.dataset.rdPanePath;
  const paneId = paneEl.dataset.paneId;
  const name = row.dataset.rdName;
  if (!panePath || !paneId || !name) return;
  // 現在の選択を取得 (リアクティブな pane state を直接見る)
  const pane = state.panes[paneId];
  let sel = pane?.selection ?? [];
  if (!sel.includes(name)) sel = [name];
  const paths = sel.map((n) => joinPath(panePath, n));
  pointerCand = {
    paths,
    sourcePath: panePath,
    sourcePaneId: paneId,
    startX: ev.clientX,
    startY: ev.clientY,
  };
  pointerActive = false;
}

function onKeyDownCancel(ev: KeyboardEvent): void {
  if (ev.key === "Escape") endPointerDrag();
}

export function installInternalPointerDnd(): () => void {
  if (pointerInstalled) return () => {};
  pointerInstalled = true;
  window.addEventListener("mousedown", onPointerDownCapture, true);
  window.addEventListener("mousemove", onPointerMove, true);
  window.addEventListener("mouseup", onPointerUp, true);
  window.addEventListener("keydown", onKeyDownCancel, true);
  return () => {
    pointerInstalled = false;
    window.removeEventListener("mousedown", onPointerDownCapture, true);
    window.removeEventListener("mousemove", onPointerMove, true);
    window.removeEventListener("mouseup", onPointerUp, true);
    window.removeEventListener("keydown", onKeyDownCancel, true);
  };
}

export function createDnd(ctx: DndCtx) {
  const [dragOverRow, setDragOverRow] = createSignal<string | null>(null);
  const [paneDragOver, setPaneDragOver] = createSignal(false);

  const onRowDragStart = (ev: DragEvent, name: string) => {
    if (!ev.dataTransfer) return;
    let sel = ctx.pane().selection;
    if (!sel.includes(name)) {
      setPaneSelection(ctx.paneId, [name]);
      sel = [name];
    }
    const fullSel = sel.map((n) => joinPath(ctx.pane().path, n));
    // Shift+ドラッグ → OS ドラッグでエクスプローラ等へ出せる
    if (ev.shiftKey) {
      ev.preventDefault();
      void oleStartDrag(fullSel, 0x7).catch((err) =>
        console.warn("oleStartDrag:", err),
      );
      return;
    }
    // 通常ドラッグ:
    // - HTML5 dragstart は WebView2 で発火するが、dragDropEnabled=true 環境では
    //   dragover/drop が抑制されるため自プロセス drop は Tauri D&D drop へ流れる。
    // - 内部識別のためグローバル変数に paths を保存。drop 時に App.tsx の
    //   Tauri D&D drop ハンドラが getInternalDragPaths() で参照する。
    // - HTML5 互換用に DRAG_MIME も載せる (古い経路が動けば即時処理される)。
    _internalDragPaths = fullSel.slice();
    _internalDragSource = ctx.pane().path;
    scheduleAutoClear();
    const payload: DragPayload = { paths: fullSel, sourcePath: ctx.pane().path };
    try {
      ev.dataTransfer.setData(DRAG_MIME, JSON.stringify(payload));
    } catch {
      /* dragDropEnabled=true 下では setData が制限される場合あり */
    }
    ev.dataTransfer.effectAllowed = "copyMove";
  };

  const onPaneDragOver = (ev: DragEvent) => {
    if (!ev.dataTransfer?.types.includes(DRAG_MIME)) return;
    ev.preventDefault();
    ev.dataTransfer.dropEffect = ev.ctrlKey ? "copy" : "move";
    setPaneDragOver(true);
  };

  const onPaneDragLeave = () => {
    setPaneDragOver(false);
  };

  const handleDrop = async (ev: DragEvent, dest: string) => {
    setPaneDragOver(false);
    const data = ev.dataTransfer?.getData(DRAG_MIME);
    if (!data) return;
    ev.preventDefault();
    let payload: DragPayload;
    try {
      payload = JSON.parse(data);
    } catch {
      return;
    }
    if (!payload.paths?.length) return;
    const op: "copy" | "move" = ev.ctrlKey ? "copy" : "move";
    await runInternalDnd(payload.paths, dest, op, payload.sourcePath, ctx.refetch);
  };

  const onRowDragOver = (ev: DragEvent, entry: FileEntry) => {
    if (!ev.dataTransfer?.types.includes(DRAG_MIME)) return;
    if (!(entry.kind === "dir")) return;
    ev.preventDefault();
    ev.stopPropagation();
    ev.dataTransfer.dropEffect = ev.ctrlKey ? "copy" : "move";
    setDragOverRow(entry.name);
    setPaneDragOver(false);
  };

  const onRowDragLeave = (entry: FileEntry) => {
    if (dragOverRow() === entry.name) setDragOverRow(null);
  };

  const onRowDrop = async (ev: DragEvent, entry: FileEntry) => {
    setDragOverRow(null);
    if (!(entry.kind === "dir")) return;
    const data = ev.dataTransfer?.getData(DRAG_MIME);
    if (!data) return;
    ev.preventDefault();
    ev.stopPropagation();
    let payload: DragPayload;
    try {
      payload = JSON.parse(data);
    } catch {
      return;
    }
    if (!payload.paths?.length) return;
    const dest = joinPath(ctx.pane().path, entry.name);
    const op: "copy" | "move" = ev.ctrlKey ? "copy" : "move";
    await runInternalDnd(payload.paths, dest, op, payload.sourcePath, ctx.refetch);
  };

  return {
    dragOverRow,
    paneDragOver,
    onRowDragStart,
    onPaneDragOver,
    onPaneDragLeave,
    handleDrop,
    onRowDragOver,
    onRowDrop,
    onRowDragLeave,
  };
}
