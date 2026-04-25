import { batch } from "solid-js";
import type { Tab } from "../types";
import { defaultPaneUi } from "../types";
import { state, setState, persist, nid, collectPaneIds } from "./core";

export function addTab(path = "C:\\") {
  const paneId = nid("pane");
  const tab: Tab = {
    id: nid("tab"),
    title: path,
    rootPane: { kind: "leaf", paneId },
  };
  batch(() => {
    setState("panes", paneId, {
      id: paneId,
      path,
      selection: [],
      scrollTop: 0,
      linkGroupId: null,
      history: [path],
      historyIndex: 0,
    });
    setState("paneUi", paneId, defaultPaneUi());
    setState("tabs", (t) => [...t, tab]);
    setState("activeTabId", tab.id);
  });
  persist();
}

/**
 * v1.12: 同一パスのタブが既にあれば そちらをアクティブ化、なければ新規追加。
 * シェル統合 (Excel リンクなど外部からフォルダを開く場合) で重複防止。
 */
export function addOrFocusTab(path: string) {
  const norm = path.replace(/[/\\]+$/g, "").toLowerCase();
  for (const tab of state.tabs) {
    const ids: string[] = [];
    collectPaneIds(tab.rootPane, ids);
    for (const pid of ids) {
      const p = state.panes[pid]?.path?.replace(/[/\\]+$/g, "").toLowerCase();
      if (p === norm) {
        batch(() => {
          setState("activeTabId", tab.id);
          setState("focusedPaneId", pid);
        });
        persist();
        return;
      }
    }
  }
  addTab(path);
}

export function closeTab(tabId: string) {
  const tab = state.tabs.find((t) => t.id === tabId);
  if (tab?.locked) return; // ロック中は全経路で閉じない
  const tabs = state.tabs.filter((t) => t.id !== tabId);
  if (tabs.length === 0) return;
  const ids: string[] = [];
  if (tab) collectPaneIds(tab.rootPane, ids);
  batch(() => {
    setState("tabs", tabs);
    if (state.activeTabId === tabId) setState("activeTabId", tabs[0].id);
    for (const pid of ids) {
      setState("panes", pid, undefined as never);
      setState("paneUi", pid, undefined as never);
    }
    if (state.focusedPaneId && ids.includes(state.focusedPaneId)) {
      const newActive = state.activeTabId === tabId ? tabs[0] : state.tabs.find((t) => t.id === state.activeTabId);
      if (newActive) {
        const leafIds: string[] = [];
        collectPaneIds(newActive.rootPane, leafIds);
        setState("focusedPaneId", leafIds[0] ?? null);
      } else {
        setState("focusedPaneId", null);
      }
    }
  });
  persist();
}

export function setActiveTab(tabId: string) {
  setState("activeTabId", tabId);
  const tab = state.tabs.find((t) => t.id === tabId);
  if (tab) {
    const leafIds: string[] = [];
    collectPaneIds(tab.rootPane, leafIds);
    const cur = state.focusedPaneId;
    if (!cur || !leafIds.includes(cur)) {
      setState("focusedPaneId", leafIds[0] ?? null);
    }
  }
  persist();
}

export function setTabColumns(n: number) {
  setState("tabColumns", Math.min(8, Math.max(1, n)));
  persist();
}

export function updateTabTitle(tabId: string, title: string) {
  setState("tabs", (t) => t.id === tabId, "title", title);
  persist();
}

export function reorderTab(fromId: string, toIndex: number) {
  const list = [...state.tabs];
  const fromIdx = list.findIndex((t) => t.id === fromId);
  if (fromIdx < 0) return;
  const [moved] = list.splice(fromIdx, 1);
  const clamped = Math.max(0, Math.min(list.length, toIndex));
  list.splice(clamped, 0, moved);
  setState("tabs", list);
  persist();
}

export function cycleTab(delta: number) {
  if (state.tabs.length <= 1) return;
  const idx = state.tabs.findIndex((t) => t.id === state.activeTabId);
  if (idx < 0) return;
  const len = state.tabs.length;
  const next = ((idx + delta) % len + len) % len;
  setState("activeTabId", state.tabs[next].id);
  persist();
}

export function setActiveTabIndex(index: number) {
  if (index < 0 || index >= state.tabs.length) return;
  setState("activeTabId", state.tabs[index].id);
  persist();
}

export function toggleTabLock(tabId: string) {
  const tab = state.tabs.find((t) => t.id === tabId);
  if (!tab) return;
  setState("tabs", (t) => t.id === tabId, "locked", !tab.locked);
  persist();
}

/** 指定 paneId を含むタブを返す */
export function findTabOfPane(paneId: string) {
  for (const tab of state.tabs) {
    const ids: string[] = [];
    collectPaneIds(tab.rootPane, ids);
    if (ids.includes(paneId)) return tab;
  }
  return null;
}

/** 指定タブがロック中か */
export function isTabLocked(tabId: string): boolean {
  return !!state.tabs.find((t) => t.id === tabId)?.locked;
}

/** 指定 paneId を含むタブがロック中か */
export function isPaneLocked(paneId: string): boolean {
  return !!findTabOfPane(paneId)?.locked;
}
