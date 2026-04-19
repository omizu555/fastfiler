// FastFiler Plugin SDK type definitions
export {};

declare global {
  interface FileEntry {
    name: string;
    kind: "file" | "dir";
    size?: number;
    modified?: number;
    is_hidden?: boolean;
  }

  interface PaneActiveInfo {
    paneId: string;
    path: string;
    selection: string[];
  }

  interface ContextMenuTarget {
    path: string;
    name: string;
    isDir: boolean;
  }

  interface ContextMenuItemSpec {
    id: string;
    label: string;
    icon?: string;
    when?: "file" | "dir" | "any";
    extensions?: string[];
  }

  type EventTopic =
    | "pane.changed"
    | "pane.selection.changed"
    | "plugin.activated"
    | "plugin.contextMenu.invoked";

  interface FFNamespace {
    invoke(capability: string, args?: Record<string, unknown>): Promise<unknown>;
    on(topic: EventTopic, fn: (payload: any) => void): void;
    off(topic: EventTopic, fn: (payload: any) => void): void;
    notify(message: string, level?: "info" | "warn" | "error"): Promise<void>;
    fs: {
      readDir(path: string): Promise<FileEntry[]>;
      readText(path: string): Promise<{ content: string; truncated: boolean }>;
      writeText(path: string, content: string): Promise<void>;
      mkdir(path: string, recursive?: boolean): Promise<void>;
      rename(from: string, to: string): Promise<void>;
      copy(from: string, to: string): Promise<void>;
      move(from: string, to: string): Promise<void>;
      delete(paths: string[], permanent?: boolean): Promise<void>;
      stat(path: string): Promise<FileEntry>;
    };
    pane: {
      getActive(): Promise<PaneActiveInfo | null>;
      setPath(path: string, paneId?: string): Promise<void>;
    };
    shell: {
      open(path: string): Promise<void>;
    };
    storage: {
      get(key: string): Promise<unknown>;
      set(key: string, value: unknown): Promise<void>;
    };
    registerContextMenuItem(item: ContextMenuItemSpec): Promise<void>;
  }

  var ff: FFNamespace;
  interface Window { ff: FFNamespace; }
}
