// D&D / 右ドラッグ / コピペ から共通で呼ばれる「実行層」
// 衝突解決 → ジョブ実行 → undo 登録 → リフレッシュ → トースト
import { runFileJob } from "../jobs";
import { deletePath } from "../fs";
import { pushUndo, bumpRefreshPaths, pushToast } from "../store";
import type { UndoOp } from "../types";
import { openConflict } from "../components/ConflictDialog";
import { findConflicts, resolveDestinations, refreshTargets, type ConflictPolicy, type DestOp, type ResolvedItem } from "./resolve-dest";
import { getPaneRefetch } from "./ui-state";

export interface PerformDropInput {
  paths: string[];
  destPath: string;
  op: DestOp;
  /** ソース親フォルダ (refresh 対象に追加)。不明なら省略可 */
  sourceDir?: string;
  /** 着地ペイン ID (登録された refetch を呼ぶ)。不明なら省略可 */
  targetPaneId?: string | null;
  /** ログ用タグ ("[wv-drop]" など) */
  logTag?: string;
}

/**
 * 衝突があればユーザーに policy を尋ねる。衝突無しなら "rename" (no-op 同等) を返す。
 * "cancel" の場合は null を返し、呼び出し側で中断する。
 */
async function askConflictPolicy(paths: string[], destPath: string, op: DestOp): Promise<ConflictPolicy | null> {
  const conflicts = await findConflicts(paths, destPath, op);
  if (conflicts.length === 0) return "rename";
  const choice = await openConflict({
    op,
    conflictCount: conflicts.length,
    totalCount: paths.length,
    sampleNames: conflicts,
    destDir: destPath,
  });
  if (choice === "cancel") return null;
  return choice;
}

/**
 * policy=overwrite の項目について、既存の dst を事前削除する。
 * Windows の fs::rename は既存があると失敗するため move でも必須。
 * 失敗してもログのみで続行 (上書きできなかった項目はジョブ側で再エラーになる)。
 */
async function preDeleteOverwrites(items: ResolvedItem[]): Promise<void> {
  for (const it of items) {
    if (!it.overwrite) continue;
    try {
      await deletePath(it.to, true);
    } catch (e) {
      console.warn("[dnd] overwrite preDelete failed:", it.to, e);
    }
  }
}

export async function performDrop(input: PerformDropInput): Promise<void> {
  const { paths, destPath, op, sourceDir, targetPaneId, logTag = "[dnd]" } = input;
  const policy = await askConflictPolicy(paths, destPath, op);
  if (policy === null) {
    pushToast("キャンセルしました", "info");
    return;
  }
  const items = await resolveDestinations(paths, destPath, op, policy);
  if (items.length === 0) {
    pushToast("対象がありません (同じ場所への移動)", "info");
    return;
  }
  await preDeleteOverwrites(items);
  const renamedCount = items.filter((it) => it.renamed).length;
  const overwriteCount = items.filter((it) => it.overwrite).length;
  const verb = op === "copy" ? "コピー" : "移動";
  const label = `${verb} ${items.length}件 → ${destPath}`;
  const r = await runFileJob(op, items.map(({ from, to }) => ({ from, to })), { label });
  if (r.ok) {
    const ops: UndoOp[] = items.map((it) =>
      op === "copy"
        ? ({ kind: "copy", created: it.to } as UndoOp)
        : ({ kind: "move", from: it.from, to: it.to } as UndoOp),
    );
    pushUndo(label, ops);
    const refresh = refreshTargets(items, destPath, op === "move");
    if (sourceDir) refresh.push(sourceDir);
    bumpRefreshPaths(refresh);
    const notes: string[] = [];
    if (renamedCount > 0) notes.push(`${renamedCount}件は名前変更`);
    if (overwriteCount > 0) notes.push(`${overwriteCount}件は上書き`);
    const note = notes.length > 0 ? ` (${notes.join(", ")})` : "";
    pushToast(`${verb} ${items.length}件 完了${note}`, "info");
  } else if (!r.canceled) {
    console.error(`${logTag} ${label} 失敗`);
    pushToast(`${verb} 失敗`, "error");
  }
  // 着地ペインの refetch を呼ぶ (登録されていれば)
  getPaneRefetch(targetPaneId)?.();
}
