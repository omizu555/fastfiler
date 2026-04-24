// 外部 (エクスプローラ等) からのドロップを受信する 1 経路統合モジュール
//
// 受信ソース:
//   - Rust 自前 IDropTarget: メイン HWND (WebView2 領域外) → "ole-drop" イベント
//   - Tauri 標準 (WebView2 onDragDropEvent): WebView2 領域 → "drop" タイプ
//   どちらも paths 配列とドロップ座標を返す。共通ハンドラ resolveAndDrop に集約。
//
// 修飾キー (Ctrl) は drop イベント payload に含まれないため、別途 keydown/keyup を
// listen して直近の値を保持する。

import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { state, focusedLeafPaneId } from "../store";
import { hitTest } from "./hit-test";
import { decideOp } from "./policy";
import { performDrop } from "./perform";
import { setExtDragOver, clearExtDragOver } from "./ui-state";
import { parentPath } from "../path-util";

// Rust 側 ScreenToClient は物理ピクセルを返すため、CSS ピクセルへ変換
function toCssCoords(x: number, y: number): { cx: number; cy: number } {
  const dpr = window.devicePixelRatio || 1;
  return { cx: x / dpr, cy: y / dpr };
}

let ctrlDown = false;
function onModKey(e: KeyboardEvent): void {
  ctrlDown = e.ctrlKey;
}

interface DropEnvelope {
  paths: string[];
  x: number;
  y: number;
  logTag: string;
}

async function handleDropEnvelope(env: DropEnvelope): Promise<void> {
  clearExtDragOver();
  if (!env.paths.length) {
    console.warn(`${env.logTag} drop: empty paths`);
    return;
  }
  const { cx, cy } = toCssCoords(env.x, env.y);
  const hit = hitTest(cx, cy);
  let destPath = hit.destPath;
  let targetPaneId = hit.paneId;
  if (!destPath) {
    const pid = targetPaneId ?? focusedLeafPaneId();
    if (pid) {
      destPath = state.panes[pid]?.path ?? null;
      targetPaneId = pid;
    }
  }
  if (!destPath) {
    console.warn(`${env.logTag} drop: no target path resolved`);
    return;
  }
  const op = decideOp({
    ctrlKey: ctrlDown,
    isInternal: false,
    srcPaths: env.paths,
    dstPath: destPath,
  });
  // sourceDir は外部 D&D ではアプリ管理外だが、move 時は src の親を refresh して
  // エクスプローラ側がたまたま同じ場所を開いていても整合させる
  const sourceDir = op === "move" && env.paths[0] ? parentPath(env.paths[0]) : undefined;
  await performDrop({
    paths: env.paths,
    destPath,
    op,
    sourceDir,
    targetPaneId,
    logTag: env.logTag,
  });
}

function updateHover(x: number, y: number): void {
  const { cx, cy } = toCssCoords(x, y);
  const hit = hitTest(cx, cy);
  if (hit.paneId) setExtDragOver(hit.paneId, hit.folderName);
  else clearExtDragOver();
}

export async function installExternalDropListeners(): Promise<() => void> {
  const cleanups: Array<() => void> = [];
  window.addEventListener("keydown", onModKey, true);
  window.addEventListener("keyup", onModKey, true);
  cleanups.push(() => {
    window.removeEventListener("keydown", onModKey, true);
    window.removeEventListener("keyup", onModKey, true);
  });

  // Rust 自前 OLE 経路 (メイン HWND)
  try {
    const unOver = await listen<{ effect: number; x: number; y: number }>(
      "ole-drag-over",
      (e) => updateHover(e.payload.x, e.payload.y),
    );
    cleanups.push(unOver);
    const unLeave = await listen("ole-drag-leave", () => clearExtDragOver());
    cleanups.push(unLeave);
    const unDrop = await listen<{ paths: string[]; effect: number; x: number; y: number }>(
      "ole-drop",
      (e) => {
        void handleDropEnvelope({
          paths: e.payload.paths,
          x: e.payload.x,
          y: e.payload.y,
          logTag: "[ole-drop]",
        });
      },
    );
    cleanups.push(unDrop);
  } catch (err) {
    console.warn("[external-listen] OLE listen failed:", err);
  }

  // Tauri 標準 (WebView2 領域)
  try {
    const wv = getCurrentWebview();
    const unWv = await wv.onDragDropEvent((ev) => {
      const payload = ev.payload as {
        type: "enter" | "over" | "drop" | "leave";
        paths?: string[];
        position?: { x: number; y: number };
      };
      if (payload.type === "leave") {
        clearExtDragOver();
        return;
      }
      const pos = payload.position;
      if (!pos) return;
      if (payload.type === "enter" || payload.type === "over") {
        updateHover(pos.x, pos.y);
        return;
      }
      // drop
      void handleDropEnvelope({
        paths: payload.paths ?? [],
        x: pos.x,
        y: pos.y,
        logTag: "[wv-drop]",
      });
    });
    cleanups.push(unWv);
  } catch (err) {
    console.warn("[external-listen] WebView2 D&D init failed:", err);
  }

  return () => {
    for (const fn of cleanups) {
      try {
        fn();
      } catch {
        /* ignore */
      }
    }
  };
}
