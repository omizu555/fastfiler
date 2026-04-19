import { createStore } from "solid-js/store";
import { batch } from "solid-js";
import type {
  HotkeyAction,
  HotkeyMap,
  LinkChannel,
  LinkGroup,
  PaneNode,
  PaneState,
  PaneUiState,
  Tab,
  WorkspaceState,
} from "./types";
import { defaultHotkeys } from "./hotkeys";
import { defaultPaneUi } from "./types";

const defaultWorkspace = (): WorkspaceState => ({
  layout: "tabsLeft",
  showTree: false,
  tabsWidth: 240,
  treeWidth: 240,
  treeApply: "active",
});

let idSeq = 0;
const nid = (p: string) => `${p}_${++idSeq}`;

const defaultLinkGroups: LinkGroup[] = [
  {
    id: "lg-red",
    name: "Red",
    color: "#e57373",
    channels: { path: true, selection: false, scroll: true, sort: false },
  },
  {
    id: "lg-blue",
    name: "Blue",
    color: "#64b5f6",
    channels: { path: false, selection: true, scroll: false, sort: true },
  },
];

interface AppState {
  tabs: Tab[];
  activeTabId: string;
  panes: Record<string, PaneState>;
  linkGroups: LinkGroup[];
  tabColumns: number;
  showHidden: boolean;
  clipboard: { paths: string[]; op: "copy" | "cut" } | null;
  showThumbnails: boolean;
  showPreview: boolean;
  showPluginPanel: boolean;
  hotkeys: HotkeyMap;
  searchBackend: "builtin" | "everything";
  everythingPort: number;
  everythingScope: boolean;
  paneUi: Record<string, PaneUiState>;
  workspace: WorkspaceState;
}

const STORAGE_KEY = "fastfiler:state:v1";

function loadInitial(): AppState | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const v = JSON.parse(raw) as AppState & { _seq?: number };
    if (v._seq) idSeq = v._seq;
    // 後方互換: 新規追加フィールドのデフォルト
    if (v.showThumbnails === undefined) v.showThumbnails = true;
    if (v.showPreview === undefined) v.showPreview = false;
    if (v.showPluginPanel === undefined) v.showPluginPanel = false;
    if (v.clipboard === undefined) v.clipboard = null;
    if (!v.hotkeys) v.hotkeys = { ...defaultHotkeys };
    else v.hotkeys = { ...defaultHotkeys, ...v.hotkeys };
    if (v.searchBackend === undefined) v.searchBackend = "builtin";
    if (v.everythingPort === undefined) v.everythingPort = 80;
    if (v.everythingScope === undefined) v.everythingScope = true;
    if (!v.paneUi) v.paneUi = {};
    // 既存ペインに対する PaneUi 補完
    for (const pid of Object.keys(v.panes ?? {})) {
      if (!v.paneUi[pid]) v.paneUi[pid] = defaultPaneUi();
    }
    if (!v.workspace) v.workspace = defaultWorkspace();
    else v.workspace = { ...defaultWorkspace(), ...v.workspace };
    return v;
  } catch {
    return null;
  }
}

function freshState(initialPath: string): AppState {
  const paneId = nid("pane");
  const tab: Tab = {
    id: nid("tab"),
    title: initialPath,
    rootPane: { kind: "leaf", paneId },
  };
  const pane: PaneState = {
    id: paneId,
    path: initialPath,
    selection: [],
    scrollTop: 0,
    linkGroupId: null,
  };
  return {
    tabs: [tab],
    activeTabId: tab.id,
    panes: { [paneId]: pane },
    linkGroups: defaultLinkGroups,
    tabColumns: 1,
    showHidden: false,
    clipboard: null,
    showThumbnails: true,
    showPreview: false,
    showPluginPanel: false,
    hotkeys: { ...defaultHotkeys },
    searchBackend: "builtin",
    everythingPort: 80,
    everythingScope: true,
    paneUi: { [paneId]: defaultPaneUi() },
    workspace: defaultWorkspace(),
  };
}

const loaded = loadInitial();
export const [state, setState] = createStore<AppState>(loaded ?? freshState("C:\\"));

// 永続化 (簡易 debounce)
let saveTimer: number | null = null;
export function persist() {
  if (saveTimer != null) window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => {
    try {
      const snapshot = { ...state, _seq: idSeq };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(snapshot));
    } catch {/* ignore */}
  }, 250);
}

export function setInitialPath(path: string) {
  if (!loaded) {
    const id = state.tabs[0].id;
    const paneId = (state.tabs[0].rootPane as { kind: "leaf"; paneId: string }).paneId;
    batch(() => {
      setState("panes", paneId, "path", path);
      setState("tabs", (t) => t.id === id, "title", path);
    });
  }
}

export function setPanePath(paneId: string, path: string) {
  const pane = state.panes[paneId];
  if (!pane) return;
  batch(() => {
    setState("panes", paneId, { path, selection: [], scrollTop: 0 });
    propagate(pane, "path", (other) =>
      setState("panes", other.id, { path, selection: [], scrollTop: 0 }),
    );
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

export function setPaneScroll(paneId: string, scrollTop: number) {
  const pane = state.panes[paneId];
  if (!pane) return;
  setState("panes", paneId, "scrollTop", scrollTop);
  propagate(pane, "scroll", (other) =>
    setState("panes", other.id, "scrollTop", scrollTop),
  );
  persist();
}

export function setPaneLinkGroup(paneId: string, groupId: string | null) {
  setState("panes", paneId, "linkGroupId", groupId);
  persist();
}

function propagate(
  origin: PaneState,
  channel: LinkChannel,
  fn: (other: PaneState) => void,
) {
  if (!origin.linkGroupId) return;
  const grp = state.linkGroups.find((g) => g.id === origin.linkGroupId);
  if (!grp || !grp.channels[channel]) return;
  for (const p of Object.values(state.panes)) {
    if (p.id === origin.id) continue;
    if (p.linkGroupId === origin.linkGroupId) fn(p);
  }
}

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
    });
    setState("paneUi", paneId, defaultPaneUi());
    setState("tabs", (t) => [...t, tab]);
    setState("activeTabId", tab.id);
  });
  persist();
}

export function closeTab(tabId: string) {
  const tab = state.tabs.find((t) => t.id === tabId);
  const tabs = state.tabs.filter((t) => t.id !== tabId);
  if (tabs.length === 0) return;
  // 当該タブに属するペインの ID を集めて UI 状態を破棄
  const ids: string[] = [];
  if (tab) collectPaneIds(tab.rootPane, ids);
  batch(() => {
    setState("tabs", tabs);
    if (state.activeTabId === tabId) setState("activeTabId", tabs[0].id);
    for (const pid of ids) {
      setState("panes", pid, undefined as never);
      setState("paneUi", pid, undefined as never);
    }
  });
  persist();
}

function collectPaneIds(node: PaneNode, out: string[]) {
  if (node.kind === "leaf") out.push(node.paneId);
  else { collectPaneIds(node.a, out); collectPaneIds(node.b, out); }
}

export function setActiveTab(tabId: string) {
  setState("activeTabId", tabId);
  persist();
}

export function setTabColumns(n: number) {
  setState("tabColumns", Math.min(8, Math.max(1, n)));
  persist();
}

export function toggleHidden() {
  setState("showHidden", (v) => !v);
  persist();
}

export function setShowHidden(v: boolean) {
  setState("showHidden", v);
  persist();
}

export function setClipboard(paths: string[], op: "copy" | "cut") {
  setState("clipboard", paths.length ? { paths, op } : null);
}

export function clearClipboard() {
  setState("clipboard", null);
}

export function setShowThumbnails(v: boolean) { setState("showThumbnails", v); persist(); }
export function setShowPreview(v: boolean) { setState("showPreview", v); persist(); }
export function togglePreview() { setState("showPreview", (v) => !v); persist(); }
export function togglePluginPanel() { setState("showPluginPanel", (v) => !v); persist(); }

function findAndReplace(
  node: PaneNode,
  targetPaneId: string,
  replacement: PaneNode,
): PaneNode {
  if (node.kind === "leaf") {
    return node.paneId === targetPaneId ? replacement : node;
  }
  return {
    ...node,
    a: findAndReplace(node.a, targetPaneId, replacement),
    b: findAndReplace(node.b, targetPaneId, replacement),
  };
}

function removePane(node: PaneNode, targetPaneId: string): PaneNode | null {
  if (node.kind === "leaf") {
    return node.paneId === targetPaneId ? null : node;
  }
  const a = removePane(node.a, targetPaneId);
  const b = removePane(node.b, targetPaneId);
  if (!a) return b;
  if (!b) return a;
  return { ...node, a, b };
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

function updateRatio(node: PaneNode, path: number[], ratio: number): PaneNode {
  if (node.kind === "leaf") return node;
  if (path.length === 0) return { ...node, ratio };
  const [head, ...rest] = path;
  if (head === 0) return { ...node, a: updateRatio(node.a, rest, ratio) };
  return { ...node, b: updateRatio(node.b, rest, ratio) };
}

export function updateTabTitle(tabId: string, title: string) {
  setState("tabs", (t) => t.id === tabId, "title", title);
  persist();
}

// ---------- 追加: タブ並べ替え / 連動チャネル / ホットキー ----------

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

export function setLinkGroupChannel(groupId: string, channel: LinkChannel, enabled: boolean) {
  setState(
    "linkGroups",
    (g) => g.id === groupId,
    "channels",
    channel,
    enabled,
  );
  persist();
}

export function setHotkey(action: HotkeyAction, combo: string) {
  setState("hotkeys", action, combo);
  persist();
}

export function resetHotkeys() {
  setState("hotkeys", { ...defaultHotkeys });
  persist();
}

// ---------- v1.1: ペイン表示モード / 検索バックエンド / フォルダの D&D 並べ替え ----------

export function setPaneView(paneId: string, view: "list" | "tree") {
  setState("panes", paneId, "view", view);
  persist();
}

export function setSearchBackend(b: "builtin" | "everything") {
  setState("searchBackend", b);
  persist();
}
export function setEverythingPort(p: number) {
  setState("everythingPort", Math.max(1, Math.min(65535, Math.floor(p))));
  persist();
}
export function setEverythingScope(v: boolean) {
  setState("everythingScope", v);
  persist();
}

// ---------- v1.2: タブ移動 / ペイン UI 状態 ----------

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

function ensurePaneUi(paneId: string): PaneUiState {
  let ui = state.paneUi[paneId];
  if (!ui) {
    setState("paneUi", paneId, defaultPaneUi());
    ui = state.paneUi[paneId];
  }
  return ui;
}

export function getPaneUi(paneId: string): PaneUiState {
  return state.paneUi[paneId] ?? defaultPaneUi();
}

export function setPaneSearchOpen(paneId: string, open: boolean) {
  ensurePaneUi(paneId);
  setState("paneUi", paneId, "searchOpen", open);
}

export function togglePaneSearch(paneId: string) {
  const cur = getPaneUi(paneId).searchOpen;
  setPaneSearchOpen(paneId, !cur);
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

// === Workspace layout ===
export function setWorkspaceLayout(layout: WorkspaceState["layout"]) {
  setState("workspace", "layout", layout);
  persist();
}

export function cycleWorkspaceLayout() {
  const order: WorkspaceState["layout"][] = ["tabsLeft", "tabsRight", "tabsHidden"];
  const cur = state.workspace.layout;
  const idx = order.indexOf(cur);
  setWorkspaceLayout(order[(idx + 1) % order.length]);
}

export function toggleWorkspaceTabs() {
  const cur = state.workspace.layout;
  setWorkspaceLayout(cur === "tabsHidden" ? "tabsLeft" : "tabsHidden");
}

export function toggleWorkspaceTree() {
  setState("workspace", "showTree", (v) => !v);
  persist();
}

export function setWorkspaceTabsWidth(w: number) {
  setState("workspace", "tabsWidth", Math.max(140, Math.min(600, Math.round(w))));
  persist();
}

export function setWorkspaceTreeWidth(w: number) {
  setState("workspace", "treeWidth", Math.max(140, Math.min(600, Math.round(w))));
  persist();
}

export function setWorkspaceTreeApply(a: WorkspaceState["treeApply"]) {
  setState("workspace", "treeApply", a);
  persist();
}
