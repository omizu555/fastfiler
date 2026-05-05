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
  // サブメニューは fixed 配置 + 計算後に表示。初期は不可視で画面左上に置き測定する。
  const [subStyle, setSubStyle] = createSignal<Record<string, string>>({
    visibility: "hidden",
    left: "0px",
    top: "0px",
  });

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
      const vw = window.innerWidth;
      const vh = window.innerHeight;
      const w = rect.width;
      const h = rect.height;
      let left = props.x;
      let top = props.y;
      // 横: 右にはみ出すならカーソル左側へフリップ
      if (left + w + 4 > vw) {
        left = Math.max(4, props.x - w);
      }
      // 縦: 下にはみ出すならカーソル上側へフリップ。両側とも入らなければ
      // 上端 4px に貼り付けて縦スクロールを許可
      if (top + h + 4 > vh) {
        if (props.y - h >= 4) {
          top = props.y - h;
        } else {
          top = 4;
          ref.style.maxHeight = `${vh - 8}px`;
          ref.style.overflowY = "auto";
        }
      }
      ref.style.left = `${left}px`;
      ref.style.top = `${top}px`;
    });
  });
  onCleanup(() => {
    document.removeEventListener("mousedown", onDocClick, true);
    document.removeEventListener("keydown", onKey);
  });

  // サブメニューが画面外にはみ出る場合は上方向 / 左方向にフリップ。
  // position: fixed で配置するため、ピクセル絶対座標を直接計算する。
  createEffect(() => {
    const idx = openIndex();
    if (idx === null) return;
    // 開くたびに不可視状態にリセットしてから測定 (前回値の表示で一瞬ずれるのを防ぐ)
    setSubStyle({ visibility: "hidden", left: "0px", top: "0px" });
    queueMicrotask(() => {
      if (!subRef) return;
      const trigger = subRef.parentElement as HTMLElement | null;
      if (!trigger) return;
      const tRect = trigger.getBoundingClientRect();
      const sRect = subRef.getBoundingClientRect();
      const vw = window.innerWidth;
      const vh = window.innerHeight;
      const style: Record<string, string> = { visibility: "visible" };

      // 横: 右にはみ出すなら左側へ。両側ダメなら右端クランプ。
      let left: number;
      if (tRect.right + sRect.width + 4 <= vw) {
        left = tRect.right;
      } else if (tRect.left - sRect.width - 4 >= 0) {
        left = tRect.left - sRect.width;
      } else {
        left = Math.max(4, vw - sRect.width - 4);
      }

      // 縦: 下にはみ出すなら上方向フリップ。両方ダメなら上端クランプ + スクロール。
      let top: number;
      if (tRect.top - 4 + sRect.height <= vh - 4) {
        top = tRect.top - 4;
      } else if (tRect.bottom - sRect.height >= 4) {
        top = tRect.bottom - sRect.height + 4;
      } else {
        top = 4;
        style.maxHeight = `${vh - 8}px`;
        style.overflowY = "auto";
      }

      style.left = `${left}px`;
      style.top = `${top}px`;
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
