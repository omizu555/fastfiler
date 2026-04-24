import { For, createResource, createSignal, createMemo } from "solid-js";
import {
  state,
  setActiveTab,
  closeTab,
  addTab,
  setPanePath,
  reorderTab,
  setWorkspaceTabsWidth,
  panelsInSlot,
  toggleTabLock,
} from "../store";
import { listDrives, homeDir } from "../fs";
import { driveIcon, driveTitle, iconForPath } from "../drive-util";
import PanelHeader from "./PanelHeader";

export default function VerticalTabs() {
  const [drives] = createResource(() => listDrives());
  const [dragId, setDragId] = createSignal<string | null>(null);
  const [overIdx, setOverIdx] = createSignal<number | null>(null);
  const slot = createMemo(() => state.workspace.panelDock?.tabs.slot ?? "left");
  const ownSize = createMemo(() => state.workspace.panelDock?.tabs.size ?? state.workspace.tabsWidth);
  const stackMode = createMemo(() =>
    !!state.workspace.samePanelStack && panelsInSlot(slot()).length > 1);
  const panelStyle = createMemo(() => {
    const s = slot();
    const sz = ownSize();
    if (stackMode()) {
      // 同 slot 内で他パネルと積み重ね → 主軸 flex 分配, 大きさは合わせる
      if (s === "top" || s === "bottom") return { height: "100%", width: "auto", flex: "1 1 0" };
      return { width: "100%", height: "auto", flex: "1 1 0" };
    }
    if (s === "top" || s === "bottom") {
      return { height: `${sz}px`, width: "100%", flex: `0 0 ${sz}px` };
    }
    return { width: `${sz}px`, height: "100%", flex: `0 0 ${sz}px` };
  });

  const findLeaf = (n: any): string | null => {
    if (!n) return null;
    if (n.kind === "leaf") return n.paneId;
    return findLeaf(n.a) ?? findLeaf(n.b);
  };

  const navigateActive = (path: string) => {
    const tab = state.tabs.find((t) => t.id === state.activeTabId);
    if (!tab) return;
    const id = findLeaf(tab.rootPane);
    if (id) setPanePath(id, path);
  };

  const tabLabel = (tab: { rootPane: any; title: string }) => {
    const id = findLeaf(tab.rootPane);
    const p = id ? state.panes[id]?.path : "";
    if (!p) return tab.title || "(空)";
    // basename: 末尾の \ または / を除去してから最後のセパレータ以降
    const trimmed = p.replace(/[\\/]+$/, "");
    const m = trimmed.match(/[^\\/]+$/);
    if (m) return m[0];
    // ドライブルート ("C:\" 等)
    return trimmed || p;
  };

  const tabPath = (tab: { rootPane: any }): string => {
    const id = findLeaf(tab.rootPane);
    return id ? state.panes[id]?.path ?? "" : "";
  };

  const onTabsKey = (e: KeyboardEvent) => {
    const tgt = e.target as HTMLElement | null;
    if (tgt && (tgt.tagName === "INPUT" || tgt.tagName === "TEXTAREA")) return;
    const tabs = state.tabs;
    if (tabs.length === 0) return;
    const cur = tabs.findIndex((t) => t.id === state.activeTabId);
    let next = cur;
    if (e.key === "ArrowDown" || e.key === "ArrowRight") {
      next = (cur < 0 ? 0 : (cur + 1) % tabs.length);
    } else if (e.key === "ArrowUp" || e.key === "ArrowLeft") {
      next = (cur < 0 ? tabs.length - 1 : (cur - 1 + tabs.length) % tabs.length);
    } else {
      return;
    }
    e.preventDefault();
    e.stopPropagation();
    const aside = e.currentTarget as HTMLElement;
    setActiveTab(tabs[next].id);
    // タブ切替で focusedPaneId が新タブの leaf に切替わるため、
    // 連続矢印キー操作中はフォーカスを vtabs 自身に維持する
    queueMicrotask(() => aside.focus());
  };

  return (
    <aside class="vtabs" classList={{ [`slot-${slot()}`]: true }} style={panelStyle()} tabindex={0} onKeyDown={onTabsKey}>
      <VTabsSplitter slot={slot()} />
      <button
        class="vtabs-add-floating"
        title="新規タブ"
        onClick={async () => {
          try { addTab(await homeDir()); } catch { addTab("C:\\"); }
        }}
      >＋</button>
      <PanelHeader panel="tabs" title="タブ" right={
        <button class="add" title="新規タブ" onClick={async () => {
          try { addTab(await homeDir()); } catch { addTab("C:\\"); }
        }}>＋</button>
      } />
      <div class="vtabs-head" style="display:none">
        <strong>タブ</strong>
      </div>

      <div class="drives">
        <small class="muted">ドライブ</small>
        <div class="drive-list">
          <For each={drives() ?? []}>
            {(d) => (
              <button
                class="drive"
                classList={{ [`drive-kind-${d.kind}`]: true }}
                title={driveTitle(d)}
                onClick={() => navigateActive(d.letter)}
              >
                {driveIcon(d.kind)} {d.letter}
              </button>
            )}
          </For>
        </div>
      </div>

      <div
        class="vtabs-grid"
        style={{ "grid-template-columns": `repeat(${state.tabColumns}, 1fr)` }}
      >
        <For each={state.tabs}>
          {(t, i) => (
            <div
              classList={{
                vtab: true,
                active: state.activeTabId === t.id,
                locked: !!t.locked,
                dragging: dragId() === t.id,
                "drop-before": overIdx() === i(),
              }}
              draggable={true}
              onAuxClick={(ev) => {
                if (ev.button !== 1) return;
                ev.preventDefault();
                ev.stopPropagation();
                toggleTabLock(t.id);
              }}
              onMouseDown={(ev) => {
                // 中ボタンクリック時のオートスクロール抑止
                if (ev.button === 1) ev.preventDefault();
              }}
              onDragStart={(ev) => {
                setDragId(t.id);
                ev.dataTransfer?.setData("application/x-fastfiler-tab", t.id);
                if (ev.dataTransfer) ev.dataTransfer.effectAllowed = "move";
              }}
              onDragEnd={() => { setDragId(null); setOverIdx(null); }}
              onDragOver={(ev) => {
                if (!ev.dataTransfer?.types.includes("application/x-fastfiler-tab")) return;
                ev.preventDefault();
                ev.dataTransfer.dropEffect = "move";
                const rect = ev.currentTarget.getBoundingClientRect();
                const before = (ev.clientY - rect.top) < rect.height / 2;
                setOverIdx(before ? i() : i() + 1);
              }}
              onDragLeave={() => { /* ちらつき抑制のため何もしない */ }}
              onDrop={(ev) => {
                ev.preventDefault();
                const id = ev.dataTransfer?.getData("application/x-fastfiler-tab");
                const target = overIdx();
                if (id && target !== null) reorderTab(id, target);
                setDragId(null);
                setOverIdx(null);
              }}
              onClick={() => setActiveTab(t.id)}
              title={`${t.locked ? "🔒 ロック中 " : ""}${state.panes[findLeaf(t.rootPane) ?? ""]?.path ?? t.title}\n(中クリックでロック切替)`}
            >
              <span class="vtab-icon">{iconForPath(tabPath(t), drives())}</span>
              <span class="vtab-title">{tabLabel(t)}</span>
              {t.locked ? (
                <button
                  class="vtab-close vtab-locked-btn"
                  title="ロック解除 (中ボタンクリックでも切替)"
                  onClick={(e) => { e.stopPropagation(); toggleTabLock(t.id); }}
                >🔒</button>
              ) : (
                <button
                  class="vtab-close"
                  onClick={(e) => { e.stopPropagation(); closeTab(t.id); }}
                >×</button>
              )}
            </div>
          )}
        </For>
      </div>

      <div class="vtabs-foot">
        <small class="muted">FastFiler</small>
      </div>
    </aside>
  );
}

function VTabsSplitter(props: { slot: string }) {
  const onDown = (e: PointerEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startY = e.clientY;
    const startW = state.workspace.tabsWidth;
    const startH = state.workspace.tabsWidth; // size 共通フィールド
    const horizontal = props.slot === "top" || props.slot === "bottom";
    const onRight = props.slot === "right";
    const onBottom = props.slot === "bottom";
    const move = (ev: PointerEvent) => {
      if (horizontal) {
        const dy = ev.clientY - startY;
        setWorkspaceTabsWidth(onBottom ? startH - dy : startH + dy);
      } else {
        const dx = ev.clientX - startX;
        setWorkspaceTabsWidth(onRight ? startW - dx : startW + dx);
      }
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };
  return (
    <div
      class="vtabs-splitter"
      classList={{
        "on-right": props.slot === "right",
        "on-top": props.slot === "top",
        "on-bottom": props.slot === "bottom",
        "horizontal": props.slot === "top" || props.slot === "bottom",
      }}
      onPointerDown={onDown}
      title="ドラッグでサイズ変更"
    />
  );
}
