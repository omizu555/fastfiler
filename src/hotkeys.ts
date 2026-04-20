import type { HotkeyAction, HotkeyMap } from "./types";

export const defaultHotkeys: HotkeyMap = {
  open: "Enter",
  parent: "Backspace",
  refresh: "F5",
  rename: "F2",
  delete: "Delete",
  "delete-permanent": "Shift+Delete",
  "new-folder": "Ctrl+Shift+N",
  cut: "Ctrl+X",
  copy: "Ctrl+C",
  paste: "Ctrl+V",
  "select-all": "Ctrl+A",
  search: "Ctrl+F",
  "toggle-preview": "Ctrl+P",
  "toggle-plugin": "Ctrl+Shift+P",
  "open-settings": "Ctrl+,",
  "new-tab": "Ctrl+T",
  "close-tab": "Ctrl+W",
  "next-tab": "Ctrl+Tab",
  "prev-tab": "Ctrl+Shift+Tab",
  "toggle-tabs": "Ctrl+B",
  "toggle-tree": "Ctrl+Shift+E",
  "address-bar": "Ctrl+L",
  "undo": "Ctrl+Z",
};

export const hotkeyLabels: Record<HotkeyAction, string> = {
  open: "開く",
  parent: "親フォルダへ",
  refresh: "再読込",
  rename: "名前の変更",
  delete: "ゴミ箱へ",
  "delete-permanent": "完全削除",
  "new-folder": "新規フォルダ",
  cut: "切り取り",
  copy: "コピー",
  paste: "貼り付け",
  "select-all": "全選択",
  search: "検索",
  "toggle-preview": "プレビュー表示切替",
  "toggle-plugin": "プラグインパネル切替",
  "open-settings": "設定を開く",
  "new-tab": "新しいタブ",
  "close-tab": "現在のタブを閉じる",
  "next-tab": "次のタブへ",
  "prev-tab": "前のタブへ",
  "toggle-tabs": "タブサイドバー表示切替",
  "toggle-tree": "ツリーパネル表示切替",
  "address-bar": "アドレスバーを編集",
  "undo": "操作を取り消す",
};

// "Ctrl+Shift+N" -> { ctrl, shift, alt, meta, key: "N" }
interface ParsedKey {
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  meta: boolean;
  key: string; // 大文字化済み
}

export function parseHotkey(combo: string): ParsedKey | null {
  if (!combo) return null;
  const parts = combo.split("+").map((s) => s.trim()).filter(Boolean);
  if (parts.length === 0) return null;
  const out: ParsedKey = { ctrl: false, shift: false, alt: false, meta: false, key: "" };
  for (const p of parts) {
    const low = p.toLowerCase();
    if (low === "ctrl" || low === "control") out.ctrl = true;
    else if (low === "shift") out.shift = true;
    else if (low === "alt") out.alt = true;
    else if (low === "meta" || low === "win" || low === "cmd") out.meta = true;
    else out.key = p.length === 1 ? p.toUpperCase() : p;
  }
  if (!out.key) return null;
  return out;
}

function eventKey(e: KeyboardEvent): string {
  // 1 文字キーは大文字化、特殊キーはそのまま
  if (e.key.length === 1) return e.key.toUpperCase();
  return e.key;
}

export function matchKey(combo: string, e: KeyboardEvent): boolean {
  const p = parseHotkey(combo);
  if (!p) return false;
  if (e.ctrlKey !== p.ctrl) return false;
  if (e.shiftKey !== p.shift) return false;
  if (e.altKey !== p.alt) return false;
  if (e.metaKey !== p.meta) return false;
  return eventKey(e) === p.key;
}

// KeyboardEvent → "Ctrl+Shift+N" 文字列（記録用）
export function eventToCombo(e: KeyboardEvent): string | null {
  if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.shiftKey) parts.push("Shift");
  if (e.altKey) parts.push("Alt");
  if (e.metaKey) parts.push("Meta");
  parts.push(e.key.length === 1 ? e.key.toUpperCase() : e.key);
  return parts.join("+");
}
