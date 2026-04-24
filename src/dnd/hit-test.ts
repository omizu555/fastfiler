// elementsFromPoint(x, y) からドロップ先を解決する
// 優先順:
//   1. フォルダ行 (data-rd-folder="1")
//   2. パンくず (data-rd-crumb-path)
//   3. ペイン全体 (data-pane-id) → そのペインのカレントパス
import { joinPath } from "../path-util";

export interface DropTarget {
  paneId: string | null;
  /** 着地パス。フォルダ行ならフォルダパス / パンくずならその祖先 / ペイン空白ならカレント */
  destPath: string | null;
  /** ホバー中のフォルダ行の名前 (視覚フィードバック用) */
  folderName: string | null;
}

export function hitTest(x: number, y: number): DropTarget {
  const els = document.elementsFromPoint(x, y) as HTMLElement[];
  let folderRow: HTMLElement | null = null;
  let crumb: HTMLElement | null = null;
  let paneEl: HTMLElement | null = null;
  for (const el of els) {
    const ds = el.dataset;
    if (!ds) continue;
    if (!folderRow && ds.rdFolder === "1") folderRow = el;
    if (!crumb && ds.rdCrumbPath) crumb = el;
    if (!paneEl && ds.paneId) paneEl = el;
    if (folderRow && paneEl) break;
  }
  const paneId = paneEl?.dataset.paneId ?? null;
  let destPath: string | null = null;
  if (folderRow) {
    const pp = folderRow.dataset.rdPanePath;
    const name = folderRow.dataset.rdName;
    if (pp && name) destPath = joinPath(pp, name);
  } else if (crumb) {
    destPath = crumb.dataset.rdCrumbPath ?? null;
  } else if (paneEl) {
    destPath = paneEl.dataset.rdPanePath ?? null;
  }
  const result: DropTarget = {
    paneId,
    destPath,
    folderName: folderRow?.dataset.rdName ?? null,
  };
  return result;
}
