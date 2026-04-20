// v3.2: 進捗付きファイルジョブ (frontend)
//
// `runFileJob({ kind, items })` が job を生成し store.activeJobs に追加。
// Rust から "fs:job:progress" / "fs:job:done" を受信して進捗を更新。
// キャンセルは `cancelJob(jobId)`。

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { setState, state } from "./store";
import type { FileJob, FileJobKind, JobItem } from "./types";

let jobIdSeq = Date.now();
function nextJobId(): number { return ++jobIdSeq; }

let listenersInitialized = false;
async function ensureListeners() {
  if (listenersInitialized) return;
  listenersInitialized = true;
  try {
    await listen<{
      job_id: number; kind: string; phase: string;
      total_files: number; done_files: number;
      total_bytes: number; done_bytes: number; current: string;
    }>("fs:job:progress", (ev) => {
      const j = ev.payload;
      setState("activeJobs", (xs) => xs.map((x) => x.id === j.job_id ? {
        ...x,
        phase: j.phase as FileJob["phase"],
        totalFiles: j.total_files,
        doneFiles: j.done_files,
        totalBytes: j.total_bytes,
        doneBytes: j.done_bytes,
        current: j.current,
      } : x));
    });
    await listen<{
      job_id: number; kind: string; ok: boolean; canceled: boolean;
      error: string | null;
      total_files: number; done_files: number;
      total_bytes: number; done_bytes: number;
    }>("fs:job:done", (ev) => {
      const j = ev.payload;
      setState("activeJobs", (xs) => xs.map((x) => x.id === j.job_id ? {
        ...x,
        phase: "done",
        ok: j.ok,
        canceled: j.canceled,
        error: j.error,
        totalFiles: j.total_files,
        doneFiles: j.done_files,
        totalBytes: j.total_bytes,
        doneBytes: j.done_bytes,
        finishedAt: Date.now(),
      } : x));
      // 5 秒後に自動で消す
      window.setTimeout(() => {
        setState("activeJobs", (xs) => xs.filter((x) => x.id !== j.job_id));
      }, 5000);
    });
  } catch {/* non-tauri */}
}

export interface RunJobOpts {
  label: string;
}

export async function runFileJob(
  kind: FileJobKind,
  items: JobItem[] | string[],
  opts: RunJobOpts,
): Promise<{ ok: boolean; canceled: boolean; jobId: number }> {
  await ensureListeners();
  const jobId = nextJobId();
  const job: FileJob = {
    id: jobId, kind, label: opts.label,
    phase: "scan",
    totalFiles: 0, doneFiles: 0, totalBytes: 0, doneBytes: 0,
    current: "", startedAt: Date.now(),
    ok: false, canceled: false, error: null,
  };
  setState("activeJobs", (xs) => [...xs, job]);
  const cmd = kind === "copy" ? "job_copy" : kind === "move" ? "job_move" : "job_delete";
  const args: Record<string, unknown> = kind === "delete"
    ? { jobId, paths: items as string[] }
    : { jobId, items: items as JobItem[] };
  try {
    await invoke(cmd, args);
    return { ok: true, canceled: false, jobId };
  } catch (e) {
    const cur = state.activeJobs.find((x) => x.id === jobId);
    const canceled = !!cur?.canceled;
    return { ok: false, canceled, jobId };
  }
}

export async function cancelJob(jobId: number): Promise<void> {
  try { await invoke("cancel_job", { jobId }); } catch {/* ignore */}
  setState("activeJobs", (xs) => xs.map((x) => x.id === jobId ? { ...x, canceled: true } : x));
}
