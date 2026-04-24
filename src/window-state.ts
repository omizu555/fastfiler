// v1.9: ウインドウ位置 / サイズの保存復元
//
// Tauri 2 の WebviewWindow から innerPosition / innerSize を取得して
// localStorage に保存し、次回起動時にそれを適用する。
//
// 保存タイミング: App.onCloseRequested ハンドラ内 (× ボタン押下時)
// 復元タイミング: main.tsx 起動直後 (UI 描画前にできる限り早く)

const KEY = "fastfiler:window:v1";

export interface SavedWindow {
  x: number;
  y: number;
  width: number;
  height: number;
  maximized?: boolean;
}

export function loadSavedWindow(): SavedWindow | null {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return null;
    const v = JSON.parse(raw) as SavedWindow;
    if (
      typeof v.x === "number" &&
      typeof v.y === "number" &&
      typeof v.width === "number" &&
      typeof v.height === "number"
    ) {
      return v;
    }
    return null;
  } catch {
    return null;
  }
}

function save(v: SavedWindow): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(v));
  } catch {/* ignore */}
}

/** 起動時に呼ぶ。前回保存があれば適用する。 */
export async function applySavedWindow(): Promise<void> {
  const v = loadSavedWindow();
  console.info("[window-state] applySavedWindow", v);
  if (!v) return;
  try {
    const winMod = await import("@tauri-apps/api/window");
    const dpiMod = await import("@tauri-apps/api/dpi");
    const w = winMod.getCurrentWindow();
    if (v.maximized) {
      await w.maximize();
      console.info("[window-state] maximized restored");
      return;
    }
    await w.setPosition(new dpiMod.PhysicalPosition(v.x, v.y));
    await w.setSize(new dpiMod.PhysicalSize(v.width, v.height));
    console.info("[window-state] position/size restored");
  } catch (err) {
    console.warn("[window-state] applySavedWindow failed", err);
  }
}

/** 終了直前に呼ぶ。現在のウインドウ位置 / サイズを保存する。 */
export async function captureAndSaveWindow(): Promise<void> {
  try {
    const winMod = await import("@tauri-apps/api/window");
    const w = winMod.getCurrentWindow();
    const maximized = await w.isMaximized();
    if (maximized) {
      // 最大化中はサイズ/位置は前回値を保持しつつ maximized フラグだけ立てる
      const prev = loadSavedWindow();
      const v: SavedWindow = {
        x: prev?.x ?? 100,
        y: prev?.y ?? 100,
        width: prev?.width ?? 1200,
        height: prev?.height ?? 800,
        maximized: true,
      };
      save(v);
      console.info("[window-state] saved (maximized)", v);
      return;
    }
    const pos = await w.innerPosition();
    const size = await w.innerSize();
    const v: SavedWindow = {
      x: pos.x,
      y: pos.y,
      width: size.width,
      height: size.height,
      maximized: false,
    };
    save(v);
    console.info("[window-state] saved", v);
  } catch (err) {
    console.warn("[window-state] captureAndSaveWindow failed", err);
  }
}
