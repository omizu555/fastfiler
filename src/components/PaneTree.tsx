import { Show } from "solid-js";
import type { PaneNode } from "../types";
import { setSplitRatio, state } from "../store";
import FileList from "./FileList";
import TreeView from "./TreeView";
import DriveListView from "./DriveListView";
import { isDrivesPath } from "../fs";

interface Props {
  node: PaneNode;
  tabId: string;
  path?: number[];
}

function Leaf(p: { tabId: string; paneId: string }) {
  const pane = () => state.panes[p.paneId];
  const view = () => pane()?.view ?? "list";
  return (
    <Show when={!isDrivesPath(pane()?.path ?? "")} fallback={<DriveListView tabId={p.tabId} paneId={p.paneId} />}>
      <Show when={view() === "tree"} fallback={<FileList tabId={p.tabId} paneId={p.paneId} />}>
        <TreeView paneId={p.paneId} />
      </Show>
    </Show>
  );
}

export default function PaneTree(props: Props) {
  const path = () => props.path ?? [];

  return (
    <Show
      when={props.node.kind === "split"}
      fallback={
        <Leaf
          tabId={props.tabId}
          paneId={(props.node as { kind: "leaf"; paneId: string }).paneId}
        />
      }
    >
      <SplitView
        node={props.node as Extract<PaneNode, { kind: "split" }>}
        tabId={props.tabId}
        path={path()}
      />
    </Show>
  );
}

function SplitView(p: {
  node: Extract<PaneNode, { kind: "split" }>;
  tabId: string;
  path: number[];
}) {
  let containerRef: HTMLDivElement | undefined;
  const isH = () => p.node.dir === "h";

  const startDrag = (ev: PointerEvent) => {
    ev.preventDefault();
    const target = ev.currentTarget as HTMLElement;
    target.setPointerCapture(ev.pointerId);
    const move = (e: PointerEvent) => {
      if (!containerRef) return;
      const rect = containerRef.getBoundingClientRect();
      const ratio = isH()
        ? (e.clientX - rect.left) / rect.width
        : (e.clientY - rect.top) / rect.height;
      setSplitRatio(p.tabId, p.path, Math.min(0.95, Math.max(0.05, ratio)));
    };
    const up = (e: PointerEvent) => {
      target.releasePointerCapture(e.pointerId);
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <div
      ref={containerRef}
      class={`split ${isH() ? "split-h" : "split-v"}`}
      style={{
        "grid-template-columns": isH() ? `${p.node.ratio * 100}% 4px 1fr` : "1fr",
        "grid-template-rows": isH() ? "1fr" : `${p.node.ratio * 100}% 4px 1fr`,
      }}
    >
      <PaneTree node={p.node.a} tabId={p.tabId} path={[...p.path, 0]} />
      <div class="splitter" onPointerDown={startDrag} />
      <PaneTree node={p.node.b} tabId={p.tabId} path={[...p.path, 1]} />
    </div>
  );
}
