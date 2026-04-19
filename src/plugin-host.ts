// v2.0: プラグイン postMessage ブリッジ + イベント broadcast + コンテキストメニュー登録
//
// PluginPanel から切り出し。アクティブな iframe を 1 つ管理し、enable 中のプラグインは
// `pane.changed` / `pane.selection.changed` 等のイベントを受信できる。
//
// メッセージ形式 (アプリ ↔ プラグイン):
//   invoke:   { __ff:"invoke",  id, capability, args }
//   result:   { __ff:"result",  id, ok, result?, error? }
//   event:    { __ff:"event",   topic, payload }

import { createEffect } from "solid-js";
import { pluginInvoke } from "./fs";
import {
  state,
  setPanePath,
  activeLeafPaneId,
  isPluginEnabled,
  registerPluginContextMenuItem,
  unregisterPluginContextMenuItems,
  pushToast,
} from "./store";
import type { PluginContextMenuItem, Toast } from "./types";

interface PluginEntry {
  pluginId: string;
  frame: HTMLIFrameElement;
}

const entries = new Map<string, PluginEntry>();
let activePluginId: string | null = null;

export function setActivePlugin(pluginId: string | null, frame: HTMLIFrameElement | null) {
  if (activePluginId && activePluginId !== pluginId) {
    unregisterPluginContextMenuItems(activePluginId);
    entries.delete(activePluginId);
  }
  activePluginId = pluginId;
  if (pluginId && frame) {
    entries.set(pluginId, { pluginId, frame });
  }
}

// iframe ロード完了後に初期イベントを送る (script は load 前に走るので少し遅延)
export function notifyPluginActivated(pluginId: string) {
  queueMicrotask(() => {
    sendEvent(pluginId, "plugin.activated", {});
    const pid = activeLeafPaneId();
    if (pid) {
      const pane = state.panes[pid];
      if (pane) {
        sendEvent(pluginId, "pane.changed", { paneId: pid, path: pane.path });
        sendEvent(pluginId, "pane.selection.changed", { paneId: pid, selection: [...pane.selection] });
      }
    }
  });
}

function postTo(entry: PluginEntry, msg: unknown) {
  try {
    entry.frame.contentWindow?.postMessage(msg, "*");
  } catch {/* ignore */}
}

function sendEvent(pluginId: string, topic: string, payload: unknown) {
  const e = entries.get(pluginId);
  if (!e) return;
  if (!isPluginEnabled(pluginId)) return;
  postTo(e, { __ff: "event", topic, payload });
}

function broadcastEvent(topic: string, payload: unknown) {
  for (const [id] of entries) sendEvent(id, topic, payload);
}

// ---- frontend-handled capabilities ----
async function handleFrontendCap(
  pluginId: string,
  capability: string,
  args: Record<string, unknown>,
): Promise<unknown> {
  switch (capability) {
    case "ui.notify": {
      const message = String(args.message ?? "");
      const level = (args.level as Toast["level"]) ?? "info";
      pushToast(`[${pluginId}] ${message}`, level);
      return null;
    }
    case "pane.getActive": {
      const pid = activeLeafPaneId();
      if (!pid) return null;
      const p = state.panes[pid];
      if (!p) return null;
      // Solid のストアから直接返すと proxy のため structured clone に失敗する
      return { paneId: pid, path: p.path, selection: [...p.selection] };
    }
    case "pane.setPath": {
      const path = String(args.path ?? "");
      const paneId = (args.paneId as string | undefined) ?? activeLeafPaneId() ?? "";
      if (!paneId || !path) throw new Error("invalid pane.setPath args");
      setPanePath(paneId, path);
      return null;
    }
    case "ui.contextMenu.register": {
      const item: PluginContextMenuItem = {
        pluginId,
        id: String(args.id ?? ""),
        label: String(args.label ?? ""),
        icon: args.icon as string | undefined,
        when: (args.when as PluginContextMenuItem["when"]) ?? "any",
        extensions: Array.isArray(args.extensions)
          ? (args.extensions as unknown[]).map((x) => String(x).toLowerCase())
          : undefined,
      };
      if (!item.id || !item.label) throw new Error("id/label required");
      registerPluginContextMenuItem(item);
      return null;
    }
  }
  return undefined; // 未処理 → Rust へ
}

// PluginPanel から呼ばれる: window 全体の message を 1 度だけ購読
let installed = false;
export function installPluginHost() {
  if (installed) return;
  installed = true;
  window.addEventListener("message", async (ev: MessageEvent) => {
    const data = ev.data;
    if (!data || typeof data !== "object") return;
    const { __ff, id, capability, args } = data as {
      __ff?: string; id?: number; capability?: string; args?: Record<string, unknown>;
    };
    if (__ff !== "invoke" || !capability) return;
    const cur = activePluginId;
    const entry = cur ? entries.get(cur) : null;
    if (!cur || !entry || ev.source !== entry.frame.contentWindow) return;
    const pluginId = cur;
    if (!isPluginEnabled(pluginId)) {
      (ev.source as Window | null)?.postMessage(
        { __ff: "result", id, ok: false, error: "plugin disabled" },
        "*",
      );
      return;
    }
    try {
      const front = await handleFrontendCap(pluginId, capability, args ?? {});
      const result = front === undefined
        ? await pluginInvoke(pluginId, capability, args ?? {})
        : front;
      (ev.source as Window | null)?.postMessage(
        { __ff: "result", id, ok: true, result },
        "*",
      );
    } catch (e) {
      (ev.source as Window | null)?.postMessage(
        { __ff: "result", id, ok: false, error: String(e) },
        "*",
      );
    }
  });

  // pane イベント購読 → broadcast
  let lastPath: string | null = null;
  let lastSel: string | null = null;
  let lastPaneId: string | null = null;
  createEffect(() => {
    const pid = activeLeafPaneId();
    if (!pid) return;
    const pane = state.panes[pid];
    if (!pane) return;
    if (pid !== lastPaneId || pane.path !== lastPath) {
      lastPaneId = pid;
      lastPath = pane.path;
      broadcastEvent("pane.changed", { paneId: pid, path: pane.path });
    }
    const selKey = pane.selection.join("\u0001");
    if (selKey !== lastSel) {
      lastSel = selKey;
      broadcastEvent("pane.selection.changed", { paneId: pid, selection: [...pane.selection] });
    }
  });
}

// ContextMenu からプラグイン項目クリック時に呼ぶ
export function invokePluginContextMenuItem(
  item: PluginContextMenuItem,
  target: { path: string; isDir: boolean; name: string },
) {
  sendEvent(item.pluginId, "plugin.contextMenu.invoked", { itemId: item.id, target });
}
