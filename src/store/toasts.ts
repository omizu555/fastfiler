import type { Toast, ToastAction } from "../types";
import { setState } from "./core";

let toastSeq = 0;
export function pushToast(message: string, level: Toast["level"] = "info", action?: ToastAction, durationMs = 5000) {
  const id = ++toastSeq;
  setState("toasts", (xs) => [...xs, { id, message, level, action }]);
  window.setTimeout(() => {
    setState("toasts", (xs) => xs.filter((t) => t.id !== id));
  }, durationMs);
  return id;
}

export function dismissToast(id: number) {
  setState("toasts", (xs) => xs.filter((t) => t.id !== id));
}
