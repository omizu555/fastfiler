import { Show, createMemo } from "solid-js";
import VerticalTabs from "./components/VerticalTabs";
import PaneTree from "./components/PaneTree";
import { state } from "./store";

export default function App() {
  const activeTab = createMemo(() =>
    state.tabs.find((t) => t.id === state.activeTabId),
  );

  return (
    <div class="app">
      <header class="app-header">
        <span class="logo">⚡ FastFiler</span>
        <span class="muted">UI モック (Vite + Solid)</span>
        <span class="spacer" />
        <span class="muted">Phase 0–2 デモ: 縦タブ / 任意分割 / ペイン連動</span>
      </header>
      <div class="app-body">
        <VerticalTabs />
        <main class="workspace">
          <Show when={activeTab()} fallback={<div class="empty">タブなし</div>}>
            <PaneTree node={activeTab()!.rootPane} tabId={activeTab()!.id} />
          </Show>
        </main>
      </div>
    </div>
  );
}
