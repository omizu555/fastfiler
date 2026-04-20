import { For } from "solid-js";
import { state, dismissToast } from "../store";

export default function ToastContainer() {
  return (
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
  );
}
