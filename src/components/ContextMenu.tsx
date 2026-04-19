import { For, Show, onCleanup, onMount } from "solid-js";

export interface ContextMenuItem {
  label?: string;
  icon?: string;
  onClick?: () => void;
  disabled?: boolean;
  danger?: boolean;
  separator?: boolean;
  shortcut?: string;
}

interface Props {
  x: number;
  y: number;
  items: ContextMenuItem[];
  onClose: () => void;
}

export default function ContextMenu(props: Props) {
  let ref: HTMLDivElement | undefined;

  const onDocClick = (e: MouseEvent) => {
    if (!ref) return;
    if (!ref.contains(e.target as Node)) props.onClose();
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") props.onClose();
  };

  onMount(() => {
    document.addEventListener("mousedown", onDocClick, true);
    document.addEventListener("keydown", onKey);
    // 画面端でずらす
    queueMicrotask(() => {
      if (!ref) return;
      const rect = ref.getBoundingClientRect();
      const dx = Math.max(0, rect.right - window.innerWidth + 4);
      const dy = Math.max(0, rect.bottom - window.innerHeight + 4);
      if (dx || dy) {
        ref.style.left = `${props.x - dx}px`;
        ref.style.top = `${props.y - dy}px`;
      }
    });
  });
  onCleanup(() => {
    document.removeEventListener("mousedown", onDocClick, true);
    document.removeEventListener("keydown", onKey);
  });

  const handle = (it: ContextMenuItem) => {
    if (it.disabled || it.separator || !it.onClick) return;
    it.onClick();
    props.onClose();
  };

  return (
    <div
      ref={ref}
      class="ctx-menu"
      style={{ left: `${props.x}px`, top: `${props.y}px` }}
      onContextMenu={(e) => e.preventDefault()}
    >
      <For each={props.items}>
        {(it) => (
          <Show
            when={!it.separator}
            fallback={<div class="ctx-sep" />}
          >
            <div
              classList={{ "ctx-item": true, disabled: !!it.disabled, danger: !!it.danger }}
              onClick={() => handle(it)}
            >
              <span class="ctx-icon">{it.icon ?? ""}</span>
              <span class="ctx-label">{it.label}</span>
              <span class="ctx-shortcut">{it.shortcut ?? ""}</span>
            </div>
          </Show>
        )}
      </For>
    </div>
  );
}
