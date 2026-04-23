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

// ---- 外部D&D (エクスプローラ等からのドラッグ) グローバル状態 ----
// App.tsx の OLE イベントリスナーが更新し、FileList.tsx が読み取る。
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

export function createDnd(ctx: DndCtx) {
  const [dragOverRow, setDragOverRow] = createSignal<string | null>(null);
  const [paneDragOver, setPaneDragOver] = createSignal(false);
  // v1.6 (16.2): ドラッグ中の選択パスを保持し、自己/親→子へのドロップを抑止
  let dragSourcePaths: string[] | null = null;

  // パスを正規化 (末尾セパレータ除去 + 小文字化)
  const norm = (p: string) => p.replace(/[\\/]+$/, "").toLowerCase();
  // dest が src と同じか、src の子孫か?
  const isSelfOrDescendant = (src: string, dest: string): boolean => {
    const a = norm(src);
    const b = norm(dest);
    if (a === b) return true;
    return b.startsWith(a + "\\") || b.startsWith(a + "/");
  };
  const isInvalidDrop = (paths: string[] | null, destPath: string): boolean => {
    if (!paths || paths.length === 0) return false;
    return paths.some((p) => isSelfOrDescendant(p, destPath));
  };

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
    dragSourcePaths = payload.paths;
    ev.dataTransfer.setData(DRAG_MIME, JSON.stringify(payload));
    ev.dataTransfer.effectAllowed = "copyMove";
  };

  const onPaneDragOver = (ev: DragEvent) => {
    if (!ev.dataTransfer?.types.includes(DRAG_MIME)) return;
    // 自己/親→子へのドロップは禁止
    if (isInvalidDrop(dragSourcePaths, ctx.pane().path)) {
      ev.dataTransfer.dropEffect = "none";
      return;
    }
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
    if (!raw) { dragSourcePaths = null; return; }
    let payload: DragPayload;
    try { payload = JSON.parse(raw); } catch { dragSourcePaths = null; return; }
    // v1.6 (16.2): 自己 / 親→子 へのドロップを拒否
    if (isInvalidDrop(payload.paths, destPath)) {
      pushToast("自分自身または配下フォルダへは移動/コピーできません", "warn");
      dragSourcePaths = null;
      return;
    }
    if (payload.sourcePath === destPath && !ev.ctrlKey) {
      dragSourcePaths = null;
      return; // 同フォルダ移動は無意味
    }
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
    dragSourcePaths = null;
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
    const destPath = joinPath(ctx.pane().path, entry.name);
    // v1.6 (16.2): 自己 / 親→子 へのドロップは禁止 (視覚フィードバックも出さない)
    if (isInvalidDrop(dragSourcePaths, destPath)) {
      ev.dataTransfer.dropEffect = "none";
      return;
    }
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
