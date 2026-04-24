// アプリ内 D&D を pointer (mousedown/move/up) ベースで実現する自前エンジン
//
// 背景:
//   - Tauri 2 の `dragDropEnabled: true` は外部ドロップ受信に必須だが、
//     副作用として WebView2 内の HTML5 dragover/drop が抑制される。
//   - 行に `draggable={true}` を付けると HTML5 ドラッグ機構が起動し、
//     pointer 系イベント (mousemove/mouseup) も奪われ自前エンジンも完走しない。
//   → 行から `draggable` を完全に外し、左ボタンの mousedown/move/up を
//     window レベルで監視して自前で D&D を実装する。
//   - Shift+ドラッグは閾値を超えた時点で `oleStartDrag` を呼び OS ドラッグへ移行
//     (mousedown 即時起動は Shift+クリック範囲選択を壊すため不可)。
//   - Ctrl は離れたタイミングの値を採用 (mouseup 時の修飾キー)。

import { state } from "../store";
import { joinPath } from "../path-util";
import { oleStartDrag } from "../fs";
import { hitTest } from "./hit-test";
import { decideOp } from "./policy";
import { performDrop } from "./perform";
import { setExtDragOver, clearExtDragOver } from "./ui-state";

const DRAG_THRESHOLD_PX = 5;

interface PointerCandidate {
  paths: string[];
  sourcePath: string;
  sourcePaneId: string;
  startX: number;
  startY: number;
}

let pointerCand: PointerCandidate | null = null;
let pointerActive = false;
let osDragLaunched = false;
let installed = false;
let lastMoveSig = "";

function endPointerDrag(): void {
  pointerCand = null;
  if (pointerActive) {
    pointerActive = false;
    document.body.style.cursor = "";
    clearExtDragOver();
  }
  osDragLaunched = false;
  lastMoveSig = "";
}

function updateCursor(ctrl: boolean): void {
  document.body.style.cursor = ctrl ? "copy" : "move";
}

function onMouseDownCapture(ev: MouseEvent): void {
  if (ev.button !== 0) return;
  const target = ev.target as HTMLElement | null;
  if (!target) return;
  const row = target.closest("tr[data-rd-name]") as HTMLElement | null;
  if (!row) {
    // ログは過剰になるので row 不在は出さない
    return;
  }
  const paneEl = target.closest("[data-pane-id]") as HTMLElement | null;
  if (!paneEl) {
    console.info("[dnd] mousedown: row found but no [data-pane-id] ancestor");
    return;
  }
  const panePath = row.dataset.rdPanePath ?? paneEl.dataset.rdPanePath;
  const paneId = paneEl.dataset.paneId;
  const name = row.dataset.rdName;
  console.info("[dnd] mousedown candidate", { paneId, panePath, name, sel: state.panes[paneId ?? ""]?.selection });
  if (!panePath || !paneId || !name) {
    console.warn("[dnd] mousedown: missing dataset", { panePath, paneId, name });
    return;
  }
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
  osDragLaunched = false;
}

function onMouseMove(ev: MouseEvent): void {
  if (!pointerCand || osDragLaunched) return;
  const dx = ev.clientX - pointerCand.startX;
  const dy = ev.clientY - pointerCand.startY;
  if (!pointerActive) {
    if (Math.abs(dx) + Math.abs(dy) < DRAG_THRESHOLD_PX) return;
    if (ev.shiftKey) {
      console.info("[dnd] threshold exceeded with Shift → oleStartDrag", pointerCand.paths);
      osDragLaunched = true;
      const cand = pointerCand;
      pointerCand = null;
      pointerActive = false;
      void oleStartDrag(cand.paths, 0x7)
        .then(() => console.info("[dnd] oleStartDrag resolved"))
        .catch((err) => console.warn("[dnd] oleStartDrag failed:", err));
      return;
    }
    pointerActive = true;
    console.info("[dnd] drag started (internal)", pointerCand.paths);
  }
  updateCursor(ev.ctrlKey);
  const hit = hitTest(ev.clientX, ev.clientY);
  const sig = `${hit.paneId ?? ""}|${hit.folderName ?? ""}|${hit.destPath ?? ""}`;
  if (sig !== lastMoveSig) {
    console.info("[dnd] move hit changed", { x: ev.clientX, y: ev.clientY, ...hit });
    lastMoveSig = sig;
  }
  if (hit.paneId) setExtDragOver(hit.paneId, hit.folderName);
  else clearExtDragOver();
}

/**
 * v1.9: ウインドウ外に出た瞬間に OS ドラッグへ昇格 (Shift 不要)。
 *       内部 D&D 中に mouseleave が document で発火したら oleStartDrag を呼ぶ。
 */
function onDocumentMouseLeave(_ev: MouseEvent): void {
  if (!pointerCand || osDragLaunched) return;
  if (!pointerActive) return; // ドラッグ確定前は無視
  console.info("[dnd] mouseleave document → escalate to OS drag", pointerCand.paths);
  osDragLaunched = true;
  const cand = pointerCand;
  pointerCand = null;
  pointerActive = false;
  document.body.style.cursor = "";
  clearExtDragOver();
  lastMoveSig = "";
  void oleStartDrag(cand.paths, 0x7)
    .then(() => console.info("[dnd] oleStartDrag (escalation) resolved"))
    .catch((err) => console.warn("[dnd] oleStartDrag (escalation) failed:", err));
}

async function onMouseUp(ev: MouseEvent): Promise<void> {
  if (!pointerCand) {
    return;
  }
  const cand = pointerCand;
  const wasActive = pointerActive;
  endPointerDrag();
  console.info("[dnd] mouseup", { wasActive, x: ev.clientX, y: ev.clientY });
  if (!wasActive) return;
  const hit = hitTest(ev.clientX, ev.clientY);
  console.info("[dnd] mouseup hit", hit);
  if (!hit.destPath) {
    // 詳細: その地点の elementsFromPoint を出して原因を見る
    const els = document.elementsFromPoint(ev.clientX, ev.clientY) as HTMLElement[];
    console.warn("[dnd] mouseup: no destPath, drop cancelled. elementsFromPoint:",
      els.slice(0, 8).map((e) => ({
        tag: e.tagName,
        cls: (e.className as unknown as { toString?: () => string })?.toString?.() ?? "",
        ds: { ...e.dataset },
      })),
    );
    return;
  }
  if (
    !hit.folderName &&
    hit.destPath === cand.sourcePath &&
    hit.paneId === cand.sourcePaneId
  ) {
    console.info("[dnd] drop on same pane blank → no-op");
    return;
  }
  const op = decideOp({
    ctrlKey: ev.ctrlKey,
    isInternal: true,
    srcPaths: cand.paths,
    dstPath: hit.destPath,
  });
  console.info("[dnd] performDrop", { op, paths: cand.paths, dest: hit.destPath, sourceDir: cand.sourcePath, targetPaneId: hit.paneId ?? cand.sourcePaneId });
  await performDrop({
    paths: cand.paths,
    destPath: hit.destPath,
    op,
    sourceDir: cand.sourcePath,
    targetPaneId: hit.paneId ?? cand.sourcePaneId,
    logTag: "[internal-drop]",
  });
  console.info("[dnd] performDrop done");
}

function onKeyDown(ev: KeyboardEvent): void {
  if (ev.key === "Escape") endPointerDrag();
}

function onWindowBlur(): void {
  endPointerDrag();
}

export function installInternalPointerDnd(): () => void {
  if (installed) return () => {};
  installed = true;
  window.addEventListener("mousedown", onMouseDownCapture, true);
  window.addEventListener("mousemove", onMouseMove, true);
  window.addEventListener("mouseup", onMouseUp, true);
  window.addEventListener("keydown", onKeyDown, true);
  window.addEventListener("blur", onWindowBlur);
  document.documentElement.addEventListener("mouseleave", onDocumentMouseLeave);
  return () => {
    installed = false;
    window.removeEventListener("mousedown", onMouseDownCapture, true);
    window.removeEventListener("mousemove", onMouseMove, true);
    window.removeEventListener("mouseup", onMouseUp, true);
    window.removeEventListener("keydown", onKeyDown, true);
    window.removeEventListener("blur", onWindowBlur);
    document.documentElement.removeEventListener("mouseleave", onDocumentMouseLeave);
  };
}
