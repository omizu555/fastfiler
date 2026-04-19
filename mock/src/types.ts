export type FileKind = "dir" | "file";

export interface FileEntry {
  name: string;
  kind: FileKind;
  size: number;
  modified: string;
  ext?: string;
}

export interface PaneState {
  id: string;
  path: string;
  selection: string[];
  scrollTop: number;
  linkGroupId: string | null;
}

export type SplitDir = "h" | "v";

export type PaneNode =
  | { kind: "leaf"; paneId: string }
  | { kind: "split"; dir: SplitDir; ratio: number; a: PaneNode; b: PaneNode };

export interface Tab {
  id: string;
  title: string;
  rootPane: PaneNode;
}

export type LinkChannel = "path" | "selection" | "scroll" | "sort";

export interface LinkGroup {
  id: string;
  name: string;
  color: string;
  channels: Record<LinkChannel, boolean>;
}
