// FileList のファイル操作 (cut/copy/paste/delete/rename/newFolder/exportAsciiTree)
// FileList.tsx の肥大化を抑えるため独立した純粋寄りモジュールに切出。
// pane/visible/refetch は呼び出し側 (FileList) から ctx で渡してもらう。
import type { FileEntry, PaneState, UndoOp } from "../types";
import {
  state,
  setClipboard,
  clearClipboard,
  pushUndo,
  bumpRefreshPaths,
  bumpRefreshPath,
  pushToast,
} from "../store";
import { joinPath } from "../path-util";
import {
  deletePath,
  deleteToTrash,
  renamePath,
  createDir,
  writeClipboardPaths,
  readClipboardPaths,
} from "../fs";
import { runFileJob } from "../jobs";
import { openPrompt } from "../components/PromptDialog";
import { invalidNameMessage, uniqueName } from "./name-utils";
import { resolveDestinations, refreshTargets } from "./resolve-dest";
import { buildAsciiTree, parseDepthInput } from "./ascii-tree";

export interface FileOpsCtx {
  pane: () => PaneState;
  visible: () => FileEntry[];
  refetch: () => void;
}

export function cutSelection(ctx: FileOpsCtx) {
  const sel = ctx.pane().selection;
  if (!sel.length) return;
  const paths = sel.map((n) => joinPath(ctx.pane().path, n));
  setClipboard(paths, "cut");
  void writeClipboardPaths(paths, "cut");
  pushToast(`切り取り ${paths.length} 件`, "info");
}

export function copySelection(ctx: FileOpsCtx) {
  const sel = ctx.pane().selection;
  if (!sel.length) return;
  const paths = sel.map((n) => joinPath(ctx.pane().path, n));
  setClipboard(paths, "copy");
  void writeClipboardPaths(paths, "copy");
  pushToast(`コピー ${paths.length} 件`, "info");
}

export async function pasteHere(ctx: FileOpsCtx) {
  const dst = ctx.pane().path;
  // OS クリップボード優先 (エクスプローラからのコピー/カットを必ず拾うため)。
  // OS 側に無いか取得失敗した場合のみ、アプリ内 clipboard をフォールバックで使う。
  let paths: string[];
  let isCut: boolean;
  let fromInternal = false;
  const ext = await readClipboardPaths().catch(() => null);
  if (ext && ext.paths.length > 0) {
    paths = ext.paths;
    isCut = ext.op === "cut";
    // アプリ内 clipboard と一致するなら、cut 後の元削除責任を負う必要があるため internal 扱い
    if (
      state.clipboard &&
      state.clipboard.op === ext.op &&
      state.clipboard.paths.length === ext.paths.length &&
      state.clipboard.paths.every((p, i) => p === ext.paths[i])
    ) {
      fromInternal = true;
    }
  } else if (state.clipboard) {
    paths = state.clipboard.paths;
    isCut = state.clipboard.op === "cut";
    fromInternal = true;
  } else {
    return;
  }
  const op = isCut ? "move" : "copy";
  const items = await resolveDestinations(paths, dst, op);
  if (items.length === 0) {
    pushToast("貼り付け対象がありません (同じ場所への移動)", "info");
    return;
  }
  if (fromInternal && isCut) clearClipboard();
  const renamedCount = items.filter((it) => it.renamed).length;
  const label = `${isCut ? "移動" : "コピー"} ${items.length}件 → ${dst}`;
  const r = await runFileJob(op, items.map(({ from, to }) => ({ from, to })), { label });
  if (r.ok) {
    const ops: UndoOp[] = items.map((it) =>
      isCut
        ? { kind: "move", from: it.from, to: it.to }
        : { kind: "copy", created: it.to },
    );
    pushUndo(label, ops);
    bumpRefreshPaths(refreshTargets(items, dst, isCut));
    const note = renamedCount > 0 ? ` (${renamedCount}件は名前変更)` : "";
    pushToast(`${isCut ? "移動" : "コピー"} ${items.length}件 完了${note}`, "info");
  } else if (!r.canceled) {
    console.error(`[file-ops] ${label} 失敗`);
    pushToast(`${isCut ? "移動" : "コピー"} 失敗`, "error");
  }
  ctx.refetch();
}

export async function doDelete(ctx: FileOpsCtx, permanent: boolean) {
  const sel = ctx.pane().selection;
  if (!sel.length) return;
  const msg = permanent
    ? `${sel.length} 件を完全削除しますか？（元に戻せません）`
    : `${sel.length} 件をゴミ箱へ移動しますか？`;
  if (!confirm(msg)) return;
  const full = sel.map((n) => joinPath(ctx.pane().path, n));
  try {
    if (permanent) {
      for (const p of full) {
        try {
          await deletePath(p, true);
        } catch (e) {
          console.error(e);
        }
      }
    } else {
      await deleteToTrash(full);
    }
  } catch (e) {
    alert(`削除失敗: ${e}`);
  }
  bumpRefreshPath(ctx.pane().path);
  ctx.refetch();
}

export async function doRename(ctx: FileOpsCtx) {
  const sel = ctx.pane().selection;
  if (sel.length !== 1) return;
  const oldName = sel[0];
  const existing = new Set(ctx.visible().map((e) => e.name));
  existing.delete(oldName);
  const newName = await openPrompt({
    title: "名前の変更",
    label: oldName,
    initial: oldName,
    confirmLabel: "変更",
    validate: (v) => invalidNameMessage(v, existing),
  });
  if (newName && newName !== oldName) {
    const from = joinPath(ctx.pane().path, oldName);
    const to = joinPath(ctx.pane().path, newName);
    try {
      await renamePath(from, to);
      pushUndo(`名前変更: ${oldName} → ${newName}`, [
        { kind: "rename", from, to },
      ]);
      bumpRefreshPath(ctx.pane().path);
      ctx.refetch();
    } catch (e) {
      alert(`リネーム失敗: ${e}`);
    }
  }
}

export async function doNewFolder(ctx: FileOpsCtx) {
  const existing = new Set(ctx.visible().map((e) => e.name));
  const initial = uniqueName("新しいフォルダー", existing);
  const name = await openPrompt({
    title: "新しいフォルダー",
    label: "フォルダー名",
    initial,
    confirmLabel: "作成",
    validate: (v) => invalidNameMessage(v, existing),
  });
  if (!name) return;
  try {
    await createDir(joinPath(ctx.pane().path, name.trim()));
    bumpRefreshPath(ctx.pane().path);
    ctx.refetch();
  } catch (e) {
    alert(`作成失敗: ${e}`);
  }
}

export async function exportAsciiTree(rootPath: string) {
  const depthStr = await openPrompt({
    title: "ツリーをコピー (ASCII)",
    label: "再帰の深さ (1〜8) / ファイル含む場合は末尾に f を付与 (例: 4f)",
    initial: "4",
    confirmLabel: "コピー",
    validate: (v) =>
      parseDepthInput(v) ? null : "数字 (1-8) を入力 (例: 4 または 4f)",
  });
  if (!depthStr) return;
  const opts = parseDepthInput(depthStr)!;
  const text = await buildAsciiTree(rootPath, {
    ...opts,
    includeHidden: state.showHidden,
  });
  try {
    await navigator.clipboard.writeText(text);
  } catch (e) {
    alert(`クリップボードコピー失敗: ${e}\n\n${text}`);
  }
}
