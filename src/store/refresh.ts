// 任意パスの再描画通知 (非永続化)。
// FileList などはこの tick を resource source に含めることで
// move/copy/delete/rename 完了時に自分の表示パスを refetch できる。
import { createSignal, type Accessor, type Setter } from "solid-js";
import { normalizePath } from "../path-util";

const signals = new Map<string, [Accessor<number>, Setter<number>]>();

function ensure(key: string): [Accessor<number>, Setter<number>] {
  let s = signals.get(key);
  if (!s) {
    s = createSignal(0);
    signals.set(key, s);
  }
  return s;
}

export function refreshTickFor(path: string): number {
  return ensure(normalizePath(path))[0]();
}

export function bumpRefreshPath(path: string): void {
  const [, set] = ensure(normalizePath(path));
  set((v) => v + 1);
}

export function bumpRefreshPaths(paths: Iterable<string>): void {
  const seen = new Set<string>();
  for (const p of paths) {
    const k = normalizePath(p);
    if (seen.has(k)) continue;
    seen.add(k);
    const [, set] = ensure(k);
    set((v) => v + 1);
  }
}
