// v3.5: ターミナル統合 (xterm.js + portable-pty)
import "xterm/css/xterm.css";
import { Terminal } from "xterm";
import { FitAddon } from "xterm-addon-fit";
import { createEffect, createSignal, onCleanup, onMount, For, Show } from "solid-js";
import { state, toggleTerminal, setTerminalHeight, focusedLeafPaneId } from "../store";

interface TermSession {
  id: number;
  title: string;
  term: Terminal;
  fit: FitAddon;
  unlistenData?: () => void;
  unlistenExit?: () => void;
  alive: boolean;
}

async function tauriInvoke<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const mod = await import("@tauri-apps/api/core");
  return mod.invoke<T>(cmd, args);
}
async function tauriListen<T = unknown>(event: string, cb: (e: { payload: T }) => void): Promise<() => void> {
  const mod = await import("@tauri-apps/api/event");
  return mod.listen<T>(event, cb);
}

export default function TerminalPanel() {
  const [sessions, setSessions] = createSignal<TermSession[]>([]);
  const [activeId, setActiveId] = createSignal<number | null>(null);
  let containerRef: HTMLDivElement | undefined;
  let resizeStartY = 0;
  let resizeStartH = 0;

  const themeIsLight = () => document.documentElement.dataset.theme === "light";

  const newSession = async () => {
    const pid = focusedLeafPaneId();
    const cwd = pid ? state.panes[pid]?.path : undefined;
    const shell = state.terminalShell ?? undefined;
    let id: number;
    try {
      id = await tauriInvoke<number>("term_open", { cwd, shell });
    } catch (e) {
      alert(`ターミナル起動失敗: ${e}`);
      return;
    }

    const term = new Terminal({
      fontFamily: state.terminalFont || "Cascadia Mono, Consolas, monospace",
      fontSize: state.terminalFontSize ?? 13,
      cursorBlink: true,
      theme: themeIsLight()
        ? { background: "#ffffff", foreground: "#1f2328", cursor: "#1f2328" }
        : { background: "#0f1218", foreground: "#e6e6e6", cursor: "#e6e6e6" },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.onData((d) => { void tauriInvoke("term_write", { id, data: d }); });
    term.onResize(({ cols, rows }) => { void tauriInvoke("term_resize", { id, cols, rows }); });

    const session: TermSession = { id, title: cwd ?? "shell", term, fit, alive: true };

    const unData = await tauriListen<{ id: number; data: string }>("term:data", (e) => {
      if (e.payload.id === id) term.write(e.payload.data);
    });
    const unExit = await tauriListen<{ id: number; code: number | null }>("term:exit", (e) => {
      if (e.payload.id === id) {
        session.alive = false;
        term.write(`\r\n\x1b[2m[プロセス終了 code=${e.payload.code ?? "?"}]\x1b[0m\r\n`);
      }
    });
    session.unlistenData = unData;
    session.unlistenExit = unExit;

    setSessions((xs) => [...xs, session]);
    setActiveId(id);
  };

  const closeSession = async (id: number) => {
    const s = sessions().find((x) => x.id === id);
    if (!s) return;
    s.unlistenData?.();
    s.unlistenExit?.();
    s.term.dispose();
    try { await tauriInvoke("term_close", { id }); } catch { /* */ }
    setSessions((xs) => xs.filter((x) => x.id !== id));
    if (activeId() === id) {
      const remaining = sessions().filter((x) => x.id !== id);
      setActiveId(remaining[0]?.id ?? null);
    }
  };

  // 表示切替で初回セッション作成
  createEffect(() => {
    if (state.showTerminal && sessions().length === 0) {
      void newSession();
    }
  });

  // active 切替で xterm を attach
  createEffect(() => {
    const id = activeId();
    if (!containerRef) return;
    containerRef.innerHTML = "";
    if (id == null) return;
    const s = sessions().find((x) => x.id === id);
    if (!s) return;
    s.term.open(containerRef);
    queueMicrotask(() => {
      try { s.fit.fit(); } catch { /* */ }
      s.term.focus();
    });
  });

  // パネル高変化時に fit
  createEffect(() => {
    state.terminalHeight;
    const id = activeId();
    const s = sessions().find((x) => x.id === id);
    if (s) queueMicrotask(() => { try { s.fit.fit(); } catch { /* */ } });
  });

  // 終了時に全 session 解放
  onCleanup(() => {
    for (const s of sessions()) {
      s.unlistenData?.();
      s.unlistenExit?.();
      s.term.dispose();
      void tauriInvoke("term_close", { id: s.id }).catch(() => {});
    }
  });

  // ResizeObserver で fit
  onMount(() => {
    if (typeof ResizeObserver !== "undefined" && containerRef) {
      const ro = new ResizeObserver(() => {
        const id = activeId();
        const s = sessions().find((x) => x.id === id);
        if (s) { try { s.fit.fit(); } catch { /* */ } }
      });
      ro.observe(containerRef);
      onCleanup(() => ro.disconnect());
    }
  });

  const onResizeStart = (e: MouseEvent) => {
    resizeStartY = e.clientY;
    resizeStartH = state.terminalHeight ?? 240;
    e.preventDefault();
    const onMove = (ev: MouseEvent) => {
      const dy = resizeStartY - ev.clientY;
      setTerminalHeight(Math.max(80, Math.min(800, resizeStartH + dy)));
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  return (
    <Show when={state.showTerminal}>
      <div class="terminal-panel" style={{ height: `${state.terminalHeight ?? 240}px` }}>
        <div class="terminal-resize" onMouseDown={onResizeStart} />
        <div class="terminal-tabbar">
          <For each={sessions()}>
            {(s) => (
              <div
                class="terminal-tab"
                classList={{ active: activeId() === s.id }}
                onClick={() => setActiveId(s.id)}
                title={s.title}
              >
                <span>{s.alive ? "▶" : "■"} {s.title.split(/[\\/]/).pop() || "shell"}</span>
                <button class="terminal-tab-close" onClick={(e) => { e.stopPropagation(); void closeSession(s.id); }}>×</button>
              </div>
            )}
          </For>
          <button class="terminal-new" onClick={() => void newSession()} title="新しいシェル">＋</button>
          <span style={{ flex: 1 }} />
          <button class="terminal-close" onClick={toggleTerminal} title="ターミナルを閉じる">×</button>
        </div>
        <div class="terminal-host" ref={(el) => (containerRef = el)} />
      </div>
    </Show>
  );
}
