import { For, Show, createEffect, createMemo, createResource, createSignal } from "solid-js";
import { listDirs, listDrives } from "../fs";
import { ancestorChain, joinPath, normalizePath, uncServerOf } from "../path-util";
import {
  focusedLeafPaneId,
  setPanePath,
  setPaneLinkGroup,
  setWorkspaceTreeApply,
  setWorkspaceTreeWidth,
  state,
} from "../store";
import type { DriveInfo, PaneNode } from "../types";
import { driveDisplayLabel, driveIcon, driveTitle } from "../drive-util";
import PanelHeader from "./PanelHeader";

function leavesOf(n: PaneNode): string[] {
  if (n.kind === "leaf") return [n.paneId];
  return [...leavesOf(n.a), ...leavesOf(n.b)];
}

function applyPathToTargets(path: string) {
  const apply = state.workspace.treeApply;
  if (apply === "active") {
    const pid = focusedLeafPaneId();
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
    const pid = focusedLeafPaneId();
    if (pid) {
      setPaneLinkGroup(pid, targetGroup);
      setPanePath(pid, path);
    }
  }
}

const UNC_LS_KEY = "fastfiler.uncShares";

function loadUncShares(): Set<string> {
  try {
    const raw = localStorage.getItem(UNC_LS_KEY);
    if (!raw) return new Set();
    const arr = JSON.parse(raw);
    if (!Array.isArray(arr)) return new Set();
    return new Set(arr.filter((x): x is string => typeof x === "string" && x.startsWith("\\\\")));
  } catch {
    return new Set();
  }
}

function saveUncShares(s: Set<string>) {
  try {
    localStorage.setItem(UNC_LS_KEY, JSON.stringify(Array.from(s)));
  } catch {
    // ignore
  }
}

interface NodeProps {
  path: string;
  label: string;
  title?: string;
  depth: number;
  expanded: () => Set<string>;
  toggle: (p: string) => void;
  onContextMenu?: (e: MouseEvent) => void;
}

function TreeNode(props: NodeProps) {
  const isOpen = () => props.expanded().has(normalizePath(props.path));
  const isCurrent = () => {
    const pid = focusedLeafPaneId();
    if (!pid) return false;
    return normalizePath(state.panes[pid].path) === normalizePath(props.path);
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
        onContextMenu={(e) => {
          if (props.onContextMenu) {
            e.preventDefault();
            e.stopPropagation();
            props.onContextMenu(e);
          }
        }}
        title={props.title ?? props.path}
      >
        <span
          class="tree-toggle"
          onClick={(e) => { e.stopPropagation(); props.toggle(props.path); }}
        >{isOpen() ? "▾" : "▸"}</span>
        <Show when={props.depth > 0}>
          <span class="tree-icon">📁</span>
        </Show>
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
  // 開いた UNC 共有 (\\server\share) を蓄積。サーバ別にツリー先頭にグルーピング表示
  const [uncShares, setUncShares] = createSignal<Set<string>>(loadUncShares());

  const toggle = (path: string) => {
    setExpanded((old) => {
      const k = normalizePath(path);
      const next = new Set(old);
      if (next.has(k)) next.delete(k);
      else next.add(k);
      return next;
    });
  };

  // フォーカスペインの path を購読 → 祖先パスを自動展開、UNC 共有を登録
  createEffect(() => {
    const pid = focusedLeafPaneId();
    if (!pid) return;
    const cur = state.panes[pid]?.path ?? "";
    if (!cur || cur.startsWith("::")) return;
    const chain = ancestorChain(cur);
    if (chain.length === 0) return;

    setExpanded((old) => {
      const next = new Set(old);
      // UNC ならサーバノードも展開対象に
      const server = uncServerOf(chain[0]);
      if (server) next.add(normalizePath(server));
      for (const p of chain) next.add(normalizePath(p));
      return next;
    });

    // UNC ルート (\\server\share) を共有リストに記憶
    const root = chain[0];
    if (uncServerOf(root)) {
      setUncShares((old) => {
        if (old.has(root)) return old;
        const ns = new Set(old);
        ns.add(root);
        saveUncShares(ns);
        return ns;
      });
    }
  });

  // UNC を server -> shares[] にグルーピング
  const uncServers = createMemo(() => {
    const map = new Map<string, string[]>();
    for (const sh of uncShares()) {
      // sh = \\server\share
      const m = sh.match(/^\\\\([^\\]+)\\([^\\]+)$/);
      if (!m) continue;
      const server = `\\\\${m[1]}`;
      if (!map.has(server)) map.set(server, []);
      map.get(server)!.push(sh);
    }
    return Array.from(map.entries())
      .map(([server, shares]) => ({ server, shares: shares.sort() }))
      .sort((a, b) => a.server.localeCompare(b.server));
  });

  const removeShare = (share: string) => {
    setUncShares((old) => {
      if (!old.has(share)) return old;
      const ns = new Set(old);
      ns.delete(share);
      saveUncShares(ns);
      return ns;
    });
  };

  const slot = createMemo(() => state.workspace.panelDock?.tree.slot ?? "left");
  const width = createMemo(() => state.workspace.treeWidth);
  const panelStyle = createMemo(() => {
    const s = slot();
    if (s === "top" || s === "bottom") return { height: "100%", width: "auto", flex: "1 1 0" };
    return { width: "100%", height: "auto", flex: "1 1 0" };
  });

  // splitter ドラッグ
  let startX = 0;
  let startY = 0;
  let startW = 0;
  const onSplitterDown = (e: PointerEvent) => {
    startX = e.clientX;
    startY = e.clientY;
    startW = width();
    const s = slot();
    const horizontal = s === "top" || s === "bottom";
    const onRight = s === "right";
    const onBottom = s === "bottom";
    const move = (ev: PointerEvent) => {
      if (horizontal) {
        const dy = ev.clientY - startY;
        setWorkspaceTreeWidth(onBottom ? startW - dy : startW + dy);
      } else {
        const dx = ev.clientX - startX;
        setWorkspaceTreeWidth(onRight ? startW - dx : startW + dx);
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
    <aside class="workspace-tree" classList={{ [`slot-${slot()}`]: true }} style={panelStyle()}>
      <PanelHeader panel="tree" title="🌲 ツリー" right={
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
      } />
      <div class="workspace-tree-body">
        <Show when={!drives.loading} fallback={<div class="tree-loading">…</div>}>
          <For each={drives() ?? []}>
            {(d) => (
              <TreeNode
                path={d.letter}
                label={`${driveIcon(d.kind)} ${driveDisplayLabel(d)}`}
                title={driveTitle(d)}
                depth={0}
                expanded={expanded}
                toggle={toggle}
              />
            )}
          </For>
          <For each={uncServers()}>
            {(srv) => {
              const isOpen = () => expanded().has(normalizePath(srv.server));
              return (
                <>
                  <div
                    class="tree-row"
                    style={{ "padding-left": "0px" }}
                    onClick={() => toggle(srv.server)}
                    title={srv.server}
                  >
                    <span
                      class="tree-toggle"
                      onClick={(e) => { e.stopPropagation(); toggle(srv.server); }}
                    >{isOpen() ? "▾" : "▸"}</span>
                    <span class="tree-label">🖥️ {srv.server}</span>
                  </div>
                  <Show when={isOpen()}>
                    <For each={srv.shares}>
                      {(sh) => (
                        <TreeNode
                          path={sh}
                          label={`🌐 ${sh}`}
                          title={`${sh} (記憶された共有 - 右クリックで削除)`}
                          depth={1}
                          expanded={expanded}
                          toggle={toggle}
                          onContextMenu={() => {
                            if (confirm(`ツリーから ${sh} を削除しますか?`)) removeShare(sh);
                          }}
                        />
                      )}
                    </For>
                  </Show>
                </>
              );
            }}
          </For>
        </Show>
      </div>
      <div class="workspace-tree-splitter"
        classList={{ horizontal: slot() === "top" || slot() === "bottom" }}
        onPointerDown={onSplitterDown} title="ドラッグでサイズ変更" />
    </aside>
  );
}
