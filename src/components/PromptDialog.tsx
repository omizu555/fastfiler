import { Show, createEffect, createSignal } from "solid-js";

interface PromptOptions {
  title: string;
  label?: string;
  initial?: string;
  placeholder?: string;
  confirmLabel?: string;
  /** 入力値の検証。エラーメッセージを返すと OK ボタンが無効化 + メッセージ表示 */
  validate?: (v: string) => string | null;
  /** 既定で全選択 (true) か、末尾にカーソル (false)。新規フォルダなどでは true */
  selectAll?: boolean;
}

interface PromptState extends PromptOptions {
  resolve: (v: string | null) => void;
}

const [current, setCurrent] = createSignal<PromptState | null>(null);

export function openPrompt(opts: PromptOptions): Promise<string | null> {
  const prev = current();
  if (prev) prev.resolve(null);
  return new Promise((resolve) => setCurrent({ ...opts, resolve }));
}

export default function PromptDialog() {
  const [value, setValue] = createSignal("");
  let inputRef: HTMLInputElement | undefined;

  createEffect(() => {
    const c = current();
    if (!c) return;
    setValue(c.initial ?? "");
    queueMicrotask(() => {
      if (!inputRef) return;
      inputRef.focus();
      if (c.selectAll !== false) inputRef.select();
      else inputRef.setSelectionRange(inputRef.value.length, inputRef.value.length);
    });
  });

  const close = (result: string | null) => {
    const c = current();
    if (!c) return;
    setCurrent(null);
    c.resolve(result);
  };

  const error = () => {
    const c = current();
    if (!c?.validate) return null;
    return c.validate(value());
  };

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      if (!error() && value().length > 0) close(value());
    } else if (e.key === "Escape") {
      e.preventDefault();
      close(null);
    }
  };

  return (
    <Show when={current()}>
      {(c) => (
        <div class="modal-backdrop" onMouseDown={(e) => { if (e.target === e.currentTarget) close(null); }}>
          <div class="modal prompt-modal" onMouseDown={(e) => e.stopPropagation()}>
            <div class="modal-head"><strong>{c().title}</strong></div>
            <div class="modal-body">
              <Show when={c().label}>
                <label class="prompt-label">{c().label}</label>
              </Show>
              <input
                ref={inputRef}
                type="text"
                class="prompt-input"
                value={value()}
                placeholder={c().placeholder ?? ""}
                onInput={(e) => setValue(e.currentTarget.value)}
                onKeyDown={onKey}
              />
              <Show when={error()}>
                <div class="prompt-error">{error()}</div>
              </Show>
            </div>
            <div class="modal-foot">
              <span class="spacer" />
              <button onClick={() => close(null)}>キャンセル</button>
              <button
                class="primary"
                disabled={!!error() || value().length === 0}
                onClick={() => close(value())}
              >
                {c().confirmLabel ?? "OK"}
              </button>
            </div>
          </div>
        </div>
      )}
    </Show>
  );
}
