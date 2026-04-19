import { setPanelWindow, state, WINDOW_ID } from "./store";
import type { PanelId } from "./types";

/** フロートウィンドウラベルの生成 (Tauri ラベルは英数字のみ可) */
function newFloatLabel(): string {
  return `float-${Date.now().toString(36)}`;
}

/** パネルを別ウィンドウに切り出す */
export async function popOutPanel(panel: PanelId): Promise<void> {
  if (WINDOW_ID !== "main") {
    // フロート → メインに戻す
    setPanelWindow(panel, "main");
    return;
  }
  const label = newFloatLabel();
  const geom = state.workspace.panelDock?.[panel].floatGeom;
  try {
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    const opts: Record<string, unknown> = {
      url: `index.html?win=${label}`,
      title: panel === "tabs" ? "FastFiler — タブ" : "FastFiler — ツリー",
      width: geom?.w ?? 320,
      height: geom?.h ?? 600,
      minWidth: 200,
      minHeight: 200,
      decorations: true,
      resizable: true,
    };
    if (geom) {
      opts.x = geom.x;
      opts.y = geom.y;
    }
    const win = new WebviewWindow(label, opts);
    win.once("tauri://created", () => {
      setPanelWindow(panel, label);
    });
    win.once("tauri://error", (e) => {
      console.error("[float] create error", e);
      alert("フロートウィンドウを開けませんでした");
    });
  } catch (e) {
    console.error("[float] WebviewWindow import 失敗", e);
    alert("フロート機能は Tauri 環境のみで利用可能です");
  }
}

/** 現在のフロートウィンドウを閉じてパネルを main に戻す */
export async function dockBackPanel(panel: PanelId): Promise<void> {
  setPanelWindow(panel, "main");
  if (WINDOW_ID !== "main") {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const w = getCurrentWindow();
      // 自ウィンドウ内に他パネルが残っていなければ閉じる
      const pd = state.workspace.panelDock;
      const stillHere = pd && (
        (pd.tabs.windowId ?? "main") === WINDOW_ID ||
        (pd.tree.windowId ?? "main") === WINDOW_ID
      );
      if (!stillHere) await w.close();
    } catch {/* ignore */}
  }
}
