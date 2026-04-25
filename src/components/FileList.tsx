import { For, Show, createMemo, createResource, createEffect, createSignal, on, onCleanup } from "solid-js";
import { listDir, listDirs, watchDir, unwatchDir, listenFsChange, formatSize, formatDate, openWithShell, diskFree, shellMenuShow } from "../fs";
import { getCachedListing, setCachedListing } from "../dir-cache";
import { beginLoading, endLoading } from "../loading-state";
import { breadcrumbsOf, joinPath, parentPath } from "../path-util";
import { sortFileEntries } from "../file-list/sort";
import { broadcastPluginEvent } from "../plugin-host";
import {
  setPanePath,
  setPaneSelection,
  setPaneScroll,
  setPaneLinkGroup,
  splitPane,
  closePane,
  addTab,
  setPaneView,
  focusPaneSearch,
  togglePaneSearch,
  togglePaneSearchFocused,
  setPaneName,
  getPaneUi,
  setPaneSort,
  setFocusedPane,
  navigateBack,
  navigateForward,
  canGoBack,
  canGoForward,
  isPaneLocked,
  state,
  refreshTickFor,
  setFileListColWidth,
} from "../store";
import type { FileEntry, SortKey } from "../types";
import ContextMenu from "./ContextMenu";
import Thumbnail, { shouldThumb } from "./Thumbnail";
import SearchPanel from "./SearchPanel";
import { iconForEntryWith } from "../icons";
import { matchKey } from "../hotkeys";
import {
  cutSelection as opCut,
  copySelection as opCopy,
  pasteHere as opPaste,
  doDelete as opDelete,
  doRename as opRename,
  doNewFolder as opNewFolder,
  exportAsciiTree as opExportAsciiTree,
  type FileOpsCtx,
} from "../file-list/file-ops";
import { buildContextMenu } from "../file-list/build-context-menu";
import { registerPaneRefetch, extDragPaneId, extDragRowName } from "../dnd";
import { beginRightDragCandidate } from "../file-list/right-drag";

interface Props {
  paneId: string;
  tabId: string;
}

type ColKey = "name" | "size" | "mtime" | "kind";

/**
 * 列ヘッダ右端のドラッグハンドル。
 * 隣接列とで合計幅が保たれるように両方の % を更新する。
 */
function ColResizer(p: { paneId: string; col: ColKey; nextCol: ColKey }) {
  const onPointerDown = (ev: PointerEvent) => {
    ev.stopPropagation();
    ev.preventDefault();
    const startX = ev.clientX;
    const th = (ev.currentTarget as HTMLElement).closest("th") as HTMLElement | null;
    const table = th?.closest("table") as HTMLElement | null;
    const tableW = table?.getBoundingClientRect().width ?? 1;
    const cw = state.paneUi[p.paneId]?.colWidths ?? { name: 50, size: 15, mtime: 25, kind: 10 };
    const startA = cw[p.col];
    const startB = cw[p.nextCol];
    const sumAB = startA + startB;
    const onMove = (e: PointerEvent) => {
      const dxPct = ((e.clientX - startX) / tableW) * 100;
      let a = startA + dxPct;
      let b = startB - dxPct;
      const min = 5;
      if (a < min) { a = min; b = sumAB - a; }
      if (b < min) { b = min; a = sumAB - b; }
      setFileListColWidth(p.paneId, p.col, a);
      setFileListColWidth(p.paneId, p.nextCol, b);
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove, true);
      window.removeEventListener("pointerup", onUp, true);
      document.body.style.cursor = "";
    };
    window.addEventListener("pointermove", onMove, true);
    window.addEventListener("pointerup", onUp, true);
    document.body.style.cursor = "col-resize";
  };
  return (
    <span
      class="col-resizer"
      onPointerDown={onPointerDown}
      onClick={(e) => e.stopPropagation()}
      title="ドラッグで列幅変更"
    />
  );
}

export default function FileList(props: Props) {
  const pane = () => state.panes[props.paneId];

  const [refreshKey, bumpRefresh] = (() => {
    const [k, setK] = createSignal(0);
    return [k, () => setK(k() + 1)] as const;
  })();

  const [entries, { refetch }] = createResource(
    () => ({ path: pane().path, k: refreshKey(), g: refreshTickFor(pane().path) }),
    async (s) => {
      beginLoading();
      try {
        const list = await listDir(s.path);
        setCachedListing(s.path, list);
        return state.showHidden ? list : list.filter((e) => !e.hidden);
      } finally {
        endLoading();
      }
    },
    {
      // v1.10: 起動時/フォルダ移動時の体感速度向上のため、キャッシュがあれば即時描画する
      initialValue: (() => {
        const cached = getCachedListing(pane().path);
        if (!cached) return undefined;
        return state.showHidden ? cached : cached.filter((e) => !e.hidden);
      })(),
    },
  );

  // 監視・自動更新
  createEffect(() => {
    const p = pane().path;
    let unlisten: (() => void) | null = null;
    let stopped = false;
    void watchDir(p);
    void listenFsChange((ev) => {
      if (ev.path === p && !stopped) refetch();
    }).then((u) => { unlisten = u; if (stopped) u(); });
    onCleanup(() => {
      stopped = true;
      void unwatchDir(p);
      unlisten?.();
    });
  });

  const visible = createMemo<FileEntry[]>(() => {
    const list = entries() ?? [];
    const ui = getPaneUi(props.paneId);
    return sortFileEntries(list, {
      key: ui.sortKey,
      dir: ui.sortDir,
      foldersFirst: ui.foldersFirst,
    });
  });

  // ----- ステータスバー: 選択合計サイズ + ドライブ空き容量 -----
  const selectionSize = createMemo(() => {
    const sel = new Set(pane().selection);
    let n = 0;
    for (const e of visible()) {
      if (sel.has(e.name) && e.kind === "file") n += e.size ?? 0;
    }
    return n;
  });

  const [diskInfo, setDiskInfo] = createSignal<{ total: number; free: number; available: number } | null>(null);
  createEffect(() => {
    const p = pane().path;
    let cancelled = false;
    const refresh = () => {
      void diskFree(p).then((d) => { if (!cancelled) setDiskInfo(d); });
    };
    refresh();
    const id = window.setInterval(refresh, 5000);
    onCleanup(() => { cancelled = true; window.clearInterval(id); });
  });

  const sortIndicator = (k: SortKey) => {
    const ui = getPaneUi(props.paneId);
    if (ui.sortKey !== k) return "";
    return ui.sortDir === "asc" ? " ▲" : " ▼";
  };

  let lastClickedIndex: number | null = null;

  const onRowClick = (name: string, idx: number, ev: MouseEvent) => {
    const sel = pane().selection;
    const all = visible().map((e) => e.name);
    if (ev.shiftKey && lastClickedIndex != null) {
      const [a, b] = [Math.min(lastClickedIndex, idx), Math.max(lastClickedIndex, idx)];
      setPaneSelection(props.paneId, all.slice(a, b + 1));
    } else if (ev.ctrlKey) {
      const next = sel.includes(name)
        ? sel.filter((n) => n !== name)
        : [...sel, name];
      setPaneSelection(props.paneId, next);
      lastClickedIndex = idx;
    } else {
      setPaneSelection(props.paneId, [name]);
      lastClickedIndex = idx;
    }
  };

  const enter = (e: FileEntry) => {
    if (e.kind === "dir") {
      setPanePath(props.paneId, joinPath(pane().path, e.name));
    } else {
      void openWithShell(joinPath(pane().path, e.name));
    }
  };

  // ----- ファイル操作 ctx + 薄いラッパー (詳細は file-list/file-ops.ts) -----
  const fopsCtx: FileOpsCtx = {
    pane,
    visible,
    refetch: () => refetch(),
  };
  const cutSelection = () => opCut(fopsCtx);
  const copySelection = () => opCopy(fopsCtx);
  const pasteHere = () => opPaste(fopsCtx);
  const doDelete = (permanent: boolean) => opDelete(fopsCtx, permanent);
  const doRename = () => opRename(fopsCtx);
  const doNewFolder = () => opNewFolder(fopsCtx);
  const exportAsciiTree = (rootPath: string) => opExportAsciiTree(rootPath);

  // ----- インクリメンタルサーチ (英数キーで先頭一致選択) -----
  let typingBuffer = "";
  let typingTimer: number | null = null;

  const onKey = async (ev: KeyboardEvent) => {
    // v1.7: 検索 input 等の入力要素内のキーはペインの hotkey として処理しない
    const tgt = ev.target as HTMLElement | null;
    if (tgt && (tgt.tagName === "INPUT" || tgt.tagName === "TEXTAREA" || tgt.isContentEditable)) {
      return;
    }
    const sel = pane().selection;
    const hk = state.hotkeys;
    if (matchKey(hk["address-bar"], ev)) {
      ev.preventDefault();
      setPathError(null);
      setEditingPath(true);
      return;
    }
    if (matchKey(hk.open, ev) && sel.length === 1) {
      const e = visible().find((x) => x.name === sel[0]);
      if (e) enter(e);
    } else if (matchKey(hk.parent, ev)) {
      setPanePath(props.paneId, parentPath(pane().path));
    } else if (matchKey(hk.refresh, ev)) {
      ev.preventDefault();
      refetch();
    } else if (matchKey(hk.rename, ev) && sel.length === 1) {
      await doRename();
    } else if (matchKey(hk["delete-permanent"], ev) && sel.length > 0) {
      ev.preventDefault();
      await doDelete(true);
    } else if (matchKey(hk.delete, ev) && sel.length > 0) {
      ev.preventDefault();
      await doDelete(false);
    } else if (matchKey(hk["new-folder"], ev)) {
      ev.preventDefault();
      await doNewFolder();
    } else if (matchKey(hk.cut, ev)) {
      ev.preventDefault();
      cutSelection();
    } else if (matchKey(hk.copy, ev)) {
      ev.preventDefault();
      copySelection();
    } else if (matchKey(hk.paste, ev)) {
      ev.preventDefault();
      await pasteHere();
    } else if (matchKey(hk["select-all"], ev)) {
      ev.preventDefault();
      setPaneSelection(props.paneId, visible().map((e) => e.name));
    } else if (matchKey(hk.search, ev)) {
      ev.preventDefault();
      togglePaneSearchFocused(props.paneId);
    } else if (
      ev.key === "ArrowUp" || ev.key === "ArrowDown"
      || ev.key === "PageUp" || ev.key === "PageDown"
      || ev.key === "Home" || ev.key === "End"
    ) {
      const items = visible();
      if (items.length === 0) return;
      ev.preventDefault();
      const cursor = sel.length > 0
        ? items.findIndex((e) => e.name === sel[sel.length - 1])
        : -1;
      const pageSize = listRef
        ? Math.max(1, Math.floor(listRef.clientHeight / ROW_H) - 1)
        : 10;
      let next = cursor;
      switch (ev.key) {
        case "ArrowUp":   next = cursor < 0 ? items.length - 1 : Math.max(0, cursor - 1); break;
        case "ArrowDown": next = cursor < 0 ? 0 : Math.min(items.length - 1, cursor + 1); break;
        case "PageUp":   next = Math.max(0, (cursor < 0 ? 0 : cursor) - pageSize); break;
        case "PageDown": next = Math.min(items.length - 1, (cursor < 0 ? 0 : cursor) + pageSize); break;
        case "Home": next = 0; break;
        case "End":  next = items.length - 1; break;
      }
      if (ev.shiftKey && sel.length > 0) {
        const anchorIdx = items.findIndex((e) => e.name === sel[0]);
        const a = anchorIdx >= 0 ? anchorIdx : (cursor >= 0 ? cursor : next);
        const lo = Math.min(a, next), hi = Math.max(a, next);
        setPaneSelection(props.paneId, items.slice(lo, hi + 1).map((e) => e.name));
      } else {
        setPaneSelection(props.paneId, [items[next].name]);
      }
      if (listRef) {
        const top = next * ROW_H;
        const bottom = top + ROW_H;
        if (top < listRef.scrollTop) listRef.scrollTop = top;
        else if (bottom > listRef.scrollTop + listRef.clientHeight) {
          listRef.scrollTop = bottom - listRef.clientHeight;
        }
      }
    } else if (
      ev.key.length === 1 && !ev.ctrlKey && !ev.altKey && !ev.metaKey
      && /[\p{L}\p{N}._\-\s]/u.test(ev.key)
    ) {
      // インクリメンタルサーチ
      typingBuffer += ev.key.toLowerCase();
      if (typingTimer !== null) clearTimeout(typingTimer);
      typingTimer = window.setTimeout(() => { typingBuffer = ""; typingTimer = null; }, 600);
      const buf = typingBuffer;
      const items = visible();
      const hit = items.find((e) => e.name.toLowerCase().startsWith(buf));
      if (hit) {
        ev.preventDefault();
        setPaneSelection(props.paneId, [hit.name]);
        const idx = items.indexOf(hit);
        if (listRef) {
          const top = idx * ROW_H;
          const bottom = top + ROW_H;
          if (top < listRef.scrollTop || bottom > listRef.scrollTop + listRef.clientHeight) {
            listRef.scrollTop = Math.max(0, top - listRef.clientHeight / 2);
          }
        }
      }
    }
  };

  // ----- コンテキストメニュー -----
  const [ctxPos, setCtxPos] = createSignal<{ x: number; y: number } | null>(null);
  const [ctxTarget, setCtxTarget] = createSignal<{ entry: FileEntry | null }>({ entry: null });

  const openContextMenu = (e: MouseEvent, entry: FileEntry | null) => {
    e.preventDefault();
    e.stopPropagation();
    if (entry) {
      // 右クリック対象が選択範囲外なら、それだけを選択する
      if (!pane().selection.includes(entry.name)) {
        setPaneSelection(props.paneId, [entry.name]);
      }
    } else {
      setPaneSelection(props.paneId, []);
    }
    // Shift+右クリックで Windows ネイティブの右クリックメニューを直接開く
    if (e.shiftKey && entry) {
      const sel = pane().selection.includes(entry.name) ? pane().selection : [entry.name];
      const fullSel = sel.map((n) => joinPath(pane().path, n));
      const sx = (e as MouseEvent).screenX;
      const sy = (e as MouseEvent).screenY;
      void shellMenuShow(fullSel, sx, sy).catch((err) => console.warn("shellMenuShow:", err));
      return;
    }
    setCtxTarget({ entry });
    setCtxPos({ x: e.clientX, y: e.clientY });
  };

  const buildMenu = () => buildContextMenu({
    ...fopsCtx,
    target: ctxTarget().entry,
    ctxPos,
    enter,
  });

  // ----- 検索モード (タブ切替で保持される: store.paneUi) -----
  const searchMode = () => getPaneUi(props.paneId).searchOpen;

  // v1.6: Ctrl+F で検索を閉じた直後にペイン本体へ focus を戻す
  let paneRef: HTMLDivElement | undefined;
  createEffect(on(
    () => getPaneUi(props.paneId).paneFocusTick,
    (t) => {
      if (!t) return;
      queueMicrotask(() => paneRef?.focus());
    },
    { defer: true }
  ));

  // ----- D&D: 着地ペイン refresh のため refetch を pointer エンジンに登録 -----
  const unregRefetch = registerPaneRefetch(props.paneId, () => refetch());
  onCleanup(() => unregRefetch());

  const isCut = (name: string) => {
    const cb = state.clipboard;
    if (!cb || cb.op !== "cut") return false;
    return cb.paths.includes(joinPath(pane().path, name));
  };

  // ----- パンくず / アドレスバー -----
  const [editingPath, setEditingPath] = createSignal(false);
  const [pathError, setPathError] = createSignal<string | null>(null);

  const breadcrumbs = createMemo(() => breadcrumbsOf(pane().path));
  let crumbListRef: HTMLDivElement | undefined;
  createEffect(() => {
    breadcrumbs();
    queueMicrotask(() => {
      if (crumbListRef) crumbListRef.scrollLeft = crumbListRef.scrollWidth;
    });
  });

  // パンくずへの D&D ドロップは hit-test (data-rd-crumb-path) で内部 pointer エンジンが処理する
  // (旧 HTML5 経路は撤去済み)

  // ----- 仮想スクロール -----
  const ROW_H = 28;
  const BUFFER = 10;
  const [scrollTop, setScrollTop] = createSignal(0);
  const [viewH, setViewH] = createSignal(600);
  let listRef: HTMLDivElement | undefined;
  let lastAppliedRatio = -1;

  // ----- v1.6 (16.1): 矩形範囲選択 (rubber-band) -----
  type RubberRect = { x: number; y: number; w: number; h: number };
  const [rubber, setRubber] = createSignal<RubberRect | null>(null);
  const onListMouseDown = (ev: MouseEvent) => {
    if (ev.button !== 0) return;
    const t = ev.target as HTMLElement | null;
    if (!t || !listRef) return;
    // 行 (空白パディング以外) 上では起動しない
    const tr = t.closest("tr");
    if (tr && !tr.classList.contains("vpad")) return;
    // 入力要素・ボタン上では起動しない
    if (t.closest("input,button,textarea,select,[contenteditable]")) return;
    const cRect = listRef.getBoundingClientRect();
    const sx = ev.clientX - cRect.left + listRef.scrollLeft;
    const sy = ev.clientY - cRect.top + listRef.scrollTop;
    const additive = ev.ctrlKey || ev.shiftKey;
    const originalSel = pane().selection.slice();
    if (!additive) setPaneSelection(props.paneId, []);
    setRubber({ x: sx, y: sy, w: 0, h: 0 });
    setFocusedPane(props.paneId);
    const onMove = (mev: MouseEvent) => {
      if (!listRef) return;
      const r = listRef.getBoundingClientRect();
      const cx = mev.clientX - r.left + listRef.scrollLeft;
      const cy = mev.clientY - r.top + listRef.scrollTop;
      const minX = Math.min(sx, cx);
      const minY = Math.min(sy, cy);
      const w = Math.abs(cx - sx);
      const h = Math.abs(cy - sy);
      setRubber({ x: minX, y: minY, w, h });
      // 行交差判定
      const rows = listRef.querySelectorAll("tr[data-rd-name]");
      const cRect2 = listRef.getBoundingClientRect();
      const sel = new Set<string>(additive ? originalSel : []);
      rows.forEach((node) => {
        const el = node as HTMLElement;
        const rb = el.getBoundingClientRect();
        const top = rb.top - cRect2.top + listRef!.scrollTop;
        const bottom = rb.bottom - cRect2.top + listRef!.scrollTop;
        const intersects = !(bottom < minY || top > minY + h);
        if (intersects) {
          const name = el.getAttribute("data-rd-name");
          if (name) sel.add(name);
        }
      });
      setPaneSelection(props.paneId, Array.from(sel));
    };
    const onUp = () => {
      setRubber(null);
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  // v3.4: 他ペインからの scrollRatio 変化を反映 (フィードバックループ防止)
  createEffect(() => {
    const r = pane().scrollRatio;
    if (typeof r !== "number" || !isFinite(r)) return;
    if (Math.abs(r - lastAppliedRatio) < 0.0005) return;
    if (!listRef) return;
    const max = Math.max(1, listRef.scrollHeight - listRef.clientHeight);
    const targetTop = Math.round(r * max);
    if (Math.abs(listRef.scrollTop - targetTop) > 1) {
      lastAppliedRatio = r;
      listRef.scrollTop = targetTop;
      setScrollTop(targetTop);
    }
  });

  const slice = createMemo(() => {
    const items = visible();
    const top = scrollTop();
    const h = viewH();
    const start = Math.max(0, Math.floor(top / ROW_H) - BUFFER);
    const end = Math.min(items.length, Math.ceil((top + h) / ROW_H) + BUFFER);
    return {
      start,
      end,
      items: items.slice(start, end),
      topPad: start * ROW_H,
      bottomPad: Math.max(0, (items.length - end) * ROW_H),
    };
  });

  return (
    <div
      ref={paneRef}
      class="pane"
      data-pane-id={props.paneId}
      data-rd-pane-path={pane().path}
      classList={{
        "pane-ext-drag-over": extDragPaneId() === props.paneId,
        "pane-focused": state.focusedPaneId === props.paneId,
        "pane-locked": isPaneLocked(props.paneId),
      }}
      tabIndex={0}
      onPointerDown={() => setFocusedPane(props.paneId)}
      onFocusIn={() => setFocusedPane(props.paneId)}
      onKeyDown={onKey}
      onContextMenu={(e) => {
        // v1.6 (16.6): プラグインへブロードキャスト (空白部分のみ)
        const t = e.target as HTMLElement | null;
        const tr = t?.closest("tr");
        if (!tr || tr.classList.contains("vpad")) {
          broadcastPluginEvent("pane.dom.contextmenu", {
            paneId: props.paneId,
            path: pane().path,
            target: "empty",
            x: e.clientX,
            y: e.clientY,
          });
        }
        openContextMenu(e, null);
      }}
      onDblClick={(e) => {
        // v1.6 (16.6): 空白部分のダブルクリックをプラグインへ
        const t = e.target as HTMLElement | null;
        const tr = t?.closest("tr");
        if (!tr || tr.classList.contains("vpad")) {
          broadcastPluginEvent("pane.dom.dblclick", {
            paneId: props.paneId,
            path: pane().path,
            target: "empty",
          });
        }
      }}
    >
      <div class="pane-toolbar">
        <button title="親フォルダへ (Backspace)"
          onClick={() => setPanePath(props.paneId, parentPath(pane().path))}>↑</button>
        <Show when={editingPath()} fallback={
          <div
            class="breadcrumbs"
            onClick={(ev) => {
              if (ev.target === ev.currentTarget) { setPathError(null); setEditingPath(true); }
            }}
            onDblClick={(ev) => {
              const t = ev.target as HTMLElement;
              if (t.tagName !== "BUTTON") { setPathError(null); setEditingPath(true); }
            }}
          >
            <div class="crumb-list" ref={crumbListRef}>
              <For each={breadcrumbs()}>
                {(c, i) => (
                  <>
                    <Show when={i() > 0}><span class="crumb-sep">›</span></Show>
                    <button
                      class="crumb"
                      data-rd-crumb-path={c.path}
                      onClick={() => setPanePath(props.paneId, c.path)}
                      title={c.path}
                    >{c.label}</button>
                  </>
                )}
              </For>
            </div>
            <button class="crumb-edit" title="パスを編集 / コピー (Ctrl+L)" onClick={() => { setPathError(null); setEditingPath(true); }}>✎</button>
          </div>
        }>
          <input
            class="pane-path"
            classList={{ "pane-path-error": !!pathError() }}
            value={pane().path}
            title={pathError() ?? ""}
            ref={(el) => queueMicrotask(() => { el?.focus(); el?.select(); })}
            onChange={async (e) => {
              const v = e.currentTarget.value.trim();
              try {
                await listDir(v);
                setPathError(null);
                setPanePath(props.paneId, v);
                setEditingPath(false);
              } catch (err) {
                setPathError(String(err));
                e.currentTarget.focus();
              }
            }}
            onBlur={() => { if (!pathError()) setEditingPath(false); }}
            onKeyDown={async (e) => {
              if (e.key === "Escape") { setPathError(null); setEditingPath(false); }
              else if (e.key === "Tab") {
                e.preventDefault();
                const el = e.currentTarget;
                const v = el.value;
                // 末尾を分離して prefix を出す
                const m = v.match(/^(.*[\\\/])([^\\\/]*)$/);
                const parent = m ? m[1] : v;
                const prefix = (m ? m[2] : "").toLowerCase();
                try {
                  const dirs = await listDirs(parent.replace(/[\\\/]$/, "") || parent, true);
                  const candidates = dirs
                    .map((d) => d.name)
                    .filter((n) => n.toLowerCase().startsWith(prefix))
                    .sort();
                  if (candidates.length === 0) return;
                  // 共通プレフィクス
                  let common = candidates[0];
                  for (const c of candidates) {
                    let i = 0;
                    while (i < common.length && i < c.length
                      && common[i].toLowerCase() === c[i].toLowerCase()) i++;
                    common = common.slice(0, i);
                  }
                  const completed = parent + (common || candidates[0]);
                  el.value = completed + (candidates.length === 1 ? "\\" : "");
                  // 補完した部分を選択
                  el.setSelectionRange(parent.length + prefix.length, el.value.length);
                  setPathError(null);
                } catch { /* noop */ }
              }
            }}
          />
        </Show>
        <button title="再読込 (F5)" onClick={() => { bumpRefresh(); refetch(); }}>⟳</button>
        <button title="水平分割" onClick={() => splitPane(props.tabId, props.paneId, "h")}>⬌</button>
        <button title="垂直分割" onClick={() => splitPane(props.tabId, props.paneId, "v")}>⬍</button>
        <button title="検索 (Ctrl+F)" classList={{ active: searchMode() }} onClick={() => togglePaneSearch(props.paneId)}>🔍</button>
        <button title="ペインを閉じる" onClick={() => closePane(props.tabId, props.paneId)}>✕</button>
      </div>
      <Show when={searchMode()}>
        <SearchPanel paneId={props.paneId} />
      </Show>
      <div
        class="file-list"
        ref={(el) => {
          listRef = el;
          // 初期スクロール復元
          queueMicrotask(() => {
            if (el && Math.abs(el.scrollTop - pane().scrollTop) > 1) {
              el.scrollTop = pane().scrollTop;
              setScrollTop(el.scrollTop);
            }
            if (el) setViewH(el.clientHeight);
          });
          if (el && typeof ResizeObserver !== "undefined") {
            const ro = new ResizeObserver(() => setViewH(el.clientHeight));
            ro.observe(el);
            onCleanup(() => ro.disconnect());
          }
        }}
        onMouseDown={onListMouseDown}
        onScroll={(e) => {
          const el = e.currentTarget;
          const top = el.scrollTop;
          const max = Math.max(1, el.scrollHeight - el.clientHeight);
          const ratio = max > 0 ? top / max : 0;
          setScrollTop(top);
          setPaneScroll(props.paneId, top, ratio);
        }}
      >
        <table class="vlist">
          <colgroup>
            <col style={{ width: `${(state.paneUi[props.paneId]?.colWidths.name ?? 50)}%` }} />
            <col style={{ width: `${(state.paneUi[props.paneId]?.colWidths.size ?? 15)}%` }} />
            <col style={{ width: `${(state.paneUi[props.paneId]?.colWidths.mtime ?? 25)}%` }} />
            <col style={{ width: `${(state.paneUi[props.paneId]?.colWidths.kind ?? 10)}%` }} />
          </colgroup>
          <thead>
            <tr>
              <th class="sortable" onClick={() => setPaneSort(props.paneId, "name")}>
                名前{sortIndicator("name")}
                <ColResizer paneId={props.paneId} col="name" nextCol="size" />
              </th>
              <th class="sortable" onClick={() => setPaneSort(props.paneId, "size")}>
                サイズ{sortIndicator("size")}
                <ColResizer paneId={props.paneId} col="size" nextCol="mtime" />
              </th>
              <th class="sortable" onClick={() => setPaneSort(props.paneId, "mtime")}>
                更新日時{sortIndicator("mtime")}
                <ColResizer paneId={props.paneId} col="mtime" nextCol="kind" />
              </th>
              <th class="sortable" onClick={() => setPaneSort(props.paneId, "kind")}>
                種別{sortIndicator("kind")}
              </th>
            </tr>
          </thead>
          <tbody>
            <Show when={slice().topPad > 0}>
              <tr class="vpad" style={{ height: `${slice().topPad}px` }}><td colspan={4} /></tr>
            </Show>
            <For each={slice().items}>
              {(e, i) => {
                const idx = () => slice().start + i();
                return (
                  <tr
                    style={{ height: `${ROW_H}px` }}
                    classList={{
                      selected: pane().selection.includes(e.name),
                      hidden: !!e.hidden,
                      cut: isCut(e.name),
                      "drag-over-row":
                        extDragPaneId() === props.paneId && extDragRowName() === e.name && e.kind === "dir",
                    }}
                    data-rd-pane-path={pane().path}
                    data-rd-name={e.name}
                    data-rd-folder={e.kind === "dir" ? "1" : undefined}
                    onMouseDown={(ev) => {
                      if (ev.button !== 2) return;
                      let sel = pane().selection;
                      if (!sel.includes(e.name)) {
                        setPaneSelection(props.paneId, [e.name]);
                        sel = [e.name];
                      }
                      const paths = sel.map((n) => joinPath(pane().path, n));
                      beginRightDragCandidate(
                        { paths, sourcePath: pane().path, label: paths.length === 1 ? (paths[0].split(/[\\/]/).pop() ?? "") : `${paths.length} 件` },
                        ev.clientX,
                        ev.clientY,
                      );
                    }}
                    onClick={(ev) => onRowClick(e.name, idx(), ev)}
                    onDblClick={() => enter(e)}
                    onContextMenu={(ev) => openContextMenu(ev, e)}
                  >
                    <td>
                      <Show
                        when={state.showThumbnails && shouldThumb(e.ext)}
                        fallback={<span class="icon">{iconForEntryWith(e, state.iconSet)}</span>}
                      >
                        <Thumbnail
                          path={joinPath(pane().path, e.name)}
                          ext={e.ext}
                          size={48}
                          fallback={iconForEntryWith(e, state.iconSet)}
                        />
                      </Show>
                      {e.name}
                    </td>
                    <td>{e.kind === "dir" ? "" : formatSize(e.size)}</td>
                    <td>{formatDate(e.modified)}</td>
                    <td>{e.ext ?? (e.kind === "dir" ? "<DIR>" : "")}</td>
                  </tr>
                );
              }}
            </For>
            <Show when={slice().bottomPad > 0}>
              <tr class="vpad" style={{ height: `${slice().bottomPad}px` }}><td colspan={4} /></tr>
            </Show>
          </tbody>
        </table>
        <Show when={!entries.loading && visible().length === 0}>
          <div class="empty">空のフォルダ または 読み込み不可</div>
        </Show>
        <Show when={entries.loading}>
          <div class="empty muted">読み込み中…</div>
        </Show>
        <Show when={rubber()}>
          {(r) => (
            <div
              class="rubber-band"
              style={{
                left: `${r().x}px`,
                top: `${r().y}px`,
                width: `${r().w}px`,
                height: `${r().h}px`,
              }}
            />
          )}
        </Show>
      </div>
      <div class="pane-status">
        {visible().length} 項目 ／ 選択 {pane().selection.length}
        <Show when={selectionSize() > 0}>
          <span class="muted" style={{ "margin-left": "10px" }}>
            ／ {formatSize(selectionSize())}
          </span>
        </Show>
        <Show when={state.clipboard}>
          {(cb) => (
            <span class="muted" style={{ "margin-left": "10px" }}>
              ／ クリップボード: {cb().paths.length}件 ({cb().op === "cut" ? "切り取り" : "コピー"})
            </span>
          )}
        </Show>
        <Show when={diskInfo()}>
          {(d) => (
            <span class="muted status-disk" title={`空き ${formatSize(d().free)} / 全 ${formatSize(d().total)}`}>
              💾 空き {formatSize(d().free)} / {formatSize(d().total)}
            </span>
          )}
        </Show>
      </div>
      <Show when={ctxPos()}>
        {(p) => (
          <ContextMenu
            x={p().x}
            y={p().y}
            items={buildMenu()}
            onClose={() => setCtxPos(null)}
          />
        )}
      </Show>
    </div>
  );
}
