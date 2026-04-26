// ユーザー定義コマンド (v1.13)
// %APPDATA%\fastfiler\commands\commands.json から読み込んで右クリックメニューに差し込む。

import { createSignal } from "solid-js";

let _invoke: (<T = unknown>(cmd: string, args?: Record<string, unknown>) => Promise<T>) | null = null;
async function invoke<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!_invoke) {
    const m = await import("@tauri-apps/api/core");
    _invoke = m.invoke as never;
  }
  return _invoke!<T>(cmd, args);
}

export type UserCommandWhen = "file" | "folder" | "any" | "background";

export interface UserCommand {
  id: string;
  label: string;
  icon?: string | null;
  exec: string;
  args: string[];
  cwd?: string | null;
  when: UserCommandWhen;
  extensions: string[];
  submenu?: string | null;
  shell: boolean;
  hidden: boolean;
}

export async function userCommandsDir(): Promise<string> {
  return await invoke<string>("user_commands_dir");
}

export async function fetchUserCommands(): Promise<UserCommand[]> {
  try {
    return await invoke<UserCommand[]>("list_user_commands");
  } catch (e) {
    console.warn("[user-commands] list failed:", e);
    return [];
  }
}

export async function runUserCommand(id: string, paths: string[], cwd: string): Promise<void> {
  await invoke<void>("run_user_command", { id, ctx: { paths, cwd } });
}

const [userCommands, setUserCommands] = createSignal<UserCommand[]>([]);
const [userCommandsError, setUserCommandsError] = createSignal<string | null>(null);
export { userCommands, userCommandsError };

export async function refreshUserCommands(): Promise<void> {
  try {
    const list = await invoke<UserCommand[]>("list_user_commands");
    setUserCommands(list);
    setUserCommandsError(null);
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    setUserCommandsError(msg);
    setUserCommands([]);
  }
}
