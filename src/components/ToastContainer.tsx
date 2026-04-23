import { For, Show } from "solid-js";
import { state, dismissToast } from "../store";

export default function ToastContainer() {
  // v1.6 (16.3): statusbar 表示モードでは StatusBarToast が描画する
  return (
    <Show when={state.toastPosition !== "statusbar"}>
      <div class="toast-container">
        <For each={state.toasts}>
          {(t) => (
            <div class={`toast toast-${t.level}`}>
              <span class="toast-msg">{t.message}</span>
              {t.action ? (
                <button
                  class="toast-action"
                  onClick={() => { t.action!.onClick(); dismissToast(t.id); }}
                >{t.action.label}</button>
              ) : null}
              <button class="toast-close" title="閉じる" onClick={() => dismissToast(t.id)}>×</button>
            </div>
          )}
        </For>
      </div>
    </Show>
  );
}
