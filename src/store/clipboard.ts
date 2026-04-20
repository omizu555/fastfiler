import { setState } from "./core";

export function setClipboard(paths: string[], op: "copy" | "cut") {
  setState("clipboard", paths.length ? { paths, op } : null);
}

export function clearClipboard() {
  setState("clipboard", null);
}
