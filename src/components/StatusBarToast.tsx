import { Show } from "solid-js";
import { state, dismissToast } from "../store";

/**
 * v1.7: ステータスバーに直近 1 件のトーストを表示する。
 */
export default function StatusBarToast() {
  const last = () => state.toasts[state.toasts.length - 1];
  return (
    <Show when={last()}>
      {(t) => (
        <div class={`statusbar-toast statusbar-toast-${t().level}`}>
          <span class="statusbar-toast-msg" title={t().message}>{t().message}</span>
          {t().action ? (
            <button
              class="statusbar-toast-action"
              onClick={() => { t().action!.onClick(); dismissToast(t().id); }}
            >{t().action!.label}</button>
          ) : null}
          <button class="statusbar-toast-close" title="閉じる" onClick={() => dismissToast(t().id)}>×</button>
        </div>
      )}
    </Show>
  );
}
