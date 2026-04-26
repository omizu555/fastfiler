import { For, Show, createEffect, createSignal, onCleanup, onMount } from "solid-js";

export interface ContextMenuItem {
  label?: string;
  icon?: string;
  onClick?: () => void;
  disabled?: boolean;
  danger?: boolean;
  separator?: boolean;
  shortcut?: string;
  submenu?: ContextMenuItem[];
}

interface Props {
  x: number;
  y: number;
  items: ContextMenuItem[];
  onClose: () => void;
}

export default function ContextMenu(props: Props) {
  let ref: HTMLDivElement | undefined;
  let subRef: HTMLDivElement | undefined;
  const [openIndex, setOpenIndex] = createSignal<number | null>(null);
  const [subStyle, setSubStyle] = createSignal<Record<string, string>>({ top: "-4px", left: "100%" });

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

  // サブメニューが画面外にはみ出る場合は上方向 / 左方向にフリップ
  createEffect(() => {
    const idx = openIndex();
    if (idx === null) return;
    queueMicrotask(() => {
      if (!subRef) return;
      const trigger = subRef.parentElement as HTMLElement | null;
      if (!trigger) return;
      const tRect = trigger.getBoundingClientRect();
      const sRect = subRef.getBoundingClientRect();
      const vw = window.innerWidth;
      const vh = window.innerHeight;
      const style: Record<string, string> = {};
      // 横: 右にはみ出すなら左側へ
      if (tRect.right + sRect.width + 4 > vw) {
        style.left = "auto";
        style.right = "100%";
      } else {
        style.left = "100%";
        style.right = "auto";
      }
      // 縦: 下にはみ出すなら上揃え (下端を triggerの下端に合わせる)
      if (tRect.top + sRect.height > vh - 4) {
        // 親 .ctx-item からの相対で、サブメニューの bottom を trigger 高さと揃える
        style.top = "auto";
        style.bottom = "-4px";
      } else {
        style.top = "-4px";
        style.bottom = "auto";
      }
      setSubStyle(style);
    });
  });

  const handle = (it: ContextMenuItem) => {
    if (it.disabled || it.separator) return;
    if (it.submenu && it.submenu.length > 0) return;
    if (!it.onClick) return;
    it.onClick();
    props.onClose();
  };

  const handleSub = (it: ContextMenuItem) => {
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
        {(it, idx) => (
          <Show
            when={!it.separator}
            fallback={<div class="ctx-sep" />}
          >
            <div
              classList={{
                "ctx-item": true,
                disabled: !!it.disabled,
                danger: !!it.danger,
                "has-submenu": !!(it.submenu && it.submenu.length),
                "submenu-open": openIndex() === idx(),
              }}
              onClick={() => handle(it)}
              onMouseEnter={() => {
                setOpenIndex(it.submenu && it.submenu.length ? idx() : null);
              }}
            >
              <span class="ctx-icon">{it.icon ?? ""}</span>
              <span class="ctx-label">{it.label}</span>
              <Show
                when={it.submenu && it.submenu.length}
                fallback={<span class="ctx-shortcut">{it.shortcut ?? ""}</span>}
              >
                <span class="ctx-arrow">▸</span>
              </Show>
              <Show when={openIndex() === idx() && it.submenu && it.submenu.length}>
                <div class="ctx-submenu" ref={subRef} style={subStyle()}>
                  <For each={it.submenu}>
                    {(sub) => (
                      <Show
                        when={!sub.separator}
                        fallback={<div class="ctx-sep" />}
                      >
                        <div
                          classList={{ "ctx-item": true, disabled: !!sub.disabled, danger: !!sub.danger }}
                          onClick={(e) => { e.stopPropagation(); handleSub(sub); }}
                        >
                          <span class="ctx-icon">{sub.icon ?? ""}</span>
                          <span class="ctx-label">{sub.label}</span>
                          <span class="ctx-shortcut">{sub.shortcut ?? ""}</span>
                        </div>
                      </Show>
                    )}
                  </For>
                </div>
              </Show>
            </div>
          </Show>
        )}
      </For>
    </div>
  );
}
