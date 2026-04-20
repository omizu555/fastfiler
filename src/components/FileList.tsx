import { For, Show, createMemo, createResource, createEffect, createSignal, on, onCleanup } from "solid-js";
import { listDir, listDirs, watchDir, unwatchDir, listenFsChange, formatSize, formatDate, openWithShell, revealInExplorer, showProperties, deletePath, deleteToTrash, renamePath, createDir, copyPath, movePath, diskFree } from "../fs";
import { breadcrumbsOf, joinPath, parentPath } from "../path-util";
import { openPrompt } from "./PromptDialog";
import {
  setPanePath,
  setPaneSelection,
  setPaneScroll,
  setPaneLinkGroup,
  splitPane,
  closePane,
  setClipboard,
  clearClipboard,
  addTab,
  setPaneView,
  focusPaneSearch,
  togglePaneSearch,
  togglePaneSearchFocused,
  setPaneName,
  getPaneUi,
  setFocusedPane,
  pushUndo,
  pushToast,
  state,
} from "../store";
import { performUndo } from "../undo";
import type { FileEntry } from "../types";
import ContextMenu, { type ContextMenuItem } from "./ContextMenu";
import Thumbnail, { shouldThumb } from "./Thumbnail";
import SearchPanel from "./SearchPanel";
import { PaneNameLabel } from "./DriveListView";
import { matchKey } from "../hotkeys";
import { invokePluginContextMenuItem } from "../plugin-host";

interface Props {
  paneId: string;
  tabId: string;
}

interface DragPayload {
  paths: string[];
  sourcePath: string;
}

export default function FileList(props: Props) {
  const pane = () => state.panes[props.paneId];

  const [refreshKey, bumpRefresh] = (() => {
    const [k, setK] = createSignal(0);
    return [k, () => setK(k() + 1)] as const;
  })();

  const [entries, { refetch }] = createResource(
    () => ({ path: pane().path, k: refreshKey() }),
    async (s) => {
      const list = await listDir(s.path);
      return state.showHidden ? list : list.filter((e) => !e.hidden);
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

  const visible = createMemo<FileEntry[]>(() => entries() ?? []);

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

  // ----- ASCII ツリーエクスポート -----
  const exportAsciiTree = async (rootPath: string) => {
    const depthStr = await openPrompt({
      title: "ツリーをコピー (ASCII)",
      label: "再帰の深さ (1〜8) / ファイル含む場合は末尾に f を付与 (例: 4f)",
      initial: "4",
      confirmLabel: "コピー",
      validate: (v) => {
        const m = v.trim().match(/^(\d+)(f?)$/i);
        if (!m) return "数字を入力してください (例: 4 または 4f)";
        const n = parseInt(m[1], 10);
        if (n < 1 || n > 8) return "深さは 1〜8 の範囲で";
        return null;
      },
    });
    if (!depthStr) return;
    const m = depthStr.trim().match(/^(\d+)(f?)$/i)!;
    const maxDepth = parseInt(m[1], 10);
    const includeFiles = !!m[2];
    const rootName = rootPath.split(/[\\/]/).pop() || rootPath;
    const lines: string[] = [rootName];

    const walk = async (path: string, prefix: string, depth: number) => {
      if (depth > maxDepth) return;
      let items: FileEntry[];
      try {
        items = await listDir(path);
      } catch { return; }
      const filtered = items
        .filter((e) => includeFiles || e.kind === "dir")
        .filter((e) => state.showHidden || !e.hidden)
        .sort((a, b) => {
          if (a.kind !== b.kind) return a.kind === "dir" ? -1 : 1;
          return a.name.localeCompare(b.name);
        });
      for (let i = 0; i < filtered.length; i++) {
        const e = filtered[i];
        const last = i === filtered.length - 1;
        const branch = last ? "└─ " : "├─ ";
        lines.push(prefix + branch + e.name + (e.kind === "dir" ? "/" : ""));
        if (e.kind === "dir") {
          await walk(joinPath(path, e.name), prefix + (last ? "   " : "│  "), depth + 1);
        }
      }
    };
    await walk(rootPath, "", 1);
    const text = lines.join("\n");
    try {
      await navigator.clipboard.writeText(text);
    } catch (e) {
      alert(`クリップボードコピー失敗: ${e}\n\n${text}`);
    }
  };


  // ----- クリップボード操作 -----
  const cutSelection = () => {
    const sel = pane().selection;
    if (!sel.length) return;
    setClipboard(sel.map((n) => joinPath(pane().path, n)), "cut");
  };
  const copySelection = () => {
    const sel = pane().selection;
    if (!sel.length) return;
    setClipboard(sel.map((n) => joinPath(pane().path, n)), "copy");
  };
  const pasteHere = async () => {
    const cb = state.clipboard;
    if (!cb) return;
    const dst = pane().path;
    const fn = cb.op === "cut" ? movePath : copyPath;
    const ops: import("../types").UndoOp[] = [];
    let okCount = 0;
    for (const src of cb.paths) {
      const name = src.split(/[\\/]/).pop() ?? "untitled";
      const to = joinPath(dst, name);
      try {
        await fn(src, to);
        if (cb.op === "cut") ops.push({ kind: "move", from: src, to });
        else ops.push({ kind: "copy", created: to });
        okCount++;
      } catch (e) {
        console.error(e);
      }
    }
    if (cb.op === "cut") clearClipboard();
    if (okCount > 0) {
      const label = cb.op === "cut" ? `移動 ${okCount}件` : `コピー ${okCount}件`;
      pushUndo(label, ops);
      pushToast(label, "info", { label: "↶取り消し", onClick: () => { void performUndo(); } });
    }
    refetch();
  };

  // ----- 削除 (Trash / 完全削除) -----
  const doDelete = async (permanent: boolean) => {
    const sel = pane().selection;
    if (!sel.length) return;
    const msg = permanent
      ? `${sel.length} 件を完全削除しますか？（元に戻せません）`
      : `${sel.length} 件をゴミ箱へ移動しますか？`;
    if (!confirm(msg)) return;
    const full = sel.map((n) => joinPath(pane().path, n));
    try {
      if (permanent) {
        for (const p of full) {
          try { await deletePath(p, true); } catch (e) { console.error(e); }
        }
      } else {
        await deleteToTrash(full);
      }
    } catch (e) {
      alert(`削除失敗: ${e}`);
    }
    refetch();
  };

  const doRename = async () => {
    const sel = pane().selection;
    if (sel.length !== 1) return;
    const oldName = sel[0];
    const existing = new Set(visible().map((e) => e.name));
    existing.delete(oldName);
    const newName = await openPrompt({
      title: "名前の変更",
      label: oldName,
      initial: oldName,
      confirmLabel: "変更",
      validate: (v) => {
        const t = v.trim();
        if (!t) return "名前を入力してください";
        if (/[\\/:*?"<>|]/.test(t)) return "使用できない文字が含まれています";
        if (existing.has(t)) return "同名の項目が既に存在します";
        return null;
      },
    });
    if (newName && newName !== oldName) {
      const from = joinPath(pane().path, oldName);
      const to = joinPath(pane().path, newName);
      try {
        await renamePath(from, to);
        pushUndo(`名前変更: ${oldName} → ${newName}`, [{ kind: "rename", from, to }]);
        pushToast(`名前変更: ${oldName} → ${newName}`, "info",
          { label: "↶取り消し", onClick: () => { void performUndo(); } });
        refetch();
      } catch (e) { alert(`リネーム失敗: ${e}`); }
    }
  };

  const doNewFolder = async () => {
    const existing = new Set(visible().map((e) => e.name));
    let initial = "新しいフォルダー";
    if (existing.has(initial)) {
      let i = 2;
      while (existing.has(`${initial} (${i})`)) i++;
      initial = `${initial} (${i})`;
    }
    const name = await openPrompt({
      title: "新しいフォルダー",
      label: "フォルダー名",
      initial,
      confirmLabel: "作成",
      validate: (v) => {
        const t = v.trim();
        if (!t) return "名前を入力してください";
        if (/[\\/:*?"<>|]/.test(t)) return "使用できない文字が含まれています";
        if (existing.has(t)) return "同名の項目が既に存在します";
        return null;
      },
    });
    if (!name) return;
    try {
      await createDir(joinPath(pane().path, name.trim()));
      refetch();
    } catch (e) { alert(`作成失敗: ${e}`); }
  };

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
    setCtxTarget({ entry });
    setCtxPos({ x: e.clientX, y: e.clientY });
  };

  const buildMenu = (): ContextMenuItem[] => {
    const sel = pane().selection;
    const target = ctxTarget().entry;
    const hasSel = sel.length > 0;
    const single = sel.length === 1;
    const cb = state.clipboard;
    const fullSel = sel.map((n) => joinPath(pane().path, n));
    const firstPath = fullSel[0];
    const base: ContextMenuItem[] = [
      {
        label: "開く", icon: "▶", disabled: !single,
        onClick: () => { if (target) enter(target); },
      },
      {
        label: "新しいタブで開く", icon: "🗂", disabled: !single || (target && target.kind !== "dir") === true,
        onClick: () => { if (target?.kind === "dir") addTab(joinPath(pane().path, target.name)); },
      },
      {
        label: "既定のアプリで開く", icon: "📤", disabled: !single,
        onClick: () => { if (firstPath) void openWithShell(firstPath); },
      },
      { separator: true },
      { label: "切り取り", icon: "✂", shortcut: "Ctrl+X", disabled: !hasSel, onClick: cutSelection },
      { label: "コピー", icon: "📋", shortcut: "Ctrl+C", disabled: !hasSel, onClick: copySelection },
      {
        label: cb ? `貼り付け (${cb.paths.length}件 / ${cb.op === "cut" ? "切り取り" : "コピー"})` : "貼り付け",
        icon: "📥", shortcut: "Ctrl+V", disabled: !cb, onClick: pasteHere,
      },
      { separator: true },
      { label: "名前の変更", icon: "✎", shortcut: "F2", disabled: !single, onClick: doRename },
      { label: "新規フォルダ", icon: "📁", shortcut: "Ctrl+Shift+N", onClick: doNewFolder },
      { separator: true },
      {
        label: "ツリーをコピー (ASCII)", icon: "🌳",
        disabled: !single || (target?.kind !== "dir"),
        onClick: () => { if (target?.kind === "dir") void exportAsciiTree(joinPath(pane().path, target.name)); },
      },
      { separator: true },
      { label: "ゴミ箱へ", icon: "🗑", shortcut: "Del", disabled: !hasSel, onClick: () => doDelete(false) },
      { label: "完全削除", icon: "✖", shortcut: "Shift+Del", disabled: !hasSel, danger: true, onClick: () => doDelete(true) },
      { separator: true },
      {
        label: "エクスプローラで表示", icon: "🪟", disabled: !single,
        onClick: () => { if (firstPath) void revealInExplorer(firstPath); },
      },
      {
        label: "プロパティ", icon: "ℹ", disabled: !single,
        onClick: () => { if (firstPath) void showProperties(firstPath); },
      },
    ];
    // v2.0: プラグイン提供のコンテキストメニュー項目を末尾に追加
    // 右クリック対象 (target) を基準に判定する。選択数は問わない。
    const pluginItems = state.pluginContextMenu.filter((item) => {
      if (!target) return false;
      const isDir = target.kind === "dir";
      if (item.when === "file" && isDir) return false;
      if (item.when === "dir" && !isDir) return false;
      if (item.extensions && item.extensions.length > 0) {
        if (isDir) return false;
        const ext = target.name.includes(".")
          ? target.name.split(".").pop()!.toLowerCase()
          : "";
        if (!item.extensions.includes(ext)) return false;
      }
      return true;
    });
    if (pluginItems.length > 0) {
      base.push({ separator: true });
      for (const it of pluginItems) {
        base.push({
          label: it.label,
          icon: it.icon ?? "🧩",
          onClick: () => {
            if (!target) return;
            const tgtPath = joinPath(pane().path, target.name);
            invokePluginContextMenuItem(it, {
              path: tgtPath,
              isDir: target.kind === "dir",
              name: target.name,
            });
          },
        });
      }
    }
    return base;
  };

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

  // ----- D&D -----
  const [dragOverRow, setDragOverRow] = createSignal<string | null>(null);
  const [paneDragOver, setPaneDragOver] = createSignal(false);
  const DRAG_MIME = "application/x-fastfiler";

  const onRowDragStart = (ev: DragEvent, name: string) => {
    if (!ev.dataTransfer) return;
    let sel = pane().selection;
    if (!sel.includes(name)) {
      setPaneSelection(props.paneId, [name]);
      sel = [name];
    }
    const payload: DragPayload = {
      paths: sel.map((n) => joinPath(pane().path, n)),
      sourcePath: pane().path,
    };
    ev.dataTransfer.setData(DRAG_MIME, JSON.stringify(payload));
    ev.dataTransfer.effectAllowed = "copyMove";
  };

  const onPaneDragOver = (ev: DragEvent) => {
    if (!ev.dataTransfer?.types.includes(DRAG_MIME)) return;
    ev.preventDefault();
    ev.dataTransfer.dropEffect = ev.ctrlKey ? "copy" : "move";
    setPaneDragOver(true);
  };
  const onPaneDragLeave = () => setPaneDragOver(false);

  const handleDrop = async (ev: DragEvent, destPath: string) => {
    ev.preventDefault();
    setPaneDragOver(false);
    setDragOverRow(null);
    const raw = ev.dataTransfer?.getData(DRAG_MIME);
    if (!raw) return;
    let payload: DragPayload;
    try { payload = JSON.parse(raw); } catch { return; }
    if (payload.sourcePath === destPath && !ev.ctrlKey) return; // 同フォルダへの移動は無意味
    const isCopy = ev.ctrlKey;
    const fn = isCopy ? copyPath : movePath;
    const ops: import("../types").UndoOp[] = [];
    let okCount = 0;
    for (const src of payload.paths) {
      const name = src.split(/[\\/]/).pop() ?? "untitled";
      const to = joinPath(destPath, name);
      try {
        await fn(src, to);
        if (isCopy) ops.push({ kind: "copy", created: to });
        else ops.push({ kind: "move", from: src, to });
        okCount++;
      } catch (e) {
        console.error(e);
      }
    }
    if (okCount > 0) {
      const label = isCopy ? `コピー ${okCount}件` : `移動 ${okCount}件`;
      pushUndo(label, ops);
      pushToast(label, "info", { label: "↶取り消し", onClick: () => { void performUndo(); } });
    }
    refetch();
  };

  const onRowDragOver = (ev: DragEvent, entry: FileEntry) => {
    if (entry.kind !== "dir") return;
    if (!ev.dataTransfer?.types.includes(DRAG_MIME)) return;
    ev.preventDefault();
    ev.stopPropagation();
    ev.dataTransfer.dropEffect = ev.ctrlKey ? "copy" : "move";
    setDragOverRow(entry.name);
  };
  const onRowDrop = (ev: DragEvent, entry: FileEntry) => {
    if (entry.kind !== "dir") return;
    ev.stopPropagation();
    void handleDrop(ev, joinPath(pane().path, entry.name));
  };

  const isCut = (name: string) => {
    const cb = state.clipboard;
    if (!cb || cb.op !== "cut") return false;
    return cb.paths.includes(joinPath(pane().path, name));
  };

  // ----- パンくず / アドレスバー -----
  const [editingPath, setEditingPath] = createSignal(false);
  const [pathError, setPathError] = createSignal<string | null>(null);
  const [crumbDropIdx, setCrumbDropIdx] = createSignal<number | null>(null);

  const breadcrumbs = createMemo(() => breadcrumbsOf(pane().path));

  const onCrumbDragOver = (ev: DragEvent, idx: number) => {
    if (!ev.dataTransfer?.types.includes(DRAG_MIME)) return;
    ev.preventDefault();
    ev.stopPropagation();
    ev.dataTransfer.dropEffect = ev.ctrlKey ? "copy" : "move";
    setCrumbDropIdx(idx);
  };
  const onCrumbDrop = (ev: DragEvent, target: string) => {
    ev.stopPropagation();
    setCrumbDropIdx(null);
    void handleDrop(ev, target);
  };

  // ----- 仮想スクロール -----
  const ROW_H = 28;
  const BUFFER = 10;
  const [scrollTop, setScrollTop] = createSignal(0);
  const [viewH, setViewH] = createSignal(600);
  let listRef: HTMLDivElement | undefined;

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
      classList={{ "drop-target": paneDragOver(), "pane-focused": state.focusedPaneId === props.paneId }}
      tabIndex={0}
      onPointerDown={() => setFocusedPane(props.paneId)}
      onFocusIn={() => setFocusedPane(props.paneId)}
      onKeyDown={onKey}
      onContextMenu={(e) => openContextMenu(e, null)}
      onDragOver={onPaneDragOver}
      onDragLeave={onPaneDragLeave}
      onDrop={(e) => handleDrop(e, pane().path)}
    >
      <div class="pane-toolbar">
        <PaneNameLabel paneId={props.paneId} />
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
            <For each={breadcrumbs()}>
              {(c, i) => (
                <>
                  <Show when={i() > 0}><span class="crumb-sep">›</span></Show>
                  <button
                    class="crumb"
                    classList={{ "crumb-drop": crumbDropIdx() === i() }}
                    onClick={() => setPanePath(props.paneId, c.path)}
                    onDragOver={(ev) => onCrumbDragOver(ev, i())}
                    onDragLeave={() => { if (crumbDropIdx() === i()) setCrumbDropIdx(null); }}
                    onDrop={(ev) => onCrumbDrop(ev, c.path)}
                    title={c.path}
                  >{c.label}</button>
                </>
              )}
            </For>
            <button class="crumb-edit" title="パスを編集 (Ctrl+L)" onClick={() => { setPathError(null); setEditingPath(true); }}>✎</button>
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
        <select
          class="link-select"
          value={pane().linkGroupId ?? ""}
          onChange={(e) => setPaneLinkGroup(props.paneId, e.currentTarget.value || null)}
          title="連動グループ"
        >
          <option value="">連動なし</option>
          <For each={state.linkGroups}>
            {(g) => <option value={g.id}>{g.name}</option>}
          </For>
        </select>
        <Show when={pane().linkGroupId}>
          <span class="link-badge"
            style={{ background: state.linkGroups.find((g) => g.id === pane().linkGroupId)?.color }}/>
        </Show>
        <button title="水平分割" onClick={() => splitPane(props.tabId, props.paneId, "h")}>⬌</button>
        <button title="垂直分割" onClick={() => splitPane(props.tabId, props.paneId, "v")}>⬍</button>
        <button title="ツリー表示に切替" onClick={() => setPaneView(props.paneId, "tree")}>🌲</button>
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
        onScroll={(e) => {
          const top = e.currentTarget.scrollTop;
          setScrollTop(top);
          setPaneScroll(props.paneId, top);
        }}
      >
        <table class="vlist">
          <colgroup>
            <col style={{ width: "50%" }} />
            <col style={{ width: "15%" }} />
            <col style={{ width: "25%" }} />
            <col style={{ width: "10%" }} />
          </colgroup>
          <thead>
            <tr>
              <th>名前</th>
              <th>サイズ</th>
              <th>更新日時</th>
              <th>種別</th>
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
                      "drag-over-row": dragOverRow() === e.name,
                    }}
                    draggable={true}
                    onDragStart={(ev) => onRowDragStart(ev, e.name)}
                    onDragOver={(ev) => onRowDragOver(ev, e)}
                    onDragLeave={() => { if (dragOverRow() === e.name) setDragOverRow(null); }}
                    onDrop={(ev) => onRowDrop(ev, e)}
                    onClick={(ev) => onRowClick(e.name, idx(), ev)}
                    onDblClick={() => enter(e)}
                    onContextMenu={(ev) => openContextMenu(ev, e)}
                  >
                    <td>
                      <Show
                        when={state.showThumbnails && shouldThumb(e.ext)}
                        fallback={<span class="icon">{e.kind === "dir" ? "📁" : "📄"}</span>}
                      >
                        <Thumbnail
                          path={joinPath(pane().path, e.name)}
                          ext={e.ext}
                          size={48}
                          fallback={e.kind === "dir" ? "📁" : "📄"}
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
