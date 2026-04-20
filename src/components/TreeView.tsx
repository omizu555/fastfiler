import { For, Show, createEffect, createMemo, createResource, createSignal } from "solid-js";
import { listDirs } from "../fs";
import { ancestorChain, joinPath, normalizePath, splitPath } from "../path-util";
import { setFocusedPane, setPanePath, setPaneView, state } from "../store";
import { iconForEntryWith } from "../icons";

interface Props {
  paneId: string;
}

export default function TreeView(props: Props) {
  const pane = () => state.panes[props.paneId];
  const [expanded, setExpanded] = createSignal<Set<string>>(new Set());

  // ルート: 現在パスの root (ドライブ または UNC \\srv\sh)
  const rootPath = createMemo(() => splitPath(pane().path)?.root ?? pane().path);

  // pane.path に到達するまでの祖先パスを自動展開
  createEffect(() => {
    const chain = ancestorChain(pane().path);
    if (chain.length === 0) return;
    setExpanded((old) => {
      const next = new Set(old);
      for (const p of chain) next.add(normalizePath(p));
      return next;
    });
  });

  const toggle = (path: string) => {
    setExpanded((old) => {
      const k = normalizePath(path);
      const next = new Set(old);
      if (next.has(k)) next.delete(k);
      else next.add(k);
      return next;
    });
  };

  return (
    <div
      class="treeview"
      classList={{ "pane-focused": state.focusedPaneId === props.paneId }}
      onPointerDown={() => setFocusedPane(props.paneId)}
    >
      <div class="treeview-head">
        <span>📂 ツリー</span>
        <button title="リスト表示に戻す" onClick={() => setPaneView(props.paneId, "list")}>📋</button>
      </div>
      <div class="treeview-body">
        <TreeNode
          path={rootPath()}
          label={rootPath()}
          paneId={props.paneId}
          depth={0}
          expanded={expanded}
          toggle={toggle}
        />
      </div>
    </div>
  );
}

interface NodeProps {
  path: string;
  label: string;
  paneId: string;
  depth: number;
  expanded: () => Set<string>;
  toggle: (p: string) => void;
}

function TreeNode(props: NodeProps) {
  const isOpen = () => props.expanded().has(normalizePath(props.path));
  const isCurrent = () => normalizePath(state.panes[props.paneId].path) === normalizePath(props.path);

  // expanded のときのみ子ディレクトリを取得
  const [children] = createResource(
    () => (isOpen() ? props.path + "::" + (state.showHidden ? "h" : "n") : null),
    async () => listDirs(props.path, state.showHidden),
  );

  const onClick = (e: MouseEvent) => {
    if (e.detail >= 2) {
      // ダブルクリック → 展開トグル
      props.toggle(props.path);
      return;
    }
    setPanePath(props.paneId, props.path);
  };

  return (
    <div class="tree-node">
      <div
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
        <span class="tree-icon">{iconForEntryWith({ kind: "dir" }, state.iconSet)}</span>
        <span class="tree-label">{props.label}</span>
      </div>
      <Show when={isOpen()}>
        <Show when={!children.loading} fallback={<div class="tree-loading" style={{ "padding-left": `${(props.depth + 1) * 14}px` }}>…</div>}>
          <For each={children() ?? []}>
            {(c) => (
              <TreeNode
                path={joinPath(props.path, c.name)}
                label={c.name}
                paneId={props.paneId}
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
