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

function endPointerDrag(): void {
  pointerCand = null;
  if (pointerActive) {
    pointerActive = false;
    document.body.style.cursor = "";
    clearExtDragOver();
  }
  osDragLaunched = false;
}

function updateCursor(ctrl: boolean): void {
  document.body.style.cursor = ctrl ? "copy" : "move";
}

function onMouseDownCapture(ev: MouseEvent): void {
  if (ev.button !== 0) return;
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
  // 現選択を取得 (リアクティブは不要、一回読みで十分)
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
    // 閾値超過 → ドラッグ確定
    if (ev.shiftKey) {
      // Shift → OS ドラッグへ移行
      osDragLaunched = true;
      const cand = pointerCand;
      pointerCand = null;
      pointerActive = false;
      void oleStartDrag(cand.paths, 0x7).catch((err) =>
        console.warn("[internal-pointer] oleStartDrag failed:", err),
      );
      return;
    }
    pointerActive = true;
  }
  updateCursor(ev.ctrlKey);
  const hit = hitTest(ev.clientX, ev.clientY);
  if (hit.paneId) setExtDragOver(hit.paneId, hit.folderName);
  else clearExtDragOver();
}

async function onMouseUp(ev: MouseEvent): Promise<void> {
  if (!pointerCand) return;
  const cand = pointerCand;
  const wasActive = pointerActive;
  endPointerDrag();
  if (!wasActive) return; // クリック相当: 何もしない
  const hit = hitTest(ev.clientX, ev.clientY);
  if (!hit.destPath) return;
  // 同一ペイン空白に drop (= 元の親ディレクトリと同じ) → no-op
  if (
    !hit.folderName &&
    hit.destPath === cand.sourcePath &&
    hit.paneId === cand.sourcePaneId
  ) {
    return;
  }
  const op = decideOp({
    ctrlKey: ev.ctrlKey,
    isInternal: true,
    srcPaths: cand.paths,
    dstPath: hit.destPath,
  });
  await performDrop({
    paths: cand.paths,
    destPath: hit.destPath,
    op,
    sourceDir: cand.sourcePath,
    targetPaneId: hit.paneId ?? cand.sourcePaneId,
    logTag: "[internal-drop]",
  });
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
  return () => {
    installed = false;
    window.removeEventListener("mousedown", onMouseDownCapture, true);
    window.removeEventListener("mousemove", onMouseMove, true);
    window.removeEventListener("mouseup", onMouseUp, true);
    window.removeEventListener("keydown", onKeyDown, true);
    window.removeEventListener("blur", onWindowBlur);
  };
}
