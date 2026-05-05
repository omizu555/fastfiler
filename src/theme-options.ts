// 設定 UI の テーマ/アイコン 選択肢を一箇所にまとめたテーブル。
// GeneralTab の select はこのテーブルから option を生成する。
// `value` は select の value、`apply` で対応する store 状態を設定する。
import type { ThemeMode, ThemePresetId, IconSet, IconPackId } from "./types";

export interface ThemeChoice {
  value: string;
  label: string;
  group: "基本" | "プリセット (ライト)" | "プリセット (ダーク)";
  theme: ThemeMode;
  preset: ThemePresetId;
}

export const THEME_CHOICES: readonly ThemeChoice[] = [
  { value: "system|default",        label: "OS依存",          group: "基本",                    theme: "system", preset: "default" },
  { value: "light|default",         label: "☀ ライト",        group: "基本",                    theme: "light",  preset: "default" },
  { value: "dark|default",          label: "🌙 ダーク",        group: "基本",                    theme: "dark",   preset: "default" },
  { value: "light|githubLight",     label: "GitHub Light",    group: "プリセット (ライト)",       theme: "light",  preset: "githubLight" },
  { value: "light|solarizedLight",  label: "Solarized Light", group: "プリセット (ライト)",       theme: "light",  preset: "solarizedLight" },
  { value: "dark|githubDark",       label: "GitHub Dark",     group: "プリセット (ダーク)",       theme: "dark",   preset: "githubDark" },
  { value: "dark|solarizedDark",    label: "Solarized Dark",  group: "プリセット (ダーク)",       theme: "dark",   preset: "solarizedDark" },
  { value: "dark|dracula",          label: "Dracula",         group: "プリセット (ダーク)",       theme: "dark",   preset: "dracula" },
  { value: "dark|nord",             label: "Nord",            group: "プリセット (ダーク)",       theme: "dark",   preset: "nord" },
  { value: "dark|monokai",          label: "Monokai",         group: "プリセット (ダーク)",       theme: "dark",   preset: "monokai" },
  { value: "dark|tokyoNight",       label: "Tokyo Night",     group: "プリセット (ダーク)",       theme: "dark",   preset: "tokyoNight" },
  { value: "dark|gruvboxDark",      label: "Gruvbox Dark",    group: "プリセット (ダーク)",       theme: "dark",   preset: "gruvboxDark" },
];

export interface IconChoice {
  value: string;
  label: string;
  group: "基本" | "パック";
  iconSet: IconSet;
  iconPack: IconPackId;
}

export const ICON_CHOICES: readonly IconChoice[] = [
  { value: "emoji|default",    label: "📁 既定 (絵文字)",         group: "基本", iconSet: "emoji",   iconPack: "default" },
  { value: "colored|default",  label: "🎨 拡張子別",              group: "基本", iconSet: "colored", iconPack: "default" },
  { value: "minimal|default",  label: "▸ ミニマル",               group: "基本", iconSet: "minimal", iconPack: "default" },
  { value: "colored|emoji",    label: "Emoji (リッチ)",           group: "パック", iconSet: "colored", iconPack: "emoji" },
  { value: "colored|material", label: "Material (色ブロック)",    group: "パック", iconSet: "colored", iconPack: "material" },
  { value: "colored|vscode",   label: "VSCode (Seti 風)",         group: "パック", iconSet: "colored", iconPack: "vscode" },
  { value: "minimal|mono",     label: "Mono (モノクロ記号)",      group: "パック", iconSet: "minimal", iconPack: "mono" },
  { value: "colored|system",   label: "System (Windows シェル)", group: "パック", iconSet: "colored", iconPack: "system" },
];

/** value 文字列から ThemeChoice / IconChoice を一意に解決するテーブル */
const THEME_BY_VALUE = new Map(THEME_CHOICES.map((c) => [c.value, c]));
const ICON_BY_VALUE = new Map(ICON_CHOICES.map((c) => [c.value, c]));

export function findThemeChoice(value: string): ThemeChoice | undefined {
  return THEME_BY_VALUE.get(value);
}

export function findIconChoice(value: string): IconChoice | undefined {
  return ICON_BY_VALUE.get(value);
}

/** 同じ group の選択肢でグルーピングして返す (optgroup 出力用) */
export function groupBy<T extends { group: string }>(choices: readonly T[]): Array<[string, T[]]> {
  const m = new Map<string, T[]>();
  for (const c of choices) {
    const arr = m.get(c.group);
    if (arr) arr.push(c);
    else m.set(c.group, [c]);
  }
  return [...m.entries()];
}
