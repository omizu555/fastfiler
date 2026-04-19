import { Show, createMemo, For } from "solid-js";
import VerticalTabs from "./components/VerticalTabs";
import WorkspaceTreePanel from "./components/WorkspaceTreePanel";
import DockOverlay from "./components/DockOverlay";
import ToastContainer from "./components/ToastContainer";
import { state, panelsInSlot, WINDOW_ID } from "./store";
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
  const totalSize = createMemo(() => {
    const pd = state.workspace.panelDock;
    if (!pd) return 240;
    let max = 0;
    for (const id of ids()) {
      const s = pd[id].size;
      if (s > max) max = s;
    }
    return max || 240;
  });
  return (
    <Show when={ids().length > 0}>
      <div
        class={`dock-area dock-${props.slot}`}
        style={horizontal()
          ? { height: `${totalSize()}px`, width: "100%" }
          : { width: `${totalSize()}px`, height: "100%" }}
      >
        <For each={ids()}>{(id) => <PanelById id={id} />}</For>
      </div>
    </Show>
  );
}

/** フロートウィンドウ用の最小レイアウト (ヘッダ無し / workspace 無し) */
export default function FloatApp() {
  return (
    <div class="app float-app">
      <header class="app-header float-header">
        <span class="logo">🗗 FastFiler</span>
        <span class="muted">フロート: {WINDOW_ID}</span>
      </header>
      <div class="app-body dock-grid">
        <DockArea slot="top" />
        <div class="dock-middle">
          <DockArea slot="left" />
          <main class="workspace float-workspace">
            <Show when={panelsInSlot("left").length + panelsInSlot("right").length + panelsInSlot("top").length + panelsInSlot("bottom").length === 0}>
              <div class="empty">パネルがメインに戻されました — このウィンドウは閉じてください</div>
            </Show>
          </main>
          <DockArea slot="right" />
        </div>
        <DockArea slot="bottom" />
      </div>
      <DockOverlay />
      <ToastContainer />
    </div>
  );
}
