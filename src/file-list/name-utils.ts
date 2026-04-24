// 名前検証 / 重複名生成 / プロンプトの validate を共有
export function invalidNameMessage(s: string, existing: ReadonlySet<string>): string | null {
  const t = s.trim();
  if (!t) return "名前を入力してください";
  if (/[\\/:*?"<>|]/.test(t)) return "使用できない文字が含まれています";
  if (existing.has(t)) return "同名の項目が既に存在します";
  return null;
}

// "新しいフォルダー" → "新しいフォルダー (2)" の様に既存と衝突しない名前を生成
export function uniqueName(base: string, existing: ReadonlySet<string>): string {
  if (!existing.has(base)) return base;
  let i = 2;
  while (existing.has(`${base} (${i})`)) i++;
  return `${base} (${i})`;
}

// 拡張子を保ったまま重複名を解決:
//   "foo.txt" + 衝突 → "foo (2).txt"
//   "foo"     + 衝突 → "foo (2)"
//   ".env"    + 衝突 → ".env (2)"  (先頭ドットのみは拡張子扱いにしない)
export function uniqueNameWithExt(base: string, existing: ReadonlySet<string>): string {
  if (!existing.has(base)) return base;
  // 先頭ドットを除いた中で最後のドット以降を拡張子とみなす
  const lastDot = base.lastIndexOf(".");
  const hasExt = lastDot > 0 && lastDot < base.length - 1;
  const stem = hasExt ? base.slice(0, lastDot) : base;
  const ext = hasExt ? base.slice(lastDot) : "";
  let i = 2;
  while (existing.has(`${stem} (${i})${ext}`)) i++;
  return `${stem} (${i})${ext}`;
}
