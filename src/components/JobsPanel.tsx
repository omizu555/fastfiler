import { For, Show } from "solid-js";
import { state } from "../store";
import { cancelJob } from "../jobs";
import { formatSize } from "../fs";

function pct(j: import("../types").FileJob): number {
  if (j.totalBytes > 0) return Math.min(100, (j.doneBytes / j.totalBytes) * 100);
  if (j.totalFiles > 0) return Math.min(100, (j.doneFiles / j.totalFiles) * 100);
  return 0;
}

function kindLabel(k: string): string {
  return k === "copy" ? "コピー" : k === "move" ? "移動" : k === "delete" ? "削除" : k;
}

export default function JobsPanel() {
  return (
    <div class="jobs-panel">
      <For each={state.activeJobs}>
        {(j) => (
          <div class="job-card" classList={{ "job-done": j.phase === "done", "job-fail": j.phase === "done" && !j.ok }}>
            <div class="job-head">
              <span class="job-kind">{kindLabel(j.kind)}</span>
              <span class="job-label" title={j.label}>{j.label}</span>
              <Show when={j.phase !== "done"}>
                <button class="job-cancel" title="キャンセル" onClick={() => void cancelJob(j.id)}>✕</button>
              </Show>
            </div>
            <div class="job-bar">
              <div class="job-bar-fill" style={{ width: pct(j) + "%" }} />
            </div>
            <div class="job-meta">
              <Show when={j.phase === "scan"} fallback={
                <>
                  {j.doneFiles}/{j.totalFiles || "?"} ファイル ／ {formatSize(j.doneBytes)}
                  <Show when={j.totalBytes > 0}> / {formatSize(j.totalBytes)}</Show>
                  <Show when={j.phase === "done"}>
                    <span class="job-status">
                      {j.canceled ? " (キャンセル)" : j.ok ? " ✓ 完了" : ` ✗ ${j.error ?? "失敗"}`}
                    </span>
                  </Show>
                </>
              }>
                スキャン中…
              </Show>
            </div>
            <Show when={j.current && j.phase !== "done"}>
              <div class="job-current" title={j.current}>{j.current}</div>
            </Show>
          </div>
        )}
      </For>
    </div>
  );
}
