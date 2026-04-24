// D&D / 右ドラッグ / コピペ から共通で呼ばれる「実行層」
// 衝突解決 → ジョブ実行 → undo 登録 → リフレッシュ → トースト
import { runFileJob } from "../jobs";
import { pushUndo, bumpRefreshPaths, pushToast } from "../store";
import type { UndoOp } from "../types";
import { resolveDestinations, refreshTargets, type DestOp } from "./resolve-dest";
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

export async function performDrop(input: PerformDropInput): Promise<void> {
  const { paths, destPath, op, sourceDir, targetPaneId, logTag = "[dnd]" } = input;
  const items = await resolveDestinations(paths, destPath, op);
  if (items.length === 0) {
    pushToast("対象がありません (同じ場所への移動)", "info");
    return;
  }
  const renamedCount = items.filter((it) => it.renamed).length;
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
    const note = renamedCount > 0 ? ` (${renamedCount}件は名前変更)` : "";
    pushToast(`${verb} ${items.length}件 完了${note}`, "info");
  } else if (!r.canceled) {
    console.error(`${logTag} ${label} 失敗`);
    pushToast(`${verb} 失敗`, "error");
  }
  // 着地ペインの refetch を呼ぶ (登録されていれば)
  getPaneRefetch(targetPaneId)?.();
}
