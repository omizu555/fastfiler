import { For, Show } from "solid-js";
import type { DockSlot } from "../types";
import { dragState, setHoverSlot } from "../dock";

const SLOTS: { slot: DockSlot; cls: string }[] = [
  { slot: "left", cls: "dock-zone-left" },
  { slot: "right", cls: "dock-zone-right" },
  { slot: "top", cls: "dock-zone-top" },
  { slot: "bottom", cls: "dock-zone-bottom" },
];

export default function DockOverlay() {
  return (
    <Show when={dragState() !== null}>
      <div class="dock-overlay">
        <For each={SLOTS}>
          {(z) => (
            <div
              class={`dock-zone ${z.cls}`}
              classList={{ hover: dragState()?.hoverSlot === z.slot }}
              onPointerEnter={() => setHoverSlot(z.slot)}
              onPointerLeave={() => {
                if (dragState()?.hoverSlot === z.slot) setHoverSlot(null);
              }}
            >
              <span class="dock-zone-label">{z.slot}</span>
            </div>
          )}
        </For>
      </div>
    </Show>
  );
}
