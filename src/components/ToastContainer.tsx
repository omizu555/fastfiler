import { For } from "solid-js";
import { state } from "../store";

export default function ToastContainer() {
  return (
    <div class="toast-container">
      <For each={state.toasts}>
        {(t) => <div class={`toast toast-${t.level}`}>{t.message}</div>}
      </For>
    </div>
  );
}
