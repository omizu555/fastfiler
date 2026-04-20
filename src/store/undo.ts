import type { UndoEntry, UndoOp } from "../types";
import { state, setState } from "./core";

const UNDO_MAX = 20;
let undoSeq = 0;

export function pushUndo(label: string, ops: UndoOp[]) {
  if (ops.length === 0) return;
  const entry: UndoEntry = { id: ++undoSeq, label, ops, ts: Date.now() };
  setState("undoStack", (xs) => {
    const next = [...xs, entry];
    if (next.length > UNDO_MAX) next.splice(0, next.length - UNDO_MAX);
    return next;
  });
  return entry;
}

export function popUndo(): UndoEntry | null {
  const stack = state.undoStack;
  if (stack.length === 0) return null;
  const last = stack[stack.length - 1];
  setState("undoStack", (xs) => xs.slice(0, -1));
  return last;
}

export function clearUndo() { setState("undoStack", []); }
