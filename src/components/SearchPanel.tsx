import { For, Show, createEffect, createSignal, on, onCleanup, onMount } from "solid-js";
import {
  everythingPing,
  listenSearchDone,
  listenSearchHit,
  openWithShell,
  searchCancel,
  searchFiles,
} from "../fs";
import {
  getPaneUi,
  setPanePath,
  setPaneSearchOpen,
  setPaneSearchOption,
  setPaneSearchQuery,
  state,
} from "../store";
import type { SearchHit, SearchDoneInfo } from "../types";
import { iconForEntryWith } from "../icons";

interface Props {
  paneId: string;
}

export default function SearchPanel(props: Props) {
  const ui = () => getPaneUi(props.paneId);
  const [results, setResults] = createSignal<SearchHit[]>([]);
  const [running, setRunning] = createSignal(false);
  const [currentJob, setCurrentJob] = createSignal<number | null>(null);
  const [doneInfo, setDoneInfo] = createSignal<SearchDoneInfo | null>(null);
  const [everythingAlive, setEverythingAlive] = createSignal<boolean | null>(null);

  let inputEl: HTMLInputElement | undefined;

  let unsubHit: (() => void) | null = null;
  let unsubDone: (() => void) | null = null;

  const ensureSubscribed = async () => {
    if (!unsubHit) {
      unsubHit = await listenSearchHit((h) => {
        if (h.job_id !== currentJob()) return;
        setResults((arr) => (arr.length < 5000 ? [...arr, h] : arr));
      });
    }
    if (!unsubDone) {
      unsubDone = await listenSearchDone((d) => {
        if (d.job_id !== currentJob()) return;
        setRunning(false);
        setDoneInfo(d);
      });
    }
  };

  onMount(() => {
    if (state.searchBackend === "everything") {
      void everythingPing(state.everythingPort).then(setEverythingAlive);
    }
    // 初期マウント時 (= Ctrl+F で開いた直後など) に focus
    queueMicrotask(() => {
      inputEl?.focus();
      inputEl?.select();
    });
  });

  // Ctrl+F 再押下 → focusTick が bump される → 再 focus + select
  createEffect(on(
    () => ui().searchFocusTick,
    (tick) => {
      if (tick === 0) return;
      queueMicrotask(() => {
        inputEl?.focus();
        inputEl?.select();
      });
    },
    { defer: true },
  ));

  onCleanup(() => {
    unsubHit?.();
    unsubDone?.();
    void searchCancel();
  });

  const start = async () => {
    const p = ui().searchQuery.trim();
    if (!p) return;
    const pane = state.panes[props.paneId];
    if (!pane) return;
    await ensureSubscribed();
    setResults([]);
    setDoneInfo(null);
    setRunning(true);
    if (state.searchBackend === "everything") {
      void everythingPing(state.everythingPort).then(setEverythingAlive);
    }
    try {
      const job = await searchFiles(pane.path, p, {
        caseSensitive: ui().searchCaseSensitive,
        useRegex: ui().searchRegex,
        includeHidden: state.showHidden,
        maxResults: 5000,
        backend: state.searchBackend,
        everythingPort: state.everythingPort,
        everythingScope: state.everythingScope,
      });
      setCurrentJob(job);
    } catch (e) {
      setRunning(false);
      alert(`検索エラー: ${e}`);
    }
  };

  const stop = () => {
    void searchCancel();
    setRunning(false);
  };

  const onResultClick = (h: SearchHit) => {
    if (h.is_dir) {
      setPanePath(props.paneId, h.path);
    } else {
      const parent = h.path.replace(/[\\/][^\\/]+$/, "");
      setPanePath(props.paneId, parent);
    }
  };

  const onResultDbl = (h: SearchHit) => {
    if (h.is_dir) {
      setPanePath(props.paneId, h.path);
    } else {
      void openWithShell(h.path);
    }
  };

  return (
    <div class="search-panel">
      <div class="search-bar">
        <input
          ref={inputEl}
          type="text"
          class="search-input"
          placeholder={state.searchBackend === "everything" ? "Everything 経由で検索…" : "このフォルダ以下を検索…"}
          value={ui().searchQuery}
          onInput={(e) => setPaneSearchQuery(props.paneId, e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void start();
            if (e.key === "Escape") {
              stop();
              setPaneSearchOpen(props.paneId, false);
            }
            if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
              e.preventDefault();
              stop();
              setPaneSearchOpen(props.paneId, false);
            }
          }}
        />
        <label class="inline" title="大文字小文字を区別">
          <input type="checkbox" checked={ui().searchCaseSensitive}
            onChange={(e) => setPaneSearchOption(props.paneId, "searchCaseSensitive", e.currentTarget.checked)} />Aa
        </label>
        <label class="inline" title="正規表現">
          <input type="checkbox" checked={ui().searchRegex}
            onChange={(e) => setPaneSearchOption(props.paneId, "searchRegex", e.currentTarget.checked)} />.*
        </label>
        <span
          class="search-backend"
          classList={{
            ev: state.searchBackend === "everything",
            "ev-down": state.searchBackend === "everything" && everythingAlive() === false,
          }}
          title={
            state.searchBackend === "everything"
              ? `Everything HTTP (port ${state.everythingPort}) ${everythingAlive() === false ? "未応答" : ""}`
              : "内蔵 (再帰列挙)"
          }
        >
          {state.searchBackend === "everything" ? "⚡ Everything" : "🐢 内蔵"}
        </span>
        <Show when={!running()} fallback={<button onClick={stop}>停止</button>}>
          <button class="primary" onClick={start}>検索</button>
        </Show>
      </div>
      <div class="search-results">
        <For each={results()}>
          {(h) => (
            <div
              class="search-row"
              title={h.path}
              onClick={() => onResultClick(h)}
              onDblClick={() => onResultDbl(h)}
            >
              <span class="icon">{iconForEntryWith({ kind: h.is_dir ? "dir" : "file", ext: h.name.includes(".") ? h.name.split(".").pop() : null }, state.iconSet)}</span>
              <span class="search-name">{h.name}</span>
              <span class="search-path muted">{h.path}</span>
            </div>
          )}
        </For>
        <Show when={results().length === 0 && !running() && doneInfo()}>
          <div class="empty muted">該当なし</div>
        </Show>
      </div>
      <footer class="search-foot muted">
        {results().length} 件
        <Show when={running()}> ／ 検索中…</Show>
        <Show when={doneInfo()}>
          {(d) => (
            <span>
              {" "}／ 完了 (合計 {d().total}{d().canceled ? " / 中断" : ""}, backend: {d().backend})
              <Show when={d().fallback}>
                <span style={{ color: "#ffa726" }}> ／ Everything 失敗 → 内蔵にフォールバック ({d().error})</span>
              </Show>
            </span>
          )}
        </Show>
      </footer>
    </div>
  );
}
