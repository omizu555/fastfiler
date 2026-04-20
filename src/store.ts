// store バレル: 旧 src/store.ts の API を再エクスポート。
// 実体は src/store/ 配下の機能別ファイルに分割されている。
// 既存の `import { ... } from "../store"` を変更せずに済むよう、ここで全てを公開する。
export { state, setState, persist } from "./store/core";
export * from "./store/tabs";
export * from "./store/panes";
export * from "./store/dock";
export * from "./store/workspace";
export * from "./store/settings";
export * from "./store/clipboard";
export * from "./store/terminal";
export * from "./store/presets";
export * from "./store/plugins";
export * from "./store/toasts";
export * from "./store/undo";
