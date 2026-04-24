import { renamePath, movePath, deletePath } from "./fs";
import { popUndo, state, pushToast } from "./store";
import type { UndoEntry, UndoOp } from "./types";

async function applyInverse(op: UndoOp): Promise<void> {
  switch (op.kind) {
    case "rename":
    case "move":
      // 元に戻す: to → from
      await (op.kind === "rename" ? renamePath : movePath)(op.to, op.from);
      return;
    case "copy":
      // コピーで作られたものを削除
      await deletePath(op.created, true);
      return;
  }
}

export async function performUndo(): Promise<boolean> {
  const entry = popUndo();
  if (!entry) {
    pushToast("元に戻す操作がありません", "info");
    return false;
  }
  const errors: string[] = [];
  // 逆順で適用 (後で行ったものから戻す)
  for (let i = entry.ops.length - 1; i >= 0; i--) {
    try {
      await applyInverse(entry.ops[i]);
    } catch (e) {
      errors.push(String(e));
    }
  }
  if (errors.length > 0) {
    console.error(`[undo] 取り消し失敗: ${entry.label} (${errors.length}件)`);
    pushToast(`取り消し失敗: ${entry.label}`, "error");
    return false;
  }
  pushToast(`取り消し: ${entry.label}`, "info");
  return true;
}

export function canUndo(): boolean {
  return state.undoStack.length > 0;
}
// UndoEntry を再エクスポートして外部 import 安定性を確保
export type { UndoEntry };

