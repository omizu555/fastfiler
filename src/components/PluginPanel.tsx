import { For, Show, createResource, createSignal, onMount } from "solid-js";
import { listPlugins, pluginsDirPath, revealInExplorer } from "../fs";
import { state, togglePluginPanel, isPluginEnabled, setPluginEnabled, setPluginPanelWidth } from "../store";
import { installPluginHost, setActivePlugin, notifyPluginActivated } from "../plugin-host";
import type { PluginInfo } from "../types";

export default function PluginPanel() {
  const [plugins, { refetch }] = createResource(() => listPlugins());
  const [active, setActive] = createSignal<PluginInfo | null>(null);
  const [dir, setDir] = createSignal("");
  let frameRef: HTMLIFrameElement | undefined;

  onMount(async () => {
    setDir(await pluginsDirPath());
    installPluginHost();
  });

  const activate = (p: PluginInfo) => {
    if (!isPluginEnabled(p.manifest.id)) return;
    setActive(p);
    queueMicrotask(async () => {
      if (!frameRef) return;
      let url: string;
      try {
        const m = await import("@tauri-apps/api/core");
        // convertFileSrc は path 全体を encodeURIComponent するので
        // バックスラッシュが %5C となり URL 上で単一セグメント化する
        // → iframe 内の相対参照 (`../sdk.js` 等) が壊れる
        // %5C を / に戻し、Windows でも有効な区切り文字にする
        url = m.convertFileSrc(p.entry_path).replace(/%5C/gi, "/");
      } catch {
        url = "file:///" + p.entry_path.replace(/\\/g, "/");
      }
      setActivePlugin(p.manifest.id, frameRef);
      const onLoad = () => {
        notifyPluginActivated(p.manifest.id);
        frameRef?.removeEventListener("load", onLoad);
      };
      frameRef.addEventListener("load", onLoad);
      frameRef.src = url;
    });
  };

  const toggleEnable = (p: PluginInfo, ev: Event) => {
    ev.stopPropagation();
    const cur = isPluginEnabled(p.manifest.id);
    setPluginEnabled(p.manifest.id, !cur);
    if (cur && active()?.manifest.id === p.manifest.id) {
      setActive(null);
      setActivePlugin(null, null);
      if (frameRef) frameRef.src = "about:blank";
    }
  };

  const onSplitterDown = (e: PointerEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = state.pluginPanelWidth;
    const move = (ev: PointerEvent) => {
      const dx = ev.clientX - startX;
      // パネルは右側にあるので、左へドラッグ(dx<0)で広く
      setPluginPanelWidth(startW - dx);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <aside class="plugin-panel" style={{ width: state.pluginPanelWidth + "px" }}>
      <div class="plugin-splitter" onPointerDown={onSplitterDown} title="ドラッグで幅変更" />
      <header class="plugin-head">
        <strong>プラグイン</strong>
        <span class="spacer" />
        <small style={{ "font-size": "10px", opacity: "0.7" }}>ctx={state.pluginContextMenu.length}</small>
        <button title="再読み込み" onClick={() => refetch()}>⟳</button>
        <button title="プラグインフォルダを開く" onClick={() => dir() && void revealInExplorer(dir())}>📂</button>
        <button title="閉じる" onClick={togglePluginPanel}>×</button>
      </header>
      <div class="plugin-list">
        <For each={plugins() ?? []}>
          {(p) => (
            <div
              classList={{ "plugin-row": true, active: active()?.manifest.id === p.manifest.id }}
              onClick={() => activate(p)}
            >
              <label
                class="plugin-toggle"
                title={isPluginEnabled(p.manifest.id) ? "無効化" : "有効化"}
                onClick={(e) => e.stopPropagation()}
              >
                <input
                  type="checkbox"
                  checked={isPluginEnabled(p.manifest.id)}
                  onChange={(e) => toggleEnable(p, e)}
                />
              </label>
              <div class="plugin-info">
                <div class="plugin-name">{p.manifest.name}</div>
                <div class="plugin-meta muted">{p.manifest.id} v{p.manifest.version}</div>
                <Show when={p.manifest.description}>
                  <div class="plugin-desc muted">{p.manifest.description}</div>
                </Show>
              </div>
            </div>
          )}
        </For>
        <Show when={(plugins()?.length ?? 0) === 0}>
          <div class="empty muted">
            プラグインがありません。<br/>
            <small>{dir()}</small> に manifest.json を含むフォルダを置いてください。
          </div>
        </Show>
      </div>
      <Show when={active()}>
        <div class="plugin-frame-wrap">
          <iframe
            ref={frameRef}
            class="plugin-frame"
            sandbox="allow-scripts"
            title="plugin"
          />
        </div>
      </Show>
      <footer class="plugin-foot muted">
        <Show when={state.showPluginPanel}>
          <small>※ 有効化したプラグインのみ capability を呼び出せます</small>
        </Show>
      </footer>
    </aside>
  );
}
