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
  bumpRefreshPaths,
  navigateBack,
  navigateForward,
  pushToast,
} from "./store";
import { homeDir } from "./fs";
import { joinPath, parentPath, volumeOf } from "./path-util";
import { setExtDragOver, clearExtDragOver, getInternalDragPaths, isInternalDropPaths, clearInternalDrag, installInternalPointerDnd } from "./file-list/dnd";
import { resolveDestinations, refreshTargets } from "./file-list/resolve-dest";
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

  // WebView2 D&D drop 時の修飾キー判定用 (drop イベントには含まれないため自前追跡)
  let ctrlDown = false;
  const onModKey = (e: KeyboardEvent) => { ctrlDown = e.ctrlKey; };
  window.addEventListener("keydown", onModKey, true);
  window.addEventListener("keyup", onModKey, true);
  onCleanup(() => {
    window.removeEventListener("keydown", onModKey, true);
    window.removeEventListener("keyup", onModKey, true);
  });

  onMount(async () => {
    ensureRightDragInstalled();
    const uninstallPtrDnd = installInternalPointerDnd();
    onCleanup(() => uninstallPtrDnd());
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
        if (!paths.length) {
          console.warn("[ole-dnd] drop: empty paths");
          return;
        }
        const { cx, cy } = toCssCoords(x, y);
        const els = document.elementsFromPoint(cx, cy) as HTMLElement[];
        const folderRow = els.find((el) => el.dataset.rdFolder === "1") as
          | HTMLElement
          | undefined;
        const paneEl = els.find((el) => el.dataset.paneId) as
          | HTMLElement
          | undefined;
        const targetPaneId = paneEl?.dataset.paneId ?? focusedLeafPaneId();
        let targetPath: string | null = null;
        if (
          folderRow &&
          folderRow.dataset.rdPanePath &&
          folderRow.dataset.rdName
        ) {
          targetPath = joinPath(
            folderRow.dataset.rdPanePath,
            folderRow.dataset.rdName,
          );
        } else if (targetPaneId) {
          targetPath = state.panes[targetPaneId]?.path ?? null;
        }
        if (!targetPath) {
          console.warn("[ole-dnd] drop: no target path resolved");
          return;
        }
        // v1.7.3: Ctrl=copy / それ以外は src/dst の volume 比較で move/copy 自動選択。
        // (effect は WebView2/エクスプローラ間のネゴシエーションで決まり、ユーザー
        //  意図と必ずしも一致しないため自前で判定する)
        const internal = isInternalDropPaths(paths);
        let op: "copy" | "move";
        if (ctrlDown) {
          op = "copy";
        } else if (internal) {
          op = "move";
        } else {
          const srcVol = volumeOf(paths[0]);
          const dstVol = volumeOf(targetPath);
          op = srcVol && dstVol && srcVol === dstVol ? "move" : "copy";
        }
        if (internal) clearInternalDrag();
        void effect; // 未使用警告抑止
        const items = await resolveDestinations(paths, targetPath, op);
        if (items.length === 0) {
          pushToast("対象がありません (同じ場所への移動)", "info");
          return;
        }
        const renamedCount = items.filter((it) => it.renamed).length;
        const label = `${op === "copy" ? "コピー" : "移動"} (${items.length} 件) → ${targetPath}`;
        const r = await runFileJob(
          op,
          items.map(({ from, to }) => ({ from, to })),
          { label },
        );
        if (r.ok) {
          const ops: UndoOp[] = items.map((it) =>
            op === "copy"
              ? { kind: "copy", created: it.to }
              : { kind: "move", from: it.from, to: it.to },
          );
          pushUndo(label, ops);
          const sourceDirs =
            op === "move" ? items.map((it) => parentPath(it.from)) : [];
          bumpRefreshPaths([targetPath, ...sourceDirs]);
          const note = renamedCount > 0 ? ` (${renamedCount}件は名前変更)` : "";
          pushToast(
            `${op === "copy" ? "コピー" : "移動"} ${items.length}件 完了${note}`,
            "info",
          );
        } else if (!r.canceled) {
          console.error(`[ole-drop] ${label} 失敗`);
          pushToast(`${op === "copy" ? "コピー" : "移動"} 失敗`, "error");
        }
      });
      unlistens.push(unDrop);

      // v1.7.2: WebView2 領域上のドロップを受けるため Tauri 標準 D&D を併用。
      // Rust 自前 IDropTarget はメイン HWND のみで WebView2 領域には届かないため。
      try {
        const { getCurrentWebview } = await import("@tauri-apps/api/webview");
        const wv = getCurrentWebview();
        const unWv = await wv.onDragDropEvent((ev) => {
          const payload = ev.payload as {
            type: "enter" | "over" | "drop" | "leave";
            paths?: string[];
            position?: { x: number; y: number };
          };
          if (payload.type === "leave") {
            clearExtDragOver();
            return;
          }
          const pos = payload.position;
          if (!pos) return;
          const { cx, cy } = toCssCoords(pos.x, pos.y);
          const els = document.elementsFromPoint(cx, cy) as HTMLElement[];
          const folderRow = els.find((el) => el.dataset.rdFolder === "1") as
            | HTMLElement | undefined;
          const paneEl = els.find((el) => el.dataset.paneId) as
            | HTMLElement | undefined;
          const targetPaneId = paneEl?.dataset.paneId ?? null;

          if (payload.type === "enter" || payload.type === "over") {
            if (targetPaneId) {
              setExtDragOver(targetPaneId, folderRow?.dataset.rdName ?? null);
            } else {
              clearExtDragOver();
            }
            return;
          }

          // drop
          clearExtDragOver();
          const paths = payload.paths ?? [];
          if (!paths.length) return;
          let targetPath: string | null = null;
          if (
            folderRow &&
            folderRow.dataset.rdPanePath &&
            folderRow.dataset.rdName
          ) {
            targetPath = joinPath(
              folderRow.dataset.rdPanePath,
              folderRow.dataset.rdName,
            );
          } else {
            const pid = targetPaneId ?? focusedLeafPaneId();
            if (pid) targetPath = state.panes[pid]?.path ?? null;
          }
          if (!targetPath) {
            console.warn("[wv-drop] no target path resolved");
            return;
          }
          // Ctrl+ドロップ→ 強制 copy。
          // それ以外:
          //   - 内部 D&D (paths が internalDragPaths と一致) → move (アプリ内)
          //   - 外部 D&D → src/dst の volume を比較 (同一=move / 別=copy)
          const internal = isInternalDropPaths(paths);
          let op: "copy" | "move";
          if (ctrlDown) {
            op = "copy";
          } else if (internal) {
            op = "move";
          } else {
            const srcVol = volumeOf(paths[0]);
            const dstVol = volumeOf(targetPath);
            op = srcVol && dstVol && srcVol === dstVol ? "move" : "copy";
          }
          if (internal) clearInternalDrag();
          const dest = targetPath;
          const logTag = internal ? "[internal-drop]" : "[wv-drop]";
          void (async () => {
            const items = await resolveDestinations(paths, dest, op);
            if (items.length === 0) {
              pushToast("対象がありません (同じ場所への移動)", "info");
              return;
            }
            const renamedCount = items.filter((it) => it.renamed).length;
            const label = `${op === "copy" ? "コピー" : "移動"} (${items.length} 件) → ${dest}`;
            const r = await runFileJob(
              op,
              items.map(({ from, to }) => ({ from, to })),
              { label },
            );
            if (r.ok) {
              const ops: UndoOp[] = items.map((it) =>
                op === "copy"
                  ? { kind: "copy", created: it.to }
                  : { kind: "move", from: it.from, to: it.to },
              );
              pushUndo(label, ops);
              bumpRefreshPaths(refreshTargets(items, dest, op === "move"));
              const note = renamedCount > 0 ? ` (${renamedCount}件は名前変更)` : "";
              pushToast(
                `${op === "copy" ? "コピー" : "移動"} ${items.length}件 完了${note}`,
                "info",
              );
            } else if (!r.canceled) {
              console.error(`${logTag} ${label} 失敗`);
              pushToast(`${op === "copy" ? "コピー" : "移動"} 失敗`, "error");
            }
          })();
        });
        unlistens.push(unWv);
      } catch (err) {
        console.warn("[wv-drop] init failed", err);
      }
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
