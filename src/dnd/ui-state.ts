// D&D ホバー中の視覚フィードバック用シグナル
// 内部 (pointer エンジン) / 外部 (OLE / WebView2) どちらの経路からも更新される
import { createSignal } from "solid-js";

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

// 各 FileList ペインの refetch を ID で集中管理
// (D&D 完了後に着地ペインのリストを更新するため)
const refetchByPaneId = new Map<string, () => void>();

export function registerPaneRefetch(paneId: string, fn: () => void): () => void {
  refetchByPaneId.set(paneId, fn);
  return () => {
    if (refetchByPaneId.get(paneId) === fn) refetchByPaneId.delete(paneId);
  };
}

export function getPaneRefetch(paneId: string | null | undefined): (() => void) | undefined {
  if (!paneId) return undefined;
  return refetchByPaneId.get(paneId);
}
