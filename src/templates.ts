// 新規ファイル テンプレート (v1.11)
//
// 内蔵テンプレ + ユーザー定義テンプレ (%APPDATA%\fastfiler\templates)。
// 内蔵は空ファイルを作るだけ (テンプレ ファイル不要)、ユーザー定義は
// バックエンドの create_file_from_template でコピー作成する。

import type { TemplateInfo } from "./types";

let _invoke: (<T = unknown>(cmd: string, args?: Record<string, unknown>) => Promise<T>) | null = null;
async function invoke<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!_invoke) {
    const m = await import("@tauri-apps/api/core");
    _invoke = m.invoke as never;
  }
  return _invoke!<T>(cmd, args);
}

export interface BuiltinTemplate {
  id: string;
  label: string; // メニュー表示
  fileName: string; // 既定ファイル名 (例: "新しいテキスト.txt")
  icon?: string;
  body?: string; // 初期内容 (空なら空ファイル)
}

export const BUILTIN_TEMPLATES: BuiltinTemplate[] = [
  { id: "blank", label: "空のファイル", fileName: "新しいファイル", icon: "📄" },
  { id: "txt", label: "テキスト", fileName: "新しいテキスト.txt", icon: "📝" },
  { id: "md", label: "Markdown", fileName: "新規ドキュメント.md", icon: "📝", body: "# \n" },
  { id: "json", label: "JSON", fileName: "新規.json", icon: "📜", body: "{\n  \n}\n" },
  { id: "yml", label: "YAML", fileName: "新規.yml", icon: "📜" },
  { id: "html", label: "HTML", fileName: "新規.html", icon: "🌐",
    body: "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title></title>\n</head>\n<body>\n\n</body>\n</html>\n" },
  { id: "css", label: "CSS", fileName: "新規.css", icon: "🎨" },
  { id: "js", label: "JavaScript", fileName: "新規.js", icon: "📜" },
  { id: "ts", label: "TypeScript", fileName: "新規.ts", icon: "📜" },
  { id: "py", label: "Python", fileName: "新規.py", icon: "🐍" },
  { id: "bat", label: "バッチ (.bat)", fileName: "新規.bat", icon: "📜", body: "@echo off\r\n" },
  { id: "ps1", label: "PowerShell", fileName: "新規.ps1", icon: "📜" },
  { id: "gitignore", label: ".gitignore", fileName: ".gitignore", icon: "📝" },
];

export async function templatesDirPath(): Promise<string> {
  return await invoke<string>("templates_dir");
}

export async function listUserTemplates(): Promise<TemplateInfo[]> {
  try {
    return await invoke<TemplateInfo[]>("list_templates");
  } catch {
    return [];
  }
}

export async function createEmptyFile(destDir: string, fileName: string, body?: string): Promise<string> {
  return await invoke<string>("create_empty_file", { destDir, fileName, body: body ?? null });
}

export async function createFromTemplate(
  templatePath: string,
  destDir: string,
  fileName?: string,
): Promise<string> {
  return await invoke<string>("create_file_from_template", {
    templatePath, destDir, fileName: fileName ?? null,
  });
}

/** 内蔵テンプレ作成: body が指定されていればその内容で書き込む */
export async function createBuiltin(t: BuiltinTemplate, destDir: string): Promise<string> {
  return await createEmptyFile(destDir, t.fileName, t.body);
}

// ---- ユーザー テンプレート キャッシュ (UI 同期用) ----
import { createSignal } from "solid-js";
const [userTemplates, setUserTemplates] = createSignal<TemplateInfo[]>([]);
export { userTemplates };

export async function refreshUserTemplates(): Promise<void> {
  const list = await listUserTemplates();
  setUserTemplates(list);
}
