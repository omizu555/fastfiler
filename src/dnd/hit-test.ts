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
    // 行 / ペイン要素は <tr>/<div> のレイアウトの都合で
    // elementsFromPoint が返すのは内側の <td> / <span> / <img> のことがある。
    // 確実に拾うため closest で祖先を辿る。
    if (!folderRow) {
      const f = el.closest('[data-rd-folder="1"]') as HTMLElement | null;
      if (f) folderRow = f;
    }
    if (!crumb) {
      const c = el.closest("[data-rd-crumb-path]") as HTMLElement | null;
      if (c) crumb = c;
    }
    if (!paneEl) {
      const p = el.closest("[data-pane-id]") as HTMLElement | null;
      if (p) paneEl = p;
    }
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
  return {
    paneId,
    destPath,
    folderName: folderRow?.dataset.rdName ?? null,
  };
}
