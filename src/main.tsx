/* @refresh reload */
import { render } from "solid-js/web";
import { createEffect } from "solid-js";
import App from "./App";
import { state, activeLeafPaneId, togglePaneSearchFocused } from "./store";
import { applySavedWindow } from "./window-state";
import "./styles.css";

// v1.7.1: WebView2 標準 Find ダイアログを抑止し Ctrl+F を必ずアプリ側で処理
window.addEventListener(
  "keydown",
  (e) => {
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "f") {
      e.preventDefault();
      e.stopImmediatePropagation();
      const pid = activeLeafPaneId();
      if (pid) togglePaneSearchFocused(pid);
    }
  },
  { capture: true },
);

// テーマ反映: state.theme と prefers-color-scheme を購読し data-theme を切替
const mql = window.matchMedia("(prefers-color-scheme: light)");

async function applyNativeTheme(effective: "light" | "dark", isSystem: boolean) {
  try {
    const mod = await import("@tauri-apps/api/window");
    const win = mod.getCurrentWindow();
    // system のときは null を渡し OS 既定に従わせる
    await win.setTheme(isSystem ? null : effective);
  } catch {
    // Tauri 環境外 (dev ブラウザ等) は無視
  }
}

createEffect(() => {
  const t = state.theme;
  const effective: "light" | "dark" =
    t === "system" ? (mql.matches ? "light" : "dark") : t;
  document.documentElement.dataset.theme = effective;
  void applyNativeTheme(effective, t === "system");
});

// v3.3: アクセントカラー反映
createEffect(() => {
  const c = state.accentColor;
  if (c) document.documentElement.style.setProperty("--accent", c);
  else document.documentElement.style.removeProperty("--accent");
});

// v3.3: アイコンセット (data 属性で CSS から参照可能に)
createEffect(() => {
  document.documentElement.dataset.iconset = state.iconSet;
});

// v1.11: テーマ プリセット (data-theme-preset で CSS 変数を上書き)
createEffect(() => {
  const p = state.themePreset;
  if (p && p !== "default") {
    document.documentElement.dataset.themePreset = p;
  } else {
    delete document.documentElement.dataset.themePreset;
  }
});
mql.addEventListener("change", () => {
  if (state.theme === "system") {
    const eff = mql.matches ? "light" : "dark";
    document.documentElement.dataset.theme = eff;
    void applyNativeTheme(eff, true);
  }
});

const root = document.getElementById("root");
if (!root) throw new Error("#root not found");
// v1.9: 前回ウインドウ位置/サイズを適用してから表示し render する
//       (visible:false で起動 → applySavedWindow 内で show() を呼ぶ)
void applySavedWindow();
render(() => <App />, root);


