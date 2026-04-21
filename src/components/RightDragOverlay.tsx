import { Show } from "solid-js";
import {
  rightDragActive,
  rightDragMenu,
  closeRightDragMenu,
  executeRightDrag,
} from "../file-list/right-drag";
import ContextMenu from "./ContextMenu";

export default function RightDragOverlay() {
  return (
    <>
      <Show when={rightDragActive()}>
        {(d) => (
          <div
            class="rdrag-ghost"
            style={{
              left: `${d().x + 12}px`,
              top: `${d().y + 12}px`,
            }}
          >
            {d().payload.label}
          </div>
        )}
      </Show>
      <Show when={rightDragMenu()}>
        {(m) => (
          <ContextMenu
            x={m().x}
            y={m().y}
            items={[
              { icon: "📁", label: `ここに移動 (${m().destPath})`, onClick: () => { void executeRightDrag("move"); } },
              { icon: "📄", label: "ここにコピー", onClick: () => { void executeRightDrag("copy"); } },
              { separator: true },
              { icon: "✖", label: "キャンセル", onClick: () => closeRightDragMenu() },
            ]}
            onClose={closeRightDragMenu}
          />
        )}
      </Show>
    </>
  );
}
