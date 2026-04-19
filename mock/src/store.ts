import { createStore } from "solid-js/store";
import { batch } from "solid-js";
import type {
  LinkChannel,
  LinkGroup,
  PaneNode,
  PaneState,
  Tab,
} from "./types";

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

const initialPaneA: PaneState = {
  id: nid("pane"),
  path: "C:\\temp\\Files",
  selection: [],
  scrollTop: 0,
  linkGroupId: null,
};
const initialPaneB: PaneState = {
  id: nid("pane"),
  path: "C:\\Users\\o_miz",
  selection: [],
  scrollTop: 0,
  linkGroupId: null,
};

const initialTab: Tab = {
  id: nid("tab"),
  title: "Files / Home",
  rootPane: {
    kind: "split",
    dir: "h",
    ratio: 0.5,
    a: { kind: "leaf", paneId: initialPaneA.id },
    b: { kind: "leaf", paneId: initialPaneB.id },
  },
};

const secondTabPaneId = nid("pane");
const secondTab: Tab = {
  id: nid("tab"),
  title: "C:\\",
  rootPane: { kind: "leaf", paneId: secondTabPaneId },
};
const secondTabPane: PaneState = {
  id: secondTabPaneId,
  path: "C:\\",
  selection: [],
  scrollTop: 0,
  linkGroupId: null,
};

interface AppState {
  tabs: Tab[];
  activeTabId: string;
  panes: Record<string, PaneState>;
  linkGroups: LinkGroup[];
  tabColumns: number;
}

export const [state, setState] = createStore<AppState>({
  tabs: [initialTab, secondTab],
  activeTabId: initialTab.id,
  panes: {
    [initialPaneA.id]: initialPaneA,
    [initialPaneB.id]: initialPaneB,
    [secondTabPane.id]: secondTabPane,
  },
  linkGroups: defaultLinkGroups,
  tabColumns: 1,
});

export function setPanePath(paneId: string, path: string) {
  const pane = state.panes[paneId];
  if (!pane) return;
  batch(() => {
    setState("panes", paneId, { path, selection: [], scrollTop: 0 });
    propagate(pane, "path", (other) =>
      setState("panes", other.id, { path, selection: [], scrollTop: 0 }),
    );
  });
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
}

export function setPaneLinkGroup(paneId: string, groupId: string | null) {
  setState("panes", paneId, "linkGroupId", groupId);
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

export function addTab() {
  const paneId = nid("pane");
  const tab: Tab = {
    id: nid("tab"),
    title: "新規タブ",
    rootPane: { kind: "leaf", paneId },
  };
  batch(() => {
    setState("panes", paneId, {
      id: paneId,
      path: "C:\\",
      selection: [],
      scrollTop: 0,
      linkGroupId: null,
    });
    setState("tabs", (t) => [...t, tab]);
    setState("activeTabId", tab.id);
  });
}

export function closeTab(tabId: string) {
  const tabs = state.tabs.filter((t) => t.id !== tabId);
  if (tabs.length === 0) return;
  batch(() => {
    setState("tabs", tabs);
    if (state.activeTabId === tabId) setState("activeTabId", tabs[0].id);
  });
}

export function setActiveTab(tabId: string) {
  setState("activeTabId", tabId);
}

export function setTabColumns(n: number) {
  setState("tabColumns", Math.min(4, Math.max(1, n)));
}

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
}

export function closePane(tabId: string, paneId: string) {
  const tab = state.tabs.find((t) => t.id === tabId);
  if (!tab) return;
  const next = removePane(tab.rootPane, paneId);
  if (!next) return;
  batch(() => {
    setState("tabs", (t) => t.id === tabId, "rootPane", next);
    setState("panes", paneId, undefined as never);
  });
}

export function setSplitRatio(tabId: string, path: number[], ratio: number) {
  setState("tabs", (t) => t.id === tabId, "rootPane", (root) =>
    updateRatio(root, path, ratio),
  );
}

function updateRatio(node: PaneNode, path: number[], ratio: number): PaneNode {
  if (node.kind === "leaf") return node;
  if (path.length === 0) return { ...node, ratio };
  const [head, ...rest] = path;
  if (head === 0) return { ...node, a: updateRatio(node.a, rest, ratio) };
  return { ...node, b: updateRatio(node.b, rest, ratio) };
}
