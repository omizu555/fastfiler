import { Show, createMemo, createResource } from "solid-js";
import { formatSize, getThumbnail, readTextPreview } from "../fs";
import { state, togglePreview } from "../store";
import { shouldThumb } from "./Thumbnail";
import type { FileEntry } from "../types";

const TEXT_EXTS = new Set([
  "txt", "md", "log", "json", "xml", "yaml", "yml", "toml", "ini", "cfg",
  "html", "htm", "css", "scss", "less",
  "js", "ts", "tsx", "jsx", "mjs", "cjs",
  "rs", "go", "py", "rb", "java", "c", "cpp", "h", "hpp", "cs",
  "ps1", "sh", "bat", "cmd",
  "csv", "tsv", "sql",
]);

const IMAGE_EXTS = new Set(["png", "jpg", "jpeg", "gif", "bmp", "webp", "ico", "svg"]);

function ext(name: string): string {
  const i = name.lastIndexOf(".");
  return i >= 0 ? name.substring(i + 1).toLowerCase() : "";
}

interface Props {
  paneId: string;
}

export default function PreviewPane(props: Props) {
  const pane = () => state.panes[props.paneId];

  // 現在のペインの「最後に選択された 1 件」を取得
  const targetPath = createMemo<{ path: string; entry: FileEntry } | null>(() => {
    const p = pane();
    if (!p || p.selection.length === 0) return null;
    const name = p.selection[p.selection.length - 1];
    const e: FileEntry = { name, kind: "file", size: 0, modified: 0, ext: ext(name) };
    return { path: p.path.endsWith("\\") ? p.path + name : p.path + "\\" + name, entry: e };
  });

  const [thumb] = createResource(
    () => {
      const t = targetPath();
      if (!t) return null;
      if (!shouldThumb(t.entry.ext)) return null;
      return t.path;
    },
    async (p) => (p ? getThumbnail(p, 320) : null),
  );

  const [text] = createResource(
    () => {
      const t = targetPath();
      if (!t) return null;
      const e = t.entry.ext ?? "";
      if (!TEXT_EXTS.has(e)) return null;
      return t.path;
    },
    async (p) => (p ? readTextPreview(p, 256 * 1024) : null),
  );

  const isImage = () => {
    const t = targetPath();
    return t && t.entry.ext && IMAGE_EXTS.has(t.entry.ext);
  };

  return (
    <aside class="preview-pane">
      <header class="preview-head">
        <strong>プレビュー</strong>
        <span class="spacer" />
        <button onClick={togglePreview} title="プレビューを閉じる">×</button>
      </header>
      <Show when={targetPath()} fallback={<div class="empty muted">ファイルを選択してください</div>}>
        {(t) => (
          <div class="preview-body">
            <div class="preview-name" title={t().path}>{t().entry.name}</div>
            <Show when={isImage() || thumb()}>
              <Show when={thumb()}>
                {(th) => (
                  <img class="preview-image" src={th().data_url} alt="" />
                )}
              </Show>
            </Show>
            <Show when={text()}>
              {(td) => (
                <Show when={td().kind === "text"}
                  fallback={
                    <Show when={td().kind === "binary"}>
                      <pre class="preview-binary">{(td() as { hex: string }).hex}</pre>
                      <small class="muted">バイナリ ({formatSize((td() as { size: number }).size)})</small>
                    </Show>
                  }
                >
                  <pre class="preview-text">{(td() as { content: string }).content}</pre>
                  <Show when={(td() as { truncated: boolean }).truncated}>
                    <small class="muted">… (先頭 256KB のみ表示)</small>
                  </Show>
                </Show>
              )}
            </Show>
            <Show when={!thumb() && !text() && !isImage()}>
              <div class="muted">プレビュー対応外のファイル</div>
            </Show>
          </div>
        )}
      </Show>
      <footer class="preview-foot muted">
        {pane().selection.length}件選択
        <Show when={text()?.kind === "text"}>
          <span> ／ {(text() as { encoding: string }).encoding}</span>
        </Show>
      </footer>
    </aside>
  );
}
