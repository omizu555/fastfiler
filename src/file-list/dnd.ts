// FileList の D&D ハンドラ群 (内部 D&D + spring-loaded folder + Alt+Drag drag-out)
// 状態 (dragOverRow / paneDragOver) と handler を作るファクトリ。
import { createSignal, onCleanup } from "solid-js";
import type { FileEntry, PaneState, UndoOp } from "../types";
import { setPanePath, setPaneSelection, pushUndo, pushToast, bumpRefreshPaths } from "../store";
import { oleStartDrag } from "../fs";
import { joinPath } from "../path-util";
import { runFileJob } from "../jobs";
import { performUndo } from "../undo";

export const DRAG_MIME = "application/x-fastfiler";

export interface DragPayload {
  paths: string[];
  sourcePath: string;
}

export interface DndCtx {
  paneId: string;
  pane: () => PaneState;
  refetch: () => void;
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
    // Alt+ドラッグ: Windows ネイティブ OS ドラッグ (エクスプローラ等への drag-out)
    if (ev.altKey) {
      ev.preventDefault();
      const fullSel = sel.map((n) => joinPath(ctx.pane().path, n));
      void oleStartDrag(fullSel, 0x7).catch((err) => console.warn("oleStartDrag:", err));
      return;
    }
    const payload: DragPayload = {
      paths: sel.map((n) => joinPath(ctx.pane().path, n)),
      sourcePath: ctx.pane().path,
    };
    ev.dataTransfer.setData(DRAG_MIME, JSON.stringify(payload));
    ev.dataTransfer.effectAllowed = "copyMove";
  };

  const onPaneDragOver = (ev: DragEvent) => {
    if (!ev.dataTransfer?.types.includes(DRAG_MIME)) return;
    ev.preventDefault();
    ev.dataTransfer.dropEffect = ev.ctrlKey ? "copy" : "move";
    setPaneDragOver(true);
  };
  const onPaneDragLeave = () => setPaneDragOver(false);

  const handleDrop = async (ev: DragEvent, destPath: string) => {
    ev.preventDefault();
    setPaneDragOver(false);
    setDragOverRow(null);
    const raw = ev.dataTransfer?.getData(DRAG_MIME);
    if (!raw) return;
    let payload: DragPayload;
    try { payload = JSON.parse(raw); } catch { return; }
    if (payload.sourcePath === destPath && !ev.ctrlKey) return; // 同フォルダ移動は無意味
    const isCopy = ev.ctrlKey;
    const items = payload.paths.map((src) => ({
      from: src,
      to: joinPath(destPath, src.split(/[\\/]/).pop() ?? "untitled"),
    }));
    const label = `${isCopy ? "コピー" : "移動"} ${items.length}件 → ${destPath}`;
    const r = await runFileJob(isCopy ? "copy" : "move", items, { label });
    if (r.ok) {
      const ops: UndoOp[] = items.map((it) =>
        isCopy ? { kind: "copy", created: it.to } : { kind: "move", from: it.from, to: it.to });
      pushUndo(label, ops);
      pushToast(label, "info", { label: "↶取り消し", onClick: () => { void performUndo(); } });
      bumpRefreshPaths([destPath, payload.sourcePath]);
    } else if (!r.canceled) {
      pushToast(`${label} 失敗`, "error");
    }
    ctx.refetch();
  };

  // v3.4: Spring-loaded folder (ホバー長押しで自動展開)
  let springTimer: number | null = null;
  let springName: string | null = null;
  const SPRING_DELAY = 800;
  const cancelSpring = () => {
    if (springTimer != null) { clearTimeout(springTimer); springTimer = null; }
    springName = null;
  };
  onCleanup(cancelSpring);

  const onRowDragOver = (ev: DragEvent, entry: FileEntry) => {
    if (entry.kind !== "dir") return;
    if (!ev.dataTransfer?.types.includes(DRAG_MIME)) return;
    ev.preventDefault();
    ev.stopPropagation();
    ev.dataTransfer.dropEffect = ev.ctrlKey ? "copy" : "move";
    setDragOverRow(entry.name);
    if (springName !== entry.name) {
      cancelSpring();
      springName = entry.name;
      springTimer = window.setTimeout(() => {
        if (dragOverRow() === entry.name) {
          setPanePath(ctx.paneId, joinPath(ctx.pane().path, entry.name));
        }
        cancelSpring();
      }, SPRING_DELAY);
    }
  };

  const onRowDrop = (ev: DragEvent, entry: FileEntry) => {
    if (entry.kind !== "dir") return;
    ev.stopPropagation();
    cancelSpring();
    void handleDrop(ev, joinPath(ctx.pane().path, entry.name));
  };

  const onRowDragLeave = (entry: FileEntry) => {
    if (dragOverRow() === entry.name) {
      setDragOverRow(null);
      cancelSpring();
    }
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
