import { batch } from "solid-js";
import type { LinkChannel, PaneNode, PaneUiState, SortKey } from "../types";
import { defaultPaneUi } from "../types";
import {
  state,
  setState,
  persist,
  nid,
  propagate,
  findAndReplace,
  removePane,
  updateRatio,
  ensurePaneUi,
} from "./core";

export function setPanePath(paneId: string, path: string, opts?: { fromHistory?: boolean }) {
  const pane = state.panes[paneId];
  if (!pane) return;
  const fromHistory = !!opts?.fromHistory;
  batch(() => {
    setState("panes", paneId, { path, selection: [], scrollTop: 0 });
    if (!fromHistory) {
      // 履歴を更新: 現在位置以降を切り捨ててから push
      const cur = pane.history ?? [pane.path];
      const idx = pane.historyIndex ?? cur.length - 1;
      // 連続する同一 path は重複させない
      const trimmed = cur.slice(0, idx + 1);
      if (trimmed[trimmed.length - 1] !== path) {
        trimmed.push(path);
      }
      // 上限 64 件 (古いものから破棄)
      const HISTORY_MAX = 64;
      const overflow = Math.max(0, trimmed.length - HISTORY_MAX);
      const next = overflow > 0 ? trimmed.slice(overflow) : trimmed;
      setState("panes", paneId, { history: next, historyIndex: next.length - 1 });
    }
    propagate(pane, "path", (other) => {
      if (fromHistory) {
        // 履歴ナビゲーションは伝搬しない (各ペイン独立)
        return;
      }
      setState("panes", other.id, { path, selection: [], scrollTop: 0 });
      // 連動先ペインの履歴も同様に更新 (連動操作はユーザー操作扱いとする)
      const ocur = other.history ?? [other.path];
      const oidx = other.historyIndex ?? ocur.length - 1;
      const otrim = ocur.slice(0, oidx + 1);
      if (otrim[otrim.length - 1] !== path) otrim.push(path);
      const HISTORY_MAX = 64;
      const ov = Math.max(0, otrim.length - HISTORY_MAX);
      const onext = ov > 0 ? otrim.slice(ov) : otrim;
      setState("panes", other.id, { history: onext, historyIndex: onext.length - 1 });
    });
    setState("focusedPaneId", paneId);
  });
  persist();
}

/** 履歴ナビゲーション可否 */
export function canGoBack(paneId: string): boolean {
  const p = state.panes[paneId];
  if (!p) return false;
  const idx = p.historyIndex ?? 0;
  return idx > 0;
}

export function canGoForward(paneId: string): boolean {
  const p = state.panes[paneId];
  if (!p) return false;
  const hist = p.history ?? [p.path];
  const idx = p.historyIndex ?? hist.length - 1;
  return idx < hist.length - 1;
}

/** 履歴を 1 つ前へ */
export function navigateBack(paneId: string): boolean {
  const p = state.panes[paneId];
  if (!p) return false;
  const hist = p.history ?? [p.path];
  const idx = p.historyIndex ?? hist.length - 1;
  if (idx <= 0) return false;
  const newIdx = idx - 1;
  const target = hist[newIdx];
  batch(() => {
    setState("panes", paneId, "historyIndex", newIdx);
    setPanePath(paneId, target, { fromHistory: true });
  });
  return true;
}

/** 履歴を 1 つ後へ */
export function navigateForward(paneId: string): boolean {
  const p = state.panes[paneId];
  if (!p) return false;
  const hist = p.history ?? [p.path];
  const idx = p.historyIndex ?? hist.length - 1;
  if (idx >= hist.length - 1) return false;
  const newIdx = idx + 1;
  const target = hist[newIdx];
  batch(() => {
    setState("panes", paneId, "historyIndex", newIdx);
    setPanePath(paneId, target, { fromHistory: true });
  });
  return true;
}

export function setPaneSelection(paneId: string, selection: string[]) {
  const pane = state.panes[paneId];
  if (!pane) return;
  batch(() => {
    setState("panes", paneId, "selection", selection);
    propagate(pane, "selection", (other) =>
      setState("panes", other.id, "selection", selection),
    );
  });
}

export function setPaneScroll(paneId: string, scrollTop: number, scrollRatio?: number) {
  const pane = state.panes[paneId];
  if (!pane) return;
  setState("panes", paneId, "scrollTop", scrollTop);
  if (typeof scrollRatio === "number" && isFinite(scrollRatio)) {
    setState("panes", paneId, "scrollRatio", scrollRatio);
    propagate(pane, "scroll", (other) =>
      setState("panes", other.id, "scrollRatio", scrollRatio),
    );
  } else {
    propagate(pane, "scroll", (other) =>
      setState("panes", other.id, "scrollTop", scrollTop),
    );
  }
  persist();
}

export function setPaneLinkGroup(paneId: string, groupId: string | null) {
  setState("panes", paneId, "linkGroupId", groupId);
  persist();
}

export function setFocusedPane(paneId: string | null) {
  if (state.focusedPaneId === paneId) return;
  setState("focusedPaneId", paneId);
}

// アクティブタブ内のフォーカス済 leaf を返す。focused が未設定 / 別タブのものなら代表 leaf にフォールバック。
export function focusedLeafPaneId(): string | null {
  const tab = state.tabs.find((t) => t.id === state.activeTabId);
  if (!tab) return null;
  const leafIds: string[] = [];
  collectLeaf(tab.rootPane, leafIds);
  const cur = state.focusedPaneId;
  if (cur && leafIds.includes(cur)) return cur;
  return leafIds[0] ?? null;
}

function collectLeaf(node: PaneNode, out: string[]) {
  if (node.kind === "leaf") out.push(node.paneId);
  else { collectLeaf(node.a, out); collectLeaf(node.b, out); }
}

export function activeLeafPaneId(): string | null {
  const t = state.tabs.find((t) => t.id === state.activeTabId);
  if (!t) return null;
  const walk = (n: PaneNode): string => (n.kind === "leaf" ? n.paneId : walk(n.a));
  return walk(t.rootPane);
}

export function splitPane(tabId: string, paneId: string, dir: "h" | "v") {
  const sourcePane = state.panes[paneId];
  if (!sourcePane) return;
  const newPaneId = nid("pane");
  batch(() => {
    setState("panes", newPaneId, {
      id: newPaneId,
      path: sourcePane.path,
      selection: [],
      scrollTop: 0,
      linkGroupId: null,
      history: [sourcePane.path],
      historyIndex: 0,
    });
    setState("paneUi", newPaneId, defaultPaneUi());
    setState(
      "tabs",
      (t) => t.id === tabId,
      "rootPane",
      (root) =>
        findAndReplace(root, paneId, {
          kind: "split",
          dir,
          ratio: 0.5,
          a: { kind: "leaf", paneId },
          b: { kind: "leaf", paneId: newPaneId },
        }),
    );
  });
  persist();
}

export function closePane(tabId: string, paneId: string) {
  const tab = state.tabs.find((t) => t.id === tabId);
  if (!tab) return;
  const next = removePane(tab.rootPane, paneId);
  if (!next) return;
  batch(() => {
    setState("tabs", (t) => t.id === tabId, "rootPane", next);
    setState("panes", paneId, undefined as never);
    setState("paneUi", paneId, undefined as never);
  });
  persist();
}

export function setSplitRatio(tabId: string, path: number[], ratio: number) {
  setState("tabs", (t) => t.id === tabId, "rootPane", (root) =>
    updateRatio(root, path, ratio),
  );
  persist();
}

export function setPaneView(paneId: string, view: "list" | "tree") {
  setState("panes", paneId, "view", view);
  persist();
}

export function setPaneName(paneId: string, name: string | null) {
  setState("panes", paneId, "name", name && name.trim() ? name.trim() : null);
  persist();
}

// === paneUi 系 ===

export function getPaneUi(paneId: string): PaneUiState {
  return state.paneUi[paneId] ?? defaultPaneUi();
}

export function setPaneSearchOpen(paneId: string, open: boolean) {
  ensurePaneUi(paneId);
  const wasOpen = state.paneUi[paneId]?.searchOpen;
  setState("paneUi", paneId, "searchOpen", open);
  if (wasOpen && !open) {
    setState("paneUi", paneId, "paneFocusTick", (n) => n + 1);
  }
}

export function togglePaneSearch(paneId: string) {
  const cur = getPaneUi(paneId).searchOpen;
  setPaneSearchOpen(paneId, !cur);
}

export function setPaneSort(paneId: string, key: SortKey) {
  ensurePaneUi(paneId);
  const ui = state.paneUi[paneId];
  if (ui.sortKey === key) {
    setState("paneUi", paneId, "sortDir", ui.sortDir === "asc" ? "desc" : "asc");
  } else {
    setState("paneUi", paneId, "sortKey", key);
    setState("paneUi", paneId, "sortDir", "asc");
  }
}

export function focusPaneSearch(paneId: string) {
  ensurePaneUi(paneId);
  batch(() => {
    setState("paneUi", paneId, "searchOpen", true);
    setState("paneUi", paneId, "searchFocusTick", (n) => n + 1);
  });
}

export function setPaneSearchQuery(paneId: string, q: string) {
  ensurePaneUi(paneId);
  setState("paneUi", paneId, "searchQuery", q);
}

export function setPaneSearchOption(
  paneId: string,
  key: "searchCaseSensitive" | "searchRegex",
  value: boolean,
) {
  ensurePaneUi(paneId);
  setState("paneUi", paneId, key, value);
}

export function togglePaneSearchFocused(paneId: string) {
  ensurePaneUi(paneId);
  const ui = state.paneUi[paneId];
  if (ui?.searchOpen) {
    setState("paneUi", paneId, "searchOpen", false);
    setState("paneUi", paneId, "paneFocusTick", (n) => n + 1);
  } else {
    setState("paneUi", paneId, "searchOpen", true);
    setState("paneUi", paneId, "searchFocusTick", (n) => n + 1);
  }
}

// 未使用 import 警告回避
export type { LinkChannel };
