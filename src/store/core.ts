// store コア: AppState 型、createStore、永続化、内部ヘルパー
// 各機能別ファイル (tabs/panes/dock/...) はここから state/setState/persist を import する。
import { createStore } from "solid-js/store";
import type {
  HotkeyMap,
  LinkChannel,
  LinkGroup,
  PaneNode,
  PaneState,
  PaneUiState,
  PluginContextMenuItem,
  Tab,
  Toast,
  UndoEntry,
  WorkspaceState,
} from "../types";
import { defaultHotkeys } from "../hotkeys";
import { defaultPaneUi } from "../types";

export const defaultWorkspace = (): WorkspaceState => ({
  layout: "tabsLeft",
  showTree: false,
  tabsWidth: 240,
  treeWidth: 240,
  treeApply: "active",
  panelDock: defaultPanelDock(),
  samePanelStack: false,
});

export function defaultPanelDock(): import("../types").PanelDockState {
  return {
    tabs: { slot: "left", order: 0, size: 240, lastDockSlot: "left" },
    tree: { slot: "left", order: 1, size: 240, lastDockSlot: "left" },
  };
}

let idSeq = 0;
export const nid = (p: string) => `${p}_${++idSeq}`;

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

export interface AppState {
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
  hidePaneToolbar: boolean;
  hotkeys: HotkeyMap;
  searchBackend: "builtin" | "everything";
  everythingPort: number;
  everythingScope: boolean;
  paneUi: Record<string, PaneUiState>;
  workspace: WorkspaceState;
  theme: import("../types").ThemeMode;
  accentColor: string | null;
  iconSet: import("../types").IconSet;
  plugins: { enabled: Record<string, boolean> };
  pluginPanelWidth: number;
  pluginContextMenu: PluginContextMenuItem[];
  toasts: Toast[];
  undoStack: UndoEntry[];
  activeJobs: import("../types").FileJob[];
  presets: import("../types").WorkspacePreset[];
  showTerminal: boolean;
  terminalHeight: number;
  terminalShell: string | null;
  terminalFont: string | null;
  terminalFontSize: number;
  uiFont: string | null;
  uiFontSize: number;
  focusedPaneId: string | null;
}

const STORAGE_KEY = "fastfiler:state:v1";

function loadInitial(): AppState | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const v = JSON.parse(raw) as AppState & { _seq?: number };
    if (v._seq) idSeq = v._seq;
    if (v.showThumbnails === undefined) v.showThumbnails = true;
    if (v.showPreview === undefined) v.showPreview = false;
    if (v.showPluginPanel === undefined) v.showPluginPanel = false;
    if (v.hidePaneToolbar === undefined) v.hidePaneToolbar = false;
    if (v.clipboard === undefined) v.clipboard = null;
    if (!v.hotkeys) v.hotkeys = { ...defaultHotkeys };
    else v.hotkeys = { ...defaultHotkeys, ...v.hotkeys };
    if (v.searchBackend === undefined) v.searchBackend = "builtin";
    if (v.everythingPort === undefined) v.everythingPort = 80;
    if (v.everythingScope === undefined) v.everythingScope = true;
    if (v.pluginPanelWidth === undefined) v.pluginPanelWidth = 320;
    if (!v.paneUi) v.paneUi = {};
    for (const pid of Object.keys(v.panes ?? {})) {
      if (!v.paneUi[pid]) v.paneUi[pid] = defaultPaneUi();
      else v.paneUi[pid] = { ...defaultPaneUi(), ...v.paneUi[pid] };
    }
    if (!v.workspace) v.workspace = defaultWorkspace();
    else {
      v.workspace = { ...defaultWorkspace(), ...v.workspace };
      if (!v.workspace.panelDock) {
        const pd = defaultPanelDock();
        const layout = v.workspace.layout;
        const tabsSlot = layout === "tabsHidden" ? "hidden" : (layout === "tabsRight" ? "right" : "left");
        pd.tabs = { slot: tabsSlot, order: 0, size: v.workspace.tabsWidth ?? 240, lastDockSlot: tabsSlot === "hidden" ? "left" : tabsSlot };
        pd.tree = { slot: v.workspace.showTree ? "left" : "hidden", order: 1, size: v.workspace.treeWidth ?? 240, lastDockSlot: "left" };
        v.workspace.panelDock = pd;
      }
    }
    if (!v.theme) v.theme = "system";
    if (v.accentColor === undefined) v.accentColor = null;
    if (!v.iconSet) v.iconSet = "emoji";
    if (!Array.isArray(v.presets)) v.presets = [];
    if (v.showTerminal === undefined) v.showTerminal = false;
    if (typeof v.terminalHeight !== "number") v.terminalHeight = 240;
    if (v.terminalShell === undefined) v.terminalShell = null;
    if (v.terminalFont === undefined) v.terminalFont = null;
    if (typeof v.terminalFontSize !== "number") v.terminalFontSize = 13;
    if (v.uiFont === undefined) v.uiFont = null;
    if (typeof v.uiFontSize !== "number") v.uiFontSize = 13;
    if (!v.plugins) v.plugins = { enabled: {} };
    if (!v.plugins.enabled) v.plugins.enabled = {};
    v.pluginContextMenu = [];
    v.toasts = [];
    v.undoStack = [];
    if (v.focusedPaneId === undefined) v.focusedPaneId = null;
    if (!v.toasts) v.toasts = [];
    if (!v.undoStack) v.undoStack = [];
    v.activeJobs = [];
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
    history: [initialPath],
    historyIndex: 0,
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
    hidePaneToolbar: false,
    hotkeys: { ...defaultHotkeys },
    searchBackend: "builtin",
    everythingPort: 80,
    everythingScope: true,
    paneUi: { [paneId]: defaultPaneUi() },
    workspace: defaultWorkspace(),
    theme: "system",
    accentColor: null,
    iconSet: "emoji",
    plugins: { enabled: {} },
    pluginPanelWidth: 320,
    pluginContextMenu: [],
    toasts: [],
    undoStack: [],
    activeJobs: [],
    presets: [],
    showTerminal: false,
    terminalHeight: 240,
    terminalShell: null,
    terminalFont: null,
    terminalFontSize: 13,
    uiFont: null,
    uiFontSize: 13,
    focusedPaneId: paneId,
  };
}

export const loaded = loadInitial();
export const [state, setState] = createStore<AppState>(loaded ?? freshState("C:\\"));

let saveTimer: number | null = null;
export function persist() {
  if (saveTimer != null) window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => {
    try {
      const snapshot = { ...state, _seq: idSeq };
      delete (snapshot as Record<string, unknown>).pluginContextMenu;
      delete (snapshot as Record<string, unknown>).toasts;
      delete (snapshot as Record<string, unknown>).undoStack;
      delete (snapshot as Record<string, unknown>).activeJobs;
      localStorage.setItem(STORAGE_KEY, JSON.stringify(snapshot));
    } catch {/* ignore */}
  }, 250);
}

if (loaded) persist();

if (typeof window !== "undefined") (window as unknown as { __ff: unknown }).__ff = { state, persist };

// === 内部ヘルパー (各機能ファイル間で共有) ===

export function propagate(
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

export function collectPaneIds(node: PaneNode, out: string[]) {
  if (node.kind === "leaf") out.push(node.paneId);
  else { collectPaneIds(node.a, out); collectPaneIds(node.b, out); }
}

export function findAndReplace(
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

export function removePane(node: PaneNode, targetPaneId: string): PaneNode | null {
  if (node.kind === "leaf") {
    return node.paneId === targetPaneId ? null : node;
  }
  const a = removePane(node.a, targetPaneId);
  const b = removePane(node.b, targetPaneId);
  if (!a) return b;
  if (!b) return a;
  return { ...node, a, b };
}

export function updateRatio(node: PaneNode, path: number[], ratio: number): PaneNode {
  if (node.kind === "leaf") return node;
  if (path.length === 0) return { ...node, ratio };
  const [head, ...rest] = path;
  if (head === 0) return { ...node, a: updateRatio(node.a, rest, ratio) };
  return { ...node, b: updateRatio(node.b, rest, ratio) };
}

export function ensurePaneUi(paneId: string): PaneUiState {
  let ui = state.paneUi[paneId];
  if (!ui) {
    setState("paneUi", paneId, defaultPaneUi());
    ui = state.paneUi[paneId];
  }
  return ui;
}

export function ensureDock() {
  if (!state.workspace.panelDock) {
    setState("workspace", "panelDock", defaultPanelDock());
  }
}
