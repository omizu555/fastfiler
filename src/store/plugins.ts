import type { PluginContextMenuItem } from "../types";
import { state, setState, persist } from "./core";

export function setPluginEnabled(pluginId: string, enabled: boolean) {
  setState("plugins", "enabled", pluginId, enabled);
  if (!enabled) {
    setState("pluginContextMenu", (xs) => xs.filter((x) => x.pluginId !== pluginId));
  }
  persist();
}

export function isPluginEnabled(pluginId: string): boolean {
  return !!state.plugins.enabled[pluginId];
}

export function setPluginPanelWidth(w: number) {
  setState("pluginPanelWidth", Math.max(220, Math.min(900, Math.round(w))));
  persist();
}

export function registerPluginContextMenuItem(item: PluginContextMenuItem) {
  setState("pluginContextMenu", (xs) => {
    const filtered = xs.filter((x) => !(x.pluginId === item.pluginId && x.id === item.id));
    return [...filtered, item];
  });
}

export function unregisterPluginContextMenuItems(pluginId: string) {
  setState("pluginContextMenu", (xs) => xs.filter((x) => x.pluginId !== pluginId));
}
