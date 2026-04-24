import { Show } from "solid-js";
import { state } from "../store";
import { cancelJob } from "../jobs";
import { formatSize } from "../fs";
import type { FileJob } from "../types";

function pct(j: FileJob): number {
  if (j.totalBytes > 0) return Math.min(100, (j.doneBytes / j.totalBytes) * 100);
  if (j.totalFiles > 0) return Math.min(100, (j.doneFiles / j.totalFiles) * 100);
  return 0;
}

function kindLabel(k: string): string {
  return k === "copy" ? "コピー" : k === "move" ? "移動" : k === "delete" ? "削除" : k;
}

/**
 * v1.9: ステータスバー内に進行中ジョブのコンパクト表示。
 * 完了したジョブは StatusBarToast 経由で別途通知される。
 */
export default function StatusBarJobs() {
  const running = () => state.activeJobs.filter((j) => j.phase !== "done");
  const head = () => running()[0];
  const others = () => Math.max(0, running().length - 1);

  return (
    <Show when={head()}>
      {(j) => (
        <div class="statusbar-jobs" title={j().label}>
          <span class="statusbar-jobs-bar">
            <span class="statusbar-jobs-bar-fill" style={{ width: pct(j()) + "%" }} />
          </span>
          <span class="statusbar-jobs-text">
            {kindLabel(j().kind)}{" "}
            <Show when={j().phase === "scan"} fallback={
              <>
                {j().doneFiles}/{j().totalFiles || "?"} ({formatSize(j().doneBytes)}
                <Show when={j().totalBytes > 0}> / {formatSize(j().totalBytes)}</Show>)
              </>
            }>スキャン中…</Show>
          </span>
          <Show when={others() > 0}>
            <span class="statusbar-jobs-more">他 {others()} 件</span>
          </Show>
          <button
            class="statusbar-jobs-cancel"
            title="キャンセル"
            onClick={() => void cancelJob(j().id)}
          >✕</button>
        </div>
      )}
    </Show>
  );
}
