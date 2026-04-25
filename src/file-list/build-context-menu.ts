// FileList のコンテキストメニュー定義 (右クリックメニュー)
// 大きなメニュー配列を切出して FileList.tsx の本体をスリム化する。
import type { ContextMenuItem } from "../components/ContextMenu";
import type { FileEntry } from "../types";
import { state, addTab } from "../store";
import { joinPath } from "../path-util";
import { openWithShell, revealInExplorer, showProperties, shellMenuShow } from "../fs";
import { invokePluginContextMenuItem } from "../plugin-host";
import {
  cutSelection,
  copySelection,
  pasteHere,
  doDelete,
  doRename,
  doNewFolder,
  doNewFileBuiltin,
  doNewFileFromTemplate,
  exportAsciiTree,
  type FileOpsCtx,
} from "./file-ops";
import { BUILTIN_TEMPLATES, userTemplates } from "../templates";

export interface BuildMenuCtx extends FileOpsCtx {
  target: FileEntry | null;
  ctxPos: () => { x: number; y: number } | null;
  enter: (e: FileEntry) => void;
}

export function buildContextMenu(ctx: BuildMenuCtx): ContextMenuItem[] {
  const sel = ctx.pane().selection;
  const target = ctx.target;
  const hasSel = sel.length > 0;
  const single = sel.length === 1;
  const cb = state.clipboard;
  const fullSel = sel.map((n) => joinPath(ctx.pane().path, n));
  const firstPath = fullSel[0];

  const base: ContextMenuItem[] = [
    {
      label: "開く", icon: "▶", disabled: !single,
      onClick: () => { if (target) ctx.enter(target); },
    },
    {
      label: "新しいタブで開く", icon: "🗂", disabled: !single || (target && target.kind !== "dir") === true,
      onClick: () => { if (target?.kind === "dir") addTab(joinPath(ctx.pane().path, target.name)); },
    },
    {
      label: "既定のアプリで開く", icon: "📤", disabled: !single,
      onClick: () => { if (firstPath) void openWithShell(firstPath); },
    },
    { separator: true },
    { label: "切り取り", icon: "✂", shortcut: "Ctrl+X", disabled: !hasSel, onClick: () => cutSelection(ctx) },
    { label: "コピー", icon: "📋", shortcut: "Ctrl+C", disabled: !hasSel, onClick: () => copySelection(ctx) },
    {
      label: cb ? `貼り付け (${cb.paths.length}件 / ${cb.op === "cut" ? "切り取り" : "コピー"})` : "貼り付け",
      icon: "📥", shortcut: "Ctrl+V", disabled: !cb, onClick: () => { void pasteHere(ctx); },
    },
    { separator: true },
    { label: "名前の変更", icon: "✎", shortcut: "F2", disabled: !single, onClick: () => { void doRename(ctx); } },
    { label: "新規フォルダ", icon: "📁", shortcut: "Ctrl+Shift+N", onClick: () => { void doNewFolder(ctx); } },
    {
      label: "新規ファイル",
      icon: "📄",
      submenu: [
        ...BUILTIN_TEMPLATES.map((t) => ({
          label: t.label,
          icon: t.icon ?? "📄",
          onClick: () => { void doNewFileBuiltin(ctx, t); },
        })),
        ...(userTemplates().length > 0
          ? [
              { separator: true } as ContextMenuItem,
              ...userTemplates().map((t) => ({
                label: t.name,
                icon: "🧩",
                onClick: () => { void doNewFileFromTemplate(ctx, t); },
              })),
            ]
          : []),
      ],
    },
    { separator: true },
    {
      label: "ツリーをコピー (ASCII)", icon: "🌳",
      disabled: !single || (target?.kind !== "dir"),
      onClick: () => { if (target?.kind === "dir") void exportAsciiTree(joinPath(ctx.pane().path, target.name)); },
    },
    { separator: true },
    { label: "ゴミ箱へ", icon: "🗑", shortcut: "Del", disabled: !hasSel, onClick: () => { void doDelete(ctx, false); } },
    { label: "完全削除", icon: "✖", shortcut: "Shift+Del", disabled: !hasSel, danger: true, onClick: () => { void doDelete(ctx, true); } },
    { separator: true },
    {
      label: "エクスプローラで表示", icon: "🪟", disabled: !single,
      onClick: () => { if (firstPath) void revealInExplorer(firstPath); },
    },
    {
      label: "Windows メニュー…", icon: "🪄", shortcut: "Shift+右クリック", disabled: !hasSel,
      onClick: () => {
        if (!fullSel.length) return;
        const sx = window.screenX + (ctx.ctxPos()?.x ?? 100);
        const sy = window.screenY + (ctx.ctxPos()?.y ?? 100);
        void shellMenuShow(fullSel, sx, sy).catch((err) => console.warn("shellMenuShow:", err));
      },
    },
    {
      label: "プロパティ", icon: "ℹ", disabled: !single,
      onClick: () => { if (firstPath) void showProperties(firstPath); },
    },
  ];

  // v2.0: プラグイン提供のコンテキストメニュー項目を末尾に追加 (右クリック対象 target で判定)
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
          const tgtPath = joinPath(ctx.pane().path, target.name);
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
}