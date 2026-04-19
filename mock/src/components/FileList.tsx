import { For, Show, createMemo } from "solid-js";
import { listDir, joinPath, parentPath } from "../mockFs";
import {
  setPanePath,
  setPaneSelection,
  setPaneScroll,
  setPaneLinkGroup,
  splitPane,
  closePane,
  state,
} from "../store";

interface Props {
  paneId: string;
  tabId: string;
}

export default function FileList(props: Props) {
  const pane = () => state.panes[props.paneId];
  const entries = createMemo(() => listDir(pane().path));

  const onRowClick = (name: string, ev: MouseEvent) => {
    const sel = pane().selection;
    if (ev.ctrlKey) {
      const next = sel.includes(name)
        ? sel.filter((n) => n !== name)
        : [...sel, name];
      setPaneSelection(props.paneId, next);
    } else {
      setPaneSelection(props.paneId, [name]);
    }
  };

  const onRowDouble = (name: string, isDir: boolean) => {
    if (isDir) setPanePath(props.paneId, joinPath(pane().path, name));
  };

  return (
    <div class="pane">
      <div class="pane-toolbar">
        <button
          title="親フォルダへ"
          onClick={() => setPanePath(props.paneId, parentPath(pane().path))}
        >
          ↑
        </button>
        <input
          class="pane-path"
          value={pane().path}
          onChange={(e) => setPanePath(props.paneId, e.currentTarget.value)}
        />
        <select
          class="link-select"
          value={pane().linkGroupId ?? ""}
          onChange={(e) =>
            setPaneLinkGroup(props.paneId, e.currentTarget.value || null)
          }
          title="連動グループ"
        >
          <option value="">連動なし</option>
          <For each={state.linkGroups}>
            {(g) => <option value={g.id}>{g.name}</option>}
          </For>
        </select>
        <Show when={pane().linkGroupId}>
          <span
            class="link-badge"
            style={{
              background: state.linkGroups.find(
                (g) => g.id === pane().linkGroupId,
              )?.color,
            }}
          />
        </Show>
        <button
          title="水平分割"
          onClick={() => splitPane(props.tabId, props.paneId, "h")}
        >
          ⬌
        </button>
        <button
          title="垂直分割"
          onClick={() => splitPane(props.tabId, props.paneId, "v")}
        >
          ⬍
        </button>
        <button
          title="ペインを閉じる"
          onClick={() => closePane(props.tabId, props.paneId)}
        >
          ✕
        </button>
      </div>
      <div
        class="file-list"
        onScroll={(e) => setPaneScroll(props.paneId, e.currentTarget.scrollTop)}
        ref={(el) => {
          queueMicrotask(() => {
            if (el && Math.abs(el.scrollTop - pane().scrollTop) > 1) {
              el.scrollTop = pane().scrollTop;
            }
          });
        }}
      >
        <table>
          <thead>
            <tr>
              <th style={{ width: "55%" }}>名前</th>
              <th style={{ width: "15%" }}>サイズ</th>
              <th style={{ width: "20%" }}>更新日</th>
              <th style={{ width: "10%" }}>種別</th>
            </tr>
          </thead>
          <tbody>
            <For each={entries()}>
              {(e) => (
                <tr
                  classList={{ selected: pane().selection.includes(e.name) }}
                  onClick={(ev) => onRowClick(e.name, ev)}
                  onDblClick={() => onRowDouble(e.name, e.kind === "dir")}
                >
                  <td>
                    <span class="icon">{e.kind === "dir" ? "📁" : "📄"}</span>
                    {e.name}
                  </td>
                  <td>{e.kind === "dir" ? "" : formatSize(e.size)}</td>
                  <td>{e.modified}</td>
                  <td>{e.ext ?? (e.kind === "dir" ? "<DIR>" : "")}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
        <Show when={entries().length === 0}>
          <div class="empty">空のフォルダ</div>
        </Show>
      </div>
      <div class="pane-status">
        {entries().length} 項目 ／ 選択 {pane().selection.length}
      </div>
    </div>
  );
}

function formatSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
