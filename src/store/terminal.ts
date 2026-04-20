import { setState, persist } from "./core";

export function toggleTerminal() { setState("showTerminal", (v) => !v); persist(); }
export function setTerminalHeight(h: number) { setState("terminalHeight", Math.max(80, Math.min(800, Math.round(h)))); persist(); }
export function setTerminalShell(s: string | null) { setState("terminalShell", s); persist(); }
export function setTerminalFont(f: string | null) { setState("terminalFont", f); persist(); }
export function setTerminalFontSize(n: number) { setState("terminalFontSize", Math.max(8, Math.min(36, Math.round(n)))); persist(); }
