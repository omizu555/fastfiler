// v1.10: 読み込み中 (listDir 実行中) のペイン数を集約。
//        ステータスバーに「読み込み中...」を出すための共有 signal。
import { createSignal } from "solid-js";

const [loadingCount, setLoadingCount] = createSignal(0);

export const isAnyLoading = () => loadingCount() > 0;

export function beginLoading(): void {
  setLoadingCount(loadingCount() + 1);
}

export function endLoading(): void {
  setLoadingCount(Math.max(0, loadingCount() - 1));
}
