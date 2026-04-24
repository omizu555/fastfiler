import {
  Show,
  createEffect,
  createMemo,
  createSignal,
  onMount,
  onCleanup,
  For,
} from "solid-js";
import VerticalTabs from "./components/VerticalTabs";
import PaneTree from "./components/PaneTree";
import SettingsDialog from "./components/SettingsDialog";
import PreviewPane from "./components/PreviewPane";
import PluginPanel from "./components/PluginPanel";
import TerminalPanel from "./components/TerminalPanel";
import StatusBarToast from "./components/StatusBarToast";
import StatusBarJobs from "./components/StatusBarJobs";
import RightDragOverlay from "./components/RightDragOverlay";
import { ensureRightDragInstalled } from "./file-list/right-drag";
import WorkspaceTreePanel from "./components/WorkspaceTreePanel";
import PromptDialog from "./components/PromptDialog";
import {
  state,
  setInitialPath,
  togglePreview,
  togglePluginPanel,
  toggleTerminal,
  addTab,
  closeTab,
  cycleTab,
  setActiveTabIndex,
  cycleWorkspaceLayout,
  toggleWorkspaceTree,
  panelsInSlot,
  focusedLeafPaneId,
  navigateBack,
  navigateForward,
  flushPersistImmediate,
} from "./store";
import { homeDir } from "./fs";
import {
  installInternalPointerDnd,
  installExternalDropListeners,
} from "./dnd";
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
  const stack = createMemo(
    () => !!state.workspace.samePanelStack && ids().length > 1,
  );
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
        style={
          horizontal()
            ? { height: `${totalSize()}px`, width: "100%" }
            : { width: `${totalSize()}px`, height: "100%" }
        }
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

  // アンマウント時に全イベントリスナーを解除
  const unlistens: Array<() => void> = [];
  onCleanup(() => unlistens.forEach((fn) => fn()));

  onMount(async () => {
    ensureRightDragInstalled();
    // アプリ内 D&D (pointer 自前) と外部 D&D (OLE / WebView2) を install
    unlistens.push(installInternalPointerDnd());
    try {
      unlistens.push(await installExternalDropListeners());
    } catch (err) {
      console.warn("[app] external dnd init failed", err);
    }
    try {
      const home = await homeDir();
      setInitialPath(home);
    } catch {
      /* ignore */
    }
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
      } else if (matchKey(hk["toggle-terminal"], e)) {
        e.preventDefault();
        toggleTerminal();
      } else if (matchKey(hk["toggle-tabs"], e)) {
        e.preventDefault();
        cycleWorkspaceLayout();
      } else if (matchKey(hk["toggle-tree"], e)) {
        e.preventDefault();
        toggleWorkspaceTree();
      } else if (matchKey(hk["new-tab"], e)) {
        e.preventDefault();
        homeDir()
          .then((h) => addTab(h))
          .catch(() => addTab("C:\\"));
      } else if (matchKey(hk["close-tab"], e)) {
        e.preventDefault();
        if (state.tabs.length > 1) closeTab(state.activeTabId);
      } else if (
        matchKey(hk["next-tab"], e) ||
        (e.ctrlKey && !e.shiftKey && !e.altKey && e.key === "PageDown")
      ) {
        e.preventDefault();
        cycleTab(1);
      } else if (
        matchKey(hk["prev-tab"], e) ||
        (e.ctrlKey && !e.shiftKey && !e.altKey && e.key === "PageUp")
      ) {
        e.preventDefault();
        cycleTab(-1);
      } else if (matchKey(hk["undo"], e)) {
        const tgt = e.target as HTMLElement | null;
        if (
          tgt &&
          (tgt.tagName === "INPUT" ||
            tgt.tagName === "TEXTAREA" ||
            tgt.isContentEditable)
        )
          return;
        e.preventDefault();
        void performUndo();
      } else if (
        e.ctrlKey &&
        !e.shiftKey &&
        !e.altKey &&
        !e.metaKey &&
        /^[1-9]$/.test(e.key)
      ) {
        // Ctrl+1..8 → 指定インデックス、Ctrl+9 → 最後のタブ
        e.preventDefault();
        const n = parseInt(e.key, 10);
        if (n === 9) setActiveTabIndex(state.tabs.length - 1);
        else setActiveTabIndex(n - 1);
      } else if (matchKey(hk["pane-back"], e)) {
        e.preventDefault();
        const pid = focusedLeafPaneId();
        if (pid) navigateBack(pid);
      } else if (matchKey(hk["pane-forward"], e)) {
        e.preventDefault();
        const pid = focusedLeafPaneId();
        if (pid) navigateForward(pid);
      }
    };
    window.addEventListener("keydown", onKey);
    // ブラウザ既定の右クリックメニューを抑止 (テキスト入力要素では許可)
    const onCtx = (e: MouseEvent) => {
      const t = e.target as HTMLElement | null;
      if (
        t &&
        (t.tagName === "INPUT" ||
          t.tagName === "TEXTAREA" ||
          t.isContentEditable)
      )
        return;
      e.preventDefault();
    };
    window.addEventListener("contextmenu", onCtx);
    // v1.6 (16.4): マウスのサイドボタン (XButton1=戻る / XButton2=進む)
    const onMouseDown = (e: MouseEvent) => {
      // 入力要素上でのサイドボタンは無視 (誤動作防止)
      const t = e.target as HTMLElement | null;
      if (
        t &&
        (t.tagName === "INPUT" ||
          t.tagName === "TEXTAREA" ||
          t.isContentEditable)
      )
        return;
      // Mouse 4 (XButton1) = 戻る, Mouse 5 (XButton2) = 進む
      if (e.button === 3) {
        e.preventDefault();
        const pid = focusedLeafPaneId();
        if (pid) navigateBack(pid);
      } else if (e.button === 4) {
        e.preventDefault();
        const pid = focusedLeafPaneId();
        if (pid) navigateForward(pid);
      }
    };
    window.addEventListener("mousedown", onMouseDown);
    // ブラウザ既定の戻る/進むナビゲーションを auxclick でも抑止
    const onAuxClick = (e: MouseEvent) => {
      if (e.button === 3 || e.button === 4) e.preventDefault();
    };
    window.addEventListener("auxclick", onAuxClick);
    // v1.9: × ボタン押下時に flush してから明示的に destroy() で閉じる。
    //       Tauri 2 では onCloseRequested ハンドラ登録時点で close が JS に委ねられる
    //       挙動があるため、preventDefault → flush → destroy() の安全パターンを使う。
    let unlistenClose: (() => void) | null = null;
    (async () => {
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        const w = getCurrentWindow();
        const un = await w.onCloseRequested(async (event) => {
          event.preventDefault();
          try { flushPersistImmediate(); } catch {/* ignore */}
          try { await w.destroy(); } catch {/* ignore */}
        });
        unlistenClose = un;
      } catch {/* tauri 外環境では無視 */}
    })();
    unlistens.push(() => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("contextmenu", onCtx);
      window.removeEventListener("mousedown", onMouseDown);
      window.removeEventListener("auxclick", onAuxClick);
      if (unlistenClose) { try { unlistenClose(); } catch {/* ignore */} }
    });
  });

  // UI フォント設定を CSS 変数に反映
  createEffect(() => {
    const root = document.documentElement;
    const f = state.uiFont;
    if (f && f.trim()) {
      root.style.setProperty(
        "--ui-font",
        `"${f}", "Yu Gothic UI", "Segoe UI", sans-serif`,
      );
    } else {
      root.style.removeProperty("--ui-font");
    }
    root.style.setProperty("--ui-font-size", `${state.uiFontSize}px`);
  });

  return (
    <div class="app" classList={{ "hide-pane-toolbar": state.hidePaneToolbar }}>
      <div class="app-body dock-grid">
        <DockArea slot="top" />
        <div class="dock-middle">
          <DockArea slot="left" />
          <main class="workspace">
            <Show
              when={activeTab()}
              fallback={<div class="empty">タブなし</div>}
            >
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
      <TerminalPanel />
      <footer class="app-statusbar">
        <span class="muted statusbar-logo">⚡ FastFiler</span>
        <StatusBarToast />
        <StatusBarJobs />
        <span class="spacer" />
        <button
          class="statusbar-btn"
          classList={{ active: state.showTerminal }}
          title="ターミナル (Ctrl+`)"
          onClick={toggleTerminal}
        >
          ⌨
        </button>
        <button
          class="statusbar-btn"
          title="設定 (Ctrl+,)"
          onClick={() => setSettingsOpen(true)}
        >
          ⚙
        </button>
      </footer>
      <SettingsDialog
        open={settingsOpen()}
        onClose={() => setSettingsOpen(false)}
      />
      <PromptDialog />
      <RightDragOverlay />
    </div>
  );
}
