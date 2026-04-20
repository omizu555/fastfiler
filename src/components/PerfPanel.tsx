import { For, Show, createSignal, onCleanup, onMount, createMemo } from "solid-js";
import { getPerfSamples, subscribePerf, clearPerf, type PerfSample } from "../perf";

export default function PerfPanel() {
  const [tick, setTick] = createSignal(0);
  onMount(() => {
    const off = subscribePerf(() => setTick((n) => n + 1));
    onCleanup(off);
  });

  const samples = createMemo(() => {
    tick();
    return [...getPerfSamples()].reverse();
  });

  const stats = createMemo(() => {
    tick();
    const all = getPerfSamples();
    const byKind: Record<string, { n: number; total: number; max: number }> = {};
    for (const s of all) {
      const b = byKind[s.kind] ?? (byKind[s.kind] = { n: 0, total: 0, max: 0 });
      b.n++;
      b.total += s.ms;
      if (s.ms > b.max) b.max = s.ms;
    }
    return Object.entries(byKind).map(([k, v]) => ({
      kind: k,
      n: v.n,
      avg: v.n > 0 ? v.total / v.n : 0,
      max: v.max,
    }));
  });

  const fmt = (ms: number) => ms < 10 ? ms.toFixed(2) : ms.toFixed(1);
  const fmtTs = (ts: number) => {
    const d = new Date(ts);
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${String(d.getMilliseconds()).padStart(3, "0")}`;
  };

  return (
    <div>
      <p class="muted small" style={{ "margin": "0 0 8px" }}>
        listDir / 検索 / サムネイル / ジョブの実行時間 (直近 200 件) を計測します。
      </p>

      <table class="hk-table" style={{ "margin-bottom": "12px" }}>
        <thead><tr><th>種類</th><th>件数</th><th>平均 (ms)</th><th>最大 (ms)</th></tr></thead>
        <tbody>
          <For each={stats()} fallback={<tr><td colspan="4" class="muted">計測データなし</td></tr>}>
            {(s) => (
              <tr>
                <td>{s.kind}</td>
                <td class="muted small">{s.n}</td>
                <td>{fmt(s.avg)}</td>
                <td classList={{ "perf-warn": s.max > 200 }}>{fmt(s.max)}</td>
              </tr>
            )}
          </For>
        </tbody>
      </table>

      <div style={{ "display": "flex", "align-items": "center", "gap": "8px", "margin-bottom": "6px" }}>
        <strong style={{ "font-size": "12px" }}>直近のサンプル</strong>
        <span style={{ "flex": "1" }} />
        <button onClick={clearPerf}>クリア</button>
      </div>

      <div style={{ "max-height": "300px", "overflow": "auto", "border": "1px solid var(--border)", "border-radius": "4px" }}>
        <table class="hk-table" style={{ "margin": "0" }}>
          <thead><tr><th>時刻</th><th>種類</th><th>ms</th><th>件数</th><th>ラベル</th></tr></thead>
          <tbody>
            <Show when={samples().length > 0} fallback={<tr><td colspan="5" class="muted" style={{ "padding": "12px" }}>サンプルなし</td></tr>}>
              <For each={samples()}>
                {(s: PerfSample) => (
                  <tr>
                    <td class="muted small" style={{ "white-space": "nowrap" }}>{fmtTs(s.ts)}</td>
                    <td class="muted small">{s.kind}</td>
                    <td classList={{ "perf-warn": s.ms > 200 }}>{fmt(s.ms)}</td>
                    <td class="muted small">{s.count ?? ""}</td>
                    <td class="muted small" style={{ "max-width": "300px", "overflow": "hidden", "text-overflow": "ellipsis", "white-space": "nowrap" }} title={s.label}>{s.label}</td>
                  </tr>
                )}
              </For>
            </Show>
          </tbody>
        </table>
      </div>
    </div>
  );
}
