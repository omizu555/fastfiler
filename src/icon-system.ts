// v1.14: system アイコン (Windows Shell) — invoke で取得し signal でキャッシュ
import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

type CacheVal = { url: string } | { pending: true };
const cache = new Map<string, CacheVal>();
const [version, setVersion] = createSignal(0);

function bump() { setVersion((v) => v + 1); }

function key(path: string, extOnly: boolean): string {
  return (extOnly ? "x:" : "p:") + path.toLowerCase();
}

/**
 * system アイコンを取得。未解決時は空文字を返し、内部で invoke を kick off。
 * 解決後は version() が増えるので、呼び出し側が version() を effect に
 * 含めていれば再描画される。
 */
export function requestSystemIcon(path: string, extOnly: boolean): string {
  // 依存関係を Solid の reactive system に登録
  void version();
  const k = key(path, extOnly);
  const v = cache.get(k);
  if (v && "url" in v) return v.url;
  if (v && "pending" in v) return "";
  cache.set(k, { pending: true });
  invoke<string>("system_icon", { path, extOnly, large: false })
    .then((url) => {
      if (typeof url === "string" && url.length > 0) {
        cache.set(k, { url });
      } else {
        cache.delete(k);
      }
      bump();
    })
    .catch(() => {
      cache.delete(k);
    });
  return "";
}

export function clearSystemIconCache() {
  cache.clear();
  bump();
}
