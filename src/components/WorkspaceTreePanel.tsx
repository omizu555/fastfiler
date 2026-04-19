import { For, Show, createEffect, createMemo, createResource, createSignal } from "solid-js";
import { listDirs, listDrives } from "../fs";
import {
  focusedLeafPaneId,
  setPanePath,
  setPaneLinkGroup,
  setWorkspaceTreeApply,
  setWorkspaceTreeWidth,
  state,
} from "../store";
import type { DriveInfo, PaneNode } from "../types";

function activeLeafPaneId(): string | null {
  return focusedLeafPaneId();
}

function leavesOf(n: PaneNode): string[] {
  if (n.kind === "leaf") return [n.paneId];
  return [...leavesOf(n.a), ...leavesOf(n.b)];
}

function applyPathToTargets(path: string) {
  const apply = state.workspace.treeApply;
  if (apply === "active") {
    const pid = activeLeafPaneId();
    if (pid) setPanePath(pid, path);
    return;
  }
  // link-red / link-blue: アクティブタブの全 leaf のうち、対応グループに所属するペインに反映
  const t = state.tabs.find((t) => t.id === state.activeTabId);
  if (!t) return;
  const targetGroup = apply === "red" ? "red" : "blue";
  let applied = 0;
  for (const pid of leavesOf(t.rootPane)) {
    const p = state.panes[pid];
    if (p?.linkGroupId === targetGroup) {
      setPanePath(pid, path);
      applied++;
    }
  }
  if (applied === 0) {
    // 該当ペインが無ければアクティブを連動グループに登録した上で反映
    const pid = activeLeafPaneId();
    if (pid) {
      setPaneLinkGroup(pid, targetGroup);
      setPanePath(pid, path);
    }
  }
}

function normalize(p: string): string {
  return p.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();
}

function joinPath(base: string, name: string): string {
  if (base.endsWith("\\") || base.endsWith("/")) return base + name;
  return base + "\\" + name;
}

interface NodeProps {
  path: string;
  label: string;
  depth: number;
  expanded: () => Set<string>;
  toggle: (p: string) => void;
}

function TreeNode(props: NodeProps) {
  const isOpen = () => props.expanded().has(normalize(props.path));
  const isCurrent = () => {
    const pid = activeLeafPaneId();
    if (!pid) return false;
    return normalize(state.panes[pid].path) === normalize(props.path);
  };

  const [children] = createResource(
    () => (isOpen() ? props.path + "::" + (state.showHidden ? "h" : "n") : null),
    async () => listDirs(props.path, state.showHidden),
  );

  let rowRef: HTMLDivElement | undefined;
  // 自分が現在パスになったらビューに収める
  createEffect(() => {
    if (isCurrent() && rowRef) {
      // 展開アニメーション後の高さ確定を待たずに済むよう microtask
      queueMicrotask(() => rowRef?.scrollIntoView({ block: "nearest" }));
    }
  });

  const onClick = (e: MouseEvent) => {
    if (e.detail >= 2) {
      props.toggle(props.path);
      return;
    }
    applyPathToTargets(props.path);
  };

  return (
    <div class="tree-node">
      <div
        ref={rowRef}
        class="tree-row"
        classList={{ current: isCurrent() }}
        style={{ "padding-left": `${props.depth * 14}px` }}
        onClick={onClick}
        title={props.path}
      >
        <span
          class="tree-toggle"
          onClick={(e) => { e.stopPropagation(); props.toggle(props.path); }}
        >{isOpen() ? "▾" : "▸"}</span>
        <span class="tree-icon">{props.depth === 0 ? "💽" : "📁"}</span>
        <span class="tree-label">{props.label}</span>
      </div>
      <Show when={isOpen()}>
        <Show
          when={!children.loading}
          fallback={<div class="tree-loading" style={{ "padding-left": `${(props.depth + 1) * 14}px` }}>…</div>}
        >
          <For each={children() ?? []}>
            {(c) => (
              <TreeNode
                path={joinPath(props.path, c.name)}
                label={c.name}
                depth={props.depth + 1}
                expanded={props.expanded}
                toggle={props.toggle}
              />
            )}
          </For>
        </Show>
      </Show>
    </div>
  );
}

export default function WorkspaceTreePanel() {
  const [drives] = createResource<DriveInfo[]>(async () => {
    try { return await listDrives(); } catch { return []; }
  });
  const [expanded, setExpanded] = createSignal<Set<string>>(new Set());

  const toggle = (path: string) => {
    setExpanded((old) => {
      const k = normalize(path);
      const next = new Set(old);
      if (next.has(k)) next.delete(k);
      else next.add(k);
      return next;
    });
  };

  // アクティブペインの path を購読 → 祖先パスを自動展開
  createEffect(() => {
    const pid = activeLeafPaneId();
    if (!pid) return;
    const cur = state.panes[pid]?.path ?? "";
    if (!cur || cur.startsWith("::")) return;
    const norm = cur.replace(/\//g, "\\");
    setExpanded((old) => {
      const next = new Set(old);
      const parts = norm.split("\\").filter(Boolean);
      let acc = "";
      for (let i = 0; i < parts.length; i++) {
        acc = i === 0 ? parts[0] + "\\" : (acc.endsWith("\\") ? acc + parts[i] : acc + "\\" + parts[i]);
        next.add(normalize(acc));
      }
      return next;
    });
  });

  const width = createMemo(() => state.workspace.treeWidth);

  // splitter ドラッグ
  let startX = 0;
  let startW = 0;
  const onSplitterDown = (e: PointerEvent) => {
    startX = e.clientX;
    startW = width();
    const move = (ev: PointerEvent) => {
      const dx = ev.clientX - startX;
      // tabsRight の場合は逆方向だが、ツリーは常にメインの左側に配置
      setWorkspaceTreeWidth(startW + dx);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <aside class="workspace-tree" style={{ width: width() + "px" }}>
      <div class="workspace-tree-head">
        <span>🌲 ツリー</span>
        <span class="spacer" />
        <select
          class="apply-select"
          title="クリック時の反映先"
          value={state.workspace.treeApply}
          onChange={(e) => setWorkspaceTreeApply(e.currentTarget.value as never)}
        >
          <option value="active">アクティブ</option>
          <option value="red">🔴 連動Red</option>
          <option value="blue">🔵 連動Blue</option>
        </select>
      </div>
      <div class="workspace-tree-body">
        <Show when={!drives.loading} fallback={<div class="tree-loading">…</div>}>
          <For each={drives() ?? []}>
            {(d) => (
              <TreeNode
                path={d.letter}
                label={d.label && d.label !== d.letter ? `${d.letter} (${d.label})` : d.letter}
                depth={0}
                expanded={expanded}
                toggle={toggle}
              />
            )}
          </For>
        </Show>
      </div>
      <div class="workspace-tree-splitter" onPointerDown={onSplitterDown} title="ドラッグで幅変更" />
    </aside>
  );
}
