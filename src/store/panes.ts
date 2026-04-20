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

export function setPanePath(paneId: string, path: string) {
  const pane = state.panes[paneId];
  if (!pane) return;
  batch(() => {
    setState("panes", paneId, { path, selection: [], scrollTop: 0 });
    propagate(pane, "path", (other) =>
      setState("panes", other.id, { path, selection: [], scrollTop: 0 }),
    );
    setState("focusedPaneId", paneId);
  });
  persist();
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
