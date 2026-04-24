import { Show, createSignal, onCleanup, onMount } from "solid-js";
import { state, setTabColumns, setShowHidden, setShowThumbnails, setHotkey } from "../store";
import { eventToCombo } from "../hotkeys";
import type { HotkeyAction } from "../types";
import PerfPanel from "./PerfPanel";
import GeneralTab from "./settings/GeneralTab";
import SearchTab from "./settings/SearchTab";
import HotkeysTab from "./settings/HotkeysTab";
import PresetsTab from "./settings/PresetsTab";
import PluginsTab from "./settings/PluginsTab";

interface Props {
  open: boolean;
  onClose: () => void;
}

type TabId = "general" | "search" | "hotkeys" | "presets" | "plugins" | "perf";

export default function SettingsDialog(props: Props) {
  const [tab, setTab] = createSignal<TabId>("general");
  const [columns, setColumns] = createSignal(state.tabColumns);
  const [hidden, setHidden] = createSignal(state.showHidden);
  const [thumbs, setThumbs] = createSignal(state.showThumbnails);
  const [dirty, setDirty] = createSignal(false);
  const [capturing, setCapturing] = createSignal<HotkeyAction | null>(null);

  const reset = () => {
    setColumns(state.tabColumns);
    setHidden(state.showHidden);
    setThumbs(state.showThumbnails);
    setDirty(false);
  };

  let prevOpen = false;
  const sync = () => {
    if (props.open && !prevOpen) reset();
    prevOpen = props.open;
  };

  onMount(() => {
    sync();
    const onKey = (e: KeyboardEvent) => {
      if (!props.open) return;
      if (capturing()) {
        // ホットキーキャプチャ中: このダイアログ内で吸収
        const combo = eventToCombo(e);
        if (combo) {
          e.preventDefault();
          e.stopPropagation();
          setHotkey(capturing()!, combo);
          setCapturing(null);
        } else if (e.key === "Escape") {
          setCapturing(null);
        }
        return;
      }
      if (e.key === "Escape") props.onClose();
    };
    window.addEventListener("keydown", onKey, true);
    onCleanup(() => window.removeEventListener("keydown", onKey, true));
  });

  const apply = () => {
    const n = Math.min(8, Math.max(1, Math.floor(columns())));
    setTabColumns(n);
    setShowHidden(hidden());
    setShowThumbnails(thumbs());
    setDirty(false);
    props.onClose();
  };

  const applyAndReload = () => {
    apply();
    setTimeout(() => location.reload(), 50);
  };

  return (
    <Show when={(sync(), props.open)}>
      <div class="modal-backdrop" onClick={props.onClose}>
        <div class="modal modal-lg" onClick={(e) => e.stopPropagation()}>
          <header class="modal-head">
            <strong>設定</strong>
            <span class="spacer" />
            <button class="modal-close" onClick={props.onClose}>×</button>
          </header>

          <nav class="settings-tabs">
            <button classList={{ active: tab() === "general" }} onClick={() => setTab("general")}>基本</button>
            <button classList={{ active: tab() === "search" }} onClick={() => setTab("search")}>検索</button>
            <button classList={{ active: tab() === "hotkeys" }} onClick={() => setTab("hotkeys")}>ホットキー</button>
            <button classList={{ active: tab() === "presets" }} onClick={() => setTab("presets")}>プリセット</button>
            <button classList={{ active: tab() === "plugins" }} onClick={() => setTab("plugins")}>プラグイン</button>
            <button classList={{ active: tab() === "perf" }} onClick={() => setTab("perf")}>計測</button>
          </nav>

          <section class="modal-body">
            <Show when={tab() === "general"}>
              <GeneralTab
                columns={columns()}
                hidden={hidden()}
                thumbs={thumbs()}
                onColumnsChange={(n) => { setColumns(n); setDirty(true); }}
                onHiddenChange={(v) => { setHidden(v); setDirty(true); }}
                onThumbsChange={(v) => { setThumbs(v); setDirty(true); }}
              />
            </Show>
            <Show when={tab() === "search"}><SearchTab /></Show>
            <Show when={tab() === "hotkeys"}>
              <HotkeysTab capturing={capturing()} onCapture={setCapturing} />
            </Show>
            <Show when={tab() === "presets"}><PresetsTab /></Show>
            <Show when={tab() === "plugins"}><PluginsTab /></Show>
            <Show when={tab() === "perf"}><PerfPanel /></Show>
          </section>

          <footer class="modal-foot">
            <small class="muted">{dirty() ? "未保存の変更あり" : ""}</small>
            <span class="spacer" />
            <button onClick={props.onClose}>キャンセル</button>
            <button onClick={applyAndReload}>適用して再読み込み</button>
            <button class="primary" onClick={apply}>OK</button>
          </footer>
        </div>
      </div>
    </Show>
  );
}
