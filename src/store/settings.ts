import { batch } from "solid-js";
import type { HotkeyAction, IconSet, LinkChannel, ThemeMode } from "../types";
import { defaultHotkeys } from "../hotkeys";
import { state, setState, persist, loaded } from "./core";

export function setInitialPath(path: string) {
  if (!loaded) {
    const id = state.tabs[0].id;
    const paneId = (state.tabs[0].rootPane as { kind: "leaf"; paneId: string }).paneId;
    batch(() => {
      setState("panes", paneId, "path", path);
      setState("tabs", (t) => t.id === id, "title", path);
    });
  }
}

export function toggleHidden() { setState("showHidden", (v) => !v); persist(); }
export function setShowHidden(v: boolean) { setState("showHidden", v); persist(); }

export function setShowThumbnails(v: boolean) { setState("showThumbnails", v); persist(); }
export function setShowPreview(v: boolean) { setState("showPreview", v); persist(); }
export function togglePreview() { setState("showPreview", (v) => !v); persist(); }
export function togglePluginPanel() { setState("showPluginPanel", (v) => !v); persist(); }
export function setHidePaneToolbar(v: boolean) { setState("hidePaneToolbar", v); persist(); }

export function setLinkGroupChannel(groupId: string, channel: LinkChannel, enabled: boolean) {
  setState("linkGroups", (g) => g.id === groupId, "channels", channel, enabled);
  persist();
}

export function setHotkey(action: HotkeyAction, combo: string) {
  setState("hotkeys", action, combo);
  persist();
}

export function resetHotkeys() {
  setState("hotkeys", { ...defaultHotkeys });
  persist();
}

export function setSearchBackend(b: "builtin" | "everything") {
  setState("searchBackend", b);
  persist();
}
export function setEverythingPort(p: number) {
  setState("everythingPort", Math.max(1, Math.min(65535, Math.floor(p))));
  persist();
}
export function setEverythingScope(v: boolean) {
  setState("everythingScope", v);
  persist();
}

export function setTheme(t: ThemeMode) { setState("theme", t); persist(); }
export function setAccentColor(c: string | null) { setState("accentColor", c); persist(); }
export function setIconSet(s: IconSet) { setState("iconSet", s); persist(); }

export function setFileListColWidth(col: "name" | "size" | "mtime" | "kind", percent: number) {
  // 各列の最小%は 5、最大は 90 (他列が押し潰されすぎないよう)
  const v = Math.max(5, Math.min(90, percent));
  setState("fileListColWidths", col, v);
  persist();
}
