import { Show, createMemo, createSignal, onMount, For } from "solid-js";
import VerticalTabs from "./components/VerticalTabs";
import PaneTree from "./components/PaneTree";
import SettingsDialog from "./components/SettingsDialog";
import PreviewPane from "./components/PreviewPane";
import PluginPanel from "./components/PluginPanel";
import ToastContainer from "./components/ToastContainer";
import JobsPanel from "./components/JobsPanel";
import WorkspaceTreePanel from "./components/WorkspaceTreePanel";
import PromptDialog from "./components/PromptDialog";
import {
  state,
  setInitialPath,
  togglePreview,
  togglePluginPanel,
  addTab,
  closeTab,
  cycleTab,
  setActiveTabIndex,
  cycleWorkspaceLayout,
  toggleWorkspaceTree,
  panelsInSlot,
  setPanelSize,
} from "./store";
import { homeDir } from "./fs";
import { matchKey } from "./hotkeys";
import { performUndo } from "./undo";
import type { DockSlot, PanelId } from "./types";

function PanelById(props: { id: PanelId }) {
  return (
    <Show when={props.id === "tabs"} fallback={<WorkspaceTreePanel />}>
      <VerticalTabs />
    </Show>
  );
}

function DockArea(props: { slot: DockSlot }) {
  const ids = createMemo(() => panelsInSlot(props.slot));
  const horizontal = () => props.slot === "top" || props.slot === "bottom";
  const stack = createMemo(() => !!state.workspace.samePanelStack && ids().length > 1);
  // 並列: SUM, スタック: MAX
  const totalSize = createMemo(() => {
    const pd = state.workspace.panelDock;
    if (!pd) return 240;
    let v = 0;
    if (stack()) {
      for (const id of ids()) v = Math.max(v, pd[id].size);
    } else {
      for (const id of ids()) v += pd[id].size;
    }
    return v || 240;
  });
  return (
    <Show when={ids().length > 0}>
      <div
        class={`dock-area dock-${props.slot}`}
        classList={{ "stack-mode": stack() }}
        style={horizontal()
          ? { height: `${totalSize()}px`, width: "100%" }
          : { width: `${totalSize()}px`, height: "100%" }}
      >
        <For each={ids()}>{(id) => <PanelById id={id} />}</For>
      </div>
    </Show>
  );
}

export default function App() {
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const activeTab = createMemo(() =>
    state.tabs.find((t) => t.id === state.activeTabId),
  );

  // 「現在フォーカスされているペイン」推定: 最後にアクティブだった leaf を辿る
  const previewPaneId = createMemo<string | null>(() => {
    const t = activeTab();
    if (!t) return null;
    const find = (n: typeof t.rootPane): string | null => {
      if (n.kind === "leaf") return n.paneId;
      return find(n.a) ?? find(n.b);
    };
    return find(t.rootPane);
  });

  onMount(async () => {
    try {
      const home = await homeDir();
      setInitialPath(home);
    } catch {/* ignore */}
    const onKey = (e: KeyboardEvent) => {
      const hk = state.hotkeys;
      if (matchKey(hk["open-settings"], e)) {
        e.preventDefault();
        setSettingsOpen(true);
      } else if (matchKey(hk["toggle-preview"], e)) {
        e.preventDefault();
        togglePreview();
      } else if (matchKey(hk["toggle-plugin"], e)) {
        e.preventDefault();
        togglePluginPanel();
      } else if (matchKey(hk["toggle-tabs"], e)) {
        e.preventDefault();
        cycleWorkspaceLayout();
      } else if (matchKey(hk["toggle-tree"], e)) {
        e.preventDefault();
        toggleWorkspaceTree();
      } else if (matchKey(hk["new-tab"], e)) {
        e.preventDefault();
        homeDir().then((h) => addTab(h)).catch(() => addTab("C:\\"));
      } else if (matchKey(hk["close-tab"], e)) {
        e.preventDefault();
        if (state.tabs.length > 1) closeTab(state.activeTabId);
      } else if (matchKey(hk["next-tab"], e) || (e.ctrlKey && !e.shiftKey && !e.altKey && e.key === "PageDown")) {
        e.preventDefault();
        cycleTab(1);
      } else if (matchKey(hk["prev-tab"], e) || (e.ctrlKey && !e.shiftKey && !e.altKey && e.key === "PageUp")) {
        e.preventDefault();
        cycleTab(-1);
      } else if (matchKey(hk["undo"], e)) {
        const tgt = e.target as HTMLElement | null;
        if (tgt && (tgt.tagName === "INPUT" || tgt.tagName === "TEXTAREA" || tgt.isContentEditable)) return;
        e.preventDefault();
        void performUndo();
      } else if (e.ctrlKey && !e.shiftKey && !e.altKey && !e.metaKey && /^[1-9]$/.test(e.key)) {
        // Ctrl+1..8 → 指定インデックス、Ctrl+9 → 最後のタブ
        e.preventDefault();
        const n = parseInt(e.key, 10);
        if (n === 9) setActiveTabIndex(state.tabs.length - 1);
        else setActiveTabIndex(n - 1);
      }
    };
    window.addEventListener("keydown", onKey);
    // ブラウザ既定の右クリックメニューを抑止 (テキスト入力要素では許可)
    const onCtx = (e: MouseEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
      e.preventDefault();
    };
    window.addEventListener("contextmenu", onCtx);
  });

  const showTabs = createMemo(() => state.workspace.panelDock?.tabs.slot !== "hidden");

  return (
    <div class="app">
      <header class="app-header">
        <span class="logo">⚡ FastFiler</span>
        <span class="muted">v0.1.0</span>
        <span class="spacer" />
        <span class="muted">Ctrl+F:検索 ／ Ctrl+P:プレビュー ／ Ctrl+B:タブサイドバー ／ Ctrl+Shift+E:ツリー ／ Ctrl+,:設定</span>
        <button class="header-btn" classList={{ active: showTabs() }}
          title="タブパネルを次の dock へ (Ctrl+B)" onClick={cycleWorkspaceLayout}>📑</button>
        <button class="header-btn" classList={{ active: state.workspace.panelDock?.tree.slot !== "hidden" }}
          title="ツリーパネル切替 (Ctrl+Shift+E)" onClick={toggleWorkspaceTree}>🌲</button>
        <button class="header-btn" classList={{ active: state.showPreview }}
          title="プレビュー (Ctrl+P)" onClick={togglePreview}>👁</button>
        <button class="header-btn" classList={{ active: state.showPluginPanel }}
          title="プラグイン (Ctrl+Shift+P)" onClick={togglePluginPanel}>🧩</button>
        <button class="header-btn" title="設定 (Ctrl+,)" onClick={() => setSettingsOpen(true)}>⚙ 設定</button>
      </header>
      <div class="app-body dock-grid">
        <DockArea slot="top" />
        <div class="dock-middle">
          <DockArea slot="left" />
          <main class="workspace">
            <Show when={activeTab()} fallback={<div class="empty">タブなし</div>}>
              <PaneTree node={activeTab()!.rootPane} tabId={activeTab()!.id} />
            </Show>
          </main>
          <Show when={state.showPreview && previewPaneId()}>
            {(pid) => <PreviewPane paneId={pid()} />}
          </Show>
          <Show when={state.showPluginPanel}>
            <PluginPanel />
          </Show>
          <DockArea slot="right" />
        </div>
        <DockArea slot="bottom" />
      </div>
      <SettingsDialog open={settingsOpen()} onClose={() => setSettingsOpen(false)} />
      <PromptDialog />
      <ToastContainer />
      <JobsPanel />
    </div>
  );
}