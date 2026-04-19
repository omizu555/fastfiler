import { JSX, Show } from "solid-js";
import type { PanelId } from "../types";
import { setPanelSlot, togglePanelVisible, state } from "../store";

interface Props {
  panel: PanelId;
  title: JSX.Element;
  right?: JSX.Element;
}

export default function PanelHeader(props: Props) {
  const cur = () => state.workspace.panelDock?.[props.panel];

  return (
    <div class="panel-header">
      <span class="panel-header-title">{props.title}</span>
      <span class="spacer" />
      {props.right}
      <div class="panel-header-dockmenu">
        <button title="左にドック" classList={{ active: cur()?.slot === "left" }} onClick={() => setPanelSlot(props.panel, "left")}>◧</button>
        <button title="右にドック" classList={{ active: cur()?.slot === "right" }} onClick={() => setPanelSlot(props.panel, "right")}>◨</button>
        <button title="上にドック" classList={{ active: cur()?.slot === "top" }} onClick={() => setPanelSlot(props.panel, "top")}>⬒</button>
        <button title="下にドック" classList={{ active: cur()?.slot === "bottom" }} onClick={() => setPanelSlot(props.panel, "bottom")}>⬓</button>
        <Show when={cur()?.slot !== "hidden"}>
          <button title="閉じる" onClick={() => togglePanelVisible(props.panel)}>✕</button>
        </Show>
      </div>
    </div>
  );
}

