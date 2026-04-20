// v3.4: パフォーマンス計測 - listDir / search レイテンシをリングバッファに記録
export interface PerfSample {
  ts: number;
  kind: "list_dir" | "search" | "thumbnail" | "file_job";
  label: string;
  ms: number;
  count?: number;
}

const RING_SIZE = 200;
const ring: PerfSample[] = [];
let listeners: Array<() => void> = [];

export function recordPerf(sample: Omit<PerfSample, "ts">) {
  ring.push({ ...sample, ts: Date.now() });
  if (ring.length > RING_SIZE) ring.splice(0, ring.length - RING_SIZE);
  for (const l of listeners) l();
}

export function getPerfSamples(): readonly PerfSample[] {
  return ring;
}

export function subscribePerf(fn: () => void): () => void {
  listeners.push(fn);
  return () => { listeners = listeners.filter((x) => x !== fn); };
}

export function clearPerf() {
  ring.length = 0;
  for (const l of listeners) l();
}

export async function measure<T>(kind: PerfSample["kind"], label: string, fn: () => Promise<T>, count?: () => number): Promise<T> {
  const t0 = performance.now();
  try {
    const r = await fn();
    recordPerf({ kind, label, ms: performance.now() - t0, count: count?.() });
    return r;
  } catch (e) {
    recordPerf({ kind, label: label + " (error)", ms: performance.now() - t0 });
    throw e;
  }
}
