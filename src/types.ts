export type FileKind = "dir" | "file" | "symlink";

export interface FileEntry {
  name: string;
  kind: FileKind;
  size: number;
  modified: number; // unix seconds
  ext?: string | null;
  hidden?: boolean;
  readonly?: boolean;
}

export interface DriveInfo {
  letter: string;
  label: string;
}

export interface PaneState {
  id: string;
  path: string;
  selection: string[];
  scrollTop: number;
  linkGroupId: string | null;
  view?: "list" | "tree"; // v1.1: ペイン表示モード
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

export interface ThumbnailResult {
  data_url: string;
  width: number;
  height: number;
}

export type PreviewData =
  | { kind: "text"; content: string; truncated: boolean; encoding: string }
  | { kind: "binary"; hex: string; size: number }
  | { kind: "empty" };

export interface SearchHit {
  job_id: number;
  path: string;
  name: string;
  is_dir: boolean;
}

export interface SearchDoneInfo {
  job_id: number;
  total: number;
  canceled: boolean;
  backend: string;
  fallback: boolean;
  error?: string | null;
}

export type SearchBackend = "builtin" | "everything";

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  entry: string;
  description?: string;
  capabilities: string[];
}

export interface PluginInfo {
  manifest: PluginManifest;
  dir: string;
  entry_path: string;
}

export type WorkspaceLayout = "tabsLeft" | "tabsRight" | "tabsHidden";

export interface WorkspaceState {
  layout: WorkspaceLayout;
  showTree: boolean;
  tabsWidth: number;
  treeWidth: number;
  treeApply: "active" | "red" | "blue";
}

export type HotkeyAction =
  | "open"
  | "parent"
  | "refresh"
  | "rename"
  | "delete"
  | "delete-permanent"
  | "new-folder"
  | "cut"
  | "copy"
  | "paste"
  | "select-all"
  | "search"
  | "toggle-preview"
  | "toggle-plugin"
  | "open-settings"
  | "new-tab"
  | "close-tab"
  | "next-tab"
  | "prev-tab"
  | "toggle-tabs"
  | "toggle-tree";

export type HotkeyMap = Record<HotkeyAction, string>;

// v1.2: ペイン単位の一時 UI 状態（タブ切替で保持される）
export interface PaneUiState {
  searchOpen: boolean;
  searchQuery: string;
  searchCaseSensitive: boolean;
  searchRegex: boolean;
  searchFocusTick: number;
}

export function defaultPaneUi(): PaneUiState {
  return {
    searchOpen: false,
    searchQuery: "",
    searchCaseSensitive: false,
    searchRegex: false,
    searchFocusTick: 0,
  };
}
