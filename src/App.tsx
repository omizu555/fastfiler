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
import ToastContainer from "./components/ToastContainer";
import StatusBarToast from "./components/StatusBarToast";
import RightDragOverlay from "./components/RightDragOverlay";
import { ensureRightDragInstalled } from "./file-list/right-drag";
import JobsPanel from "./components/JobsPanel";
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
  setPanelSize,
  focusedLeafPaneId,
  pushUndo,
  pushToast,
  bumpRefreshPaths,
  navigateBack,
  navigateForward,
} from "./store";
import { homeDir } from "./fs";
import { joinPath, parentPath } from "./path-util";
import { setExtDragOver, clearExtDragOver } from "./file-list/dnd";
import { matchKey } from "./hotkeys";
import { performUndo } from "./undo";
import { runFileJob } from "./jobs";
import type { DockSlot, PanelId, UndoOp } from "./types";

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
    try {
      const home = await homeDir();
      setInitialPath(home);
    } catch {
      /* ignore */
    }
    // v4.0+: OLE D&D 受信 (エクスプローラからのドロップ)
    try {
      const { listen } = await import("@tauri-apps/api/event");

      // Rust 側 ScreenToClient は物理ピクセルを返すため、CSS ピクセルへ変換する
      const toCssCoords = (
        x: number,
        y: number,
      ): { cx: number; cy: number } => {
        const dpr = window.devicePixelRatio || 1;
        return { cx: x / dpr, cy: y / dpr };
      };

      // ドラッグオーバー中のビジュアルフィードバック
      const unDragOver = await listen<{ effect: number; x: number; y: number }>(
        "ole-drag-over",
        (e) => {
          const { cx, cy } = toCssCoords(e.payload.x, e.payload.y);
          // elementsFromPoint は z-order の上から全要素を返す
          const els = document.elementsFromPoint(cx, cy) as HTMLElement[];
          // フォルダ行を優先して探す
          const folderRow = els.find((el) => el.dataset.rdFolder === "1") as
            | HTMLElement
            | undefined;
          // ペインを特定
          const paneEl = els.find((el) => el.dataset.paneId) as
            | HTMLElement
            | undefined;
          const paneId = paneEl?.dataset.paneId ?? null;
          // eslint-disable-next-line no-console
          console.debug("[ole-dnd] drag-over", {
            raw: e.payload,
            dpr: window.devicePixelRatio,
            cx,
            cy,
            paneId,
            folderRow: folderRow?.dataset.rdName ?? null,
            elsTop: els[0]?.tagName,
          });
          if (paneId) {
            setExtDragOver(paneId, folderRow?.dataset.rdName ?? null);
          } else {
            clearExtDragOver();
          }
        },
      );
      unlistens.push(unDragOver);

      // ドラッグ離脱
      const unDragLeave = await listen("ole-drag-leave", () => {
        // eslint-disable-next-line no-console
        console.debug("[ole-dnd] drag-leave");
        clearExtDragOver();
      });
      unlistens.push(unDragLeave);

      // ドロップ完了
      const unDrop = await listen<{
        paths: string[];
        effect: number;
        x: number;
        y: number;
      }>("ole-drop", async (e) => {
        const { paths, effect, x, y } = e.payload;
        clearExtDragOver();
        // eslint-disable-next-line no-console
        console.debug("[ole-dnd] drop received", {
          paths,
          effect,
          x,
          y,
          dpr: window.devicePixelRatio,
        });
        if (!paths.length) {
          // eslint-disable-next-line no-console
          console.warn("[ole-dnd] drop: empty paths");
          return;
        }
        const { cx, cy } = toCssCoords(x, y);
        // elementsFromPoint でフォルダ行 → ペインの順に検索
        const els = document.elementsFromPoint(cx, cy) as HTMLElement[];
        const folderRow = els.find((el) => el.dataset.rdFolder === "1") as
          | HTMLElement
          | undefined;
        const paneEl = els.find((el) => el.dataset.paneId) as
          | HTMLElement
          | undefined;
        const targetPaneId = paneEl?.dataset.paneId ?? focusedLeafPaneId();
        // eslint-disable-next-line no-console
        console.debug("[ole-dnd] drop hit-test", {
          cx,
          cy,
          targetPaneId,
          folderRow: folderRow?.dataset.rdName ?? null,
          elTags: els
            .slice(0, 5)
            .map(
              (el) =>
                `${el.tagName}.${el.className?.toString().slice(0, 30) ?? ""}`,
            ),
        });
        let targetPath: string | null = null;
        if (
          folderRow &&
          folderRow.dataset.rdPanePath &&
          folderRow.dataset.rdName
        ) {
          // フォルダ行の上にドロップ → そのフォルダ内
          targetPath = joinPath(
            folderRow.dataset.rdPanePath,
            folderRow.dataset.rdName,
          );
        } else if (targetPaneId) {
          // ペイン背景へのドロップ → ペインの現在フォルダ
          targetPath = state.panes[targetPaneId]?.path ?? null;
        }
        if (!targetPath) {
          // eslint-disable-next-line no-console
          console.warn("[ole-dnd] drop: no target path resolved");
          return;
        }
        const isMove = (effect & 2) !== 0;
        const op: "copy" | "move" = isMove ? "move" : "copy";
        const items = paths.map((from) => {
          const name =
            from
              .replace(/[\\/]+$/, "")
              .split(/[\\/]/)
              .pop() || "";
          const sep =
            targetPath!.endsWith("\\") || targetPath!.endsWith("/") ? "" : "\\";
          return { from, to: `${targetPath}${sep}${name}` };
        });
        const label = `${op === "copy" ? "コピー" : "移動"} (${items.length} 件) → ${targetPath}`;
        const r = await runFileJob(op, items, { label });
        if (r.ok) {
          const ops: UndoOp[] = items.map((it) =>
            op === "copy"
              ? { kind: "copy", created: it.to }
              : { kind: "move", from: it.from, to: it.to },
          );
          pushUndo(label, ops);
          pushToast(label, "info", {
            label: "↶取り消し",
            onClick: () => {
              void performUndo();
            },
          });
          const sourceDirs =
            op === "move" ? items.map((it) => parentPath(it.from)) : [];
          bumpRefreshPaths([targetPath, ...sourceDirs]);
        } else if (!r.canceled) {
          pushToast(`${label} 失敗`, "error");
        }
      });
      unlistens.push(unDrop);
    } catch {
      /* non-tauri */
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
    unlistens.push(() => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("contextmenu", onCtx);
      window.removeEventListener("mousedown", onMouseDown);
      window.removeEventListener("auxclick", onAuxClick);
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
      <div class="notification-area">
        <ToastContainer />
        <JobsPanel />
      </div>
      <RightDragOverlay />
    </div>
  );
}
