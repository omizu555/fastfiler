import { JSX, Show } from "solid-js";
import type { PanelId } from "../types";
import { startPanelDrag, endPanelDrag } from "../dock";
import { setPanelSlot, togglePanelVisible, state } from "../store";

interface Props {
  panel: PanelId;
  title: JSX.Element;
  right?: JSX.Element;
}

export default function PanelHeader(props: Props) {
  let downX = 0, downY = 0, dragging = false;

  const onPointerDown = (e: PointerEvent) => {
    if (e.button !== 0) return;
    // ボタンや select 等の操作要素上ではドラッグを開始しない (click を妨げない)
    const tgt = e.target as HTMLElement;
    if (tgt.closest("button, select, input, a, .panel-header-dockmenu")) return;
    downX = e.clientX;
    downY = e.clientY;
    dragging = false;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    const onMove = (ev: PointerEvent) => {
      if (!dragging && (Math.abs(ev.clientX - downX) > 6 || Math.abs(ev.clientY - downY) > 6)) {
        dragging = true;
        startPanelDrag(props.panel);
      }
      if (dragging) {
        // hover 判定は DockOverlay 側の onPointerEnter/Leave に任せる
        // ここでは何もしない (DockOverlay が drag 中だけ表示される)
      }
    };
    const onUp = (_ev: PointerEvent) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      if (dragging) endPanelDrag(true);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
  };

  const cur = () => state.workspace.panelDock?.[props.panel];

  return (
    <div class="panel-header" onPointerDown={onPointerDown} title="ドラッグで移動">
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
