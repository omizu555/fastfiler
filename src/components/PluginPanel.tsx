import { For, Show, createResource, createSignal, onCleanup, onMount } from "solid-js";
import { listPlugins, pluginInvoke, pluginsDirPath, revealInExplorer } from "../fs";
import { state, togglePluginPanel } from "../store";
import type { PluginInfo } from "../types";

export default function PluginPanel() {
  const [plugins, { refetch }] = createResource(() => listPlugins());
  const [active, setActive] = createSignal<PluginInfo | null>(null);
  const [dir, setDir] = createSignal("");
  let frameRef: HTMLIFrameElement | undefined;

  onMount(async () => {
    setDir(await pluginsDirPath());
    // postMessage ブリッジ
    const onMessage = async (ev: MessageEvent) => {
      if (!ev.data || typeof ev.data !== "object") return;
      const { __ff, id, capability, args } = ev.data as {
        __ff?: string; id?: number; capability?: string; args?: Record<string, unknown>;
      };
      if (__ff !== "invoke" || !capability) return;
      const cur = active();
      if (!cur) return;
      try {
        const result = await pluginInvoke(cur.manifest.id, capability, args ?? {});
        ev.source?.postMessage(
          { __ff: "result", id, ok: true, result },
          { targetOrigin: ev.origin === "null" ? "*" : ev.origin } as WindowPostMessageOptions,
        );
      } catch (e) {
        ev.source?.postMessage(
          { __ff: "result", id, ok: false, error: String(e) },
          { targetOrigin: ev.origin === "null" ? "*" : ev.origin } as WindowPostMessageOptions,
        );
      }
    };
    window.addEventListener("message", onMessage);
    onCleanup(() => window.removeEventListener("message", onMessage));
  });

  const activate = (p: PluginInfo) => {
    setActive(p);
    if (frameRef) {
      // file:// URL から読み込む
      const url = "file:///" + p.entry_path.replace(/\\/g, "/");
      frameRef.src = url;
    }
  };

  return (
    <aside class="plugin-panel">
      <header class="plugin-head">
        <strong>プラグイン</strong>
        <span class="spacer" />
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
              <div class="plugin-name">{p.manifest.name}</div>
              <div class="plugin-meta muted">{p.manifest.id} v{p.manifest.version}</div>
              <Show when={p.manifest.description}>
                <div class="plugin-desc muted">{p.manifest.description}</div>
              </Show>
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
          <small>※ capability で許可された API のみ呼び出し可能</small>
        </Show>
      </footer>
    </aside>
  );
}
