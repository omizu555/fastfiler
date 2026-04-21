import { setState, persist } from "./core";

export function toggleTerminal() { setState("showTerminal", (v) => !v); persist(); }
export function setTerminalHeight(h: number) { setState("terminalHeight", Math.max(80, Math.min(800, Math.round(h)))); persist(); }
export function setTerminalShell(s: string | null) { setState("terminalShell", s); persist(); }
export function setTerminalFont(f: string | null) { setState("terminalFont", f); persist(); }
export function setTerminalFontSize(n: number) { setState("terminalFontSize", Math.max(8, Math.min(36, Math.round(n)))); persist(); }
export function setUiFont(f: string | null) { setState("uiFont", f); persist(); }
export function setUiFontSize(n: number) { setState("uiFontSize", Math.max(9, Math.min(24, Math.round(n)))); persist(); }
