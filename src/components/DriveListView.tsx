import { For, createResource } from "solid-js";
import { listDrives, DRIVES_PATH } from "../fs";
import { setPanePath, setFocusedPane, state } from "../store";
import { driveIcon, driveTitle } from "../drive-util";
import type { DriveInfo } from "../types";

interface Props {
  paneId: string;
  tabId: string;
}

export default function DriveListView(props: Props) {
  const [drives] = createResource<DriveInfo[]>(async () => {
    try { return await listDrives(); } catch { return []; }
  });

  const open = (letter: string) => {
    setPanePath(props.paneId, letter.endsWith("\\") ? letter : letter + "\\");
  };

  return (
    <div
      class="pane drives-pane"
      classList={{ "pane-focused": state.focusedPaneId === props.paneId }}
      tabIndex={0}
      onPointerDown={() => setFocusedPane(props.paneId)}
      onFocusIn={() => setFocusedPane(props.paneId)}
    >
      <div class="pane-toolbar">
        <button title="これ以上は戻れません" disabled>↑</button>
        <div class="breadcrumbs">
          <span class="crumb crumb-active" title={DRIVES_PATH}>💻 PC (ドライブ一覧)</span>
        </div>
        <span class="spacer" />
      </div>
      <div class="drives-grid">
        <For each={drives() ?? []}>
          {(d) => (
            <button
              class="drive-card"
              classList={{ [`drive-kind-${d.kind}`]: true }}
              onClick={() => open(d.letter)}
              title={driveTitle(d)}
            >
              <div class="drive-icon">{driveIcon(d.kind)}</div>
              <div class="drive-letter">{d.letter}</div>
              <div class="drive-label">{d.label || (d.kind === "network" && d.remotePath) || ""}</div>
            </button>
          )}
        </For>
      </div>
    </div>
  );
}
