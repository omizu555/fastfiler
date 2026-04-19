import { Show, createResource, createSignal, onMount, onCleanup } from "solid-js";
import { getThumbnail } from "../fs";

const THUMB_EXTS = new Set([
  "png", "jpg", "jpeg", "gif", "bmp", "webp", "ico", "tiff", "tif",
  "svg", "heic", "heif",
  "mp4", "mov", "avi", "mkv", "webm",
  "pdf",
  "psd", "ai",
]);

export function shouldThumb(ext?: string | null): boolean {
  if (!ext) return false;
  return THUMB_EXTS.has(ext.toLowerCase().replace(/^\./, ""));
}

interface Props {
  path: string;
  ext?: string | null;
  size?: number;
  fallback: string;
}

export default function Thumbnail(props: Props) {
  const [visible, setVisible] = createSignal(false);
  let ref: HTMLSpanElement | undefined;

  onMount(() => {
    if (typeof IntersectionObserver === "undefined") {
      setVisible(true);
      return;
    }
    const io = new IntersectionObserver((entries) => {
      for (const e of entries) {
        if (e.isIntersecting) {
          setVisible(true);
          io.disconnect();
        }
      }
    }, { rootMargin: "100px" });
    if (ref) io.observe(ref);
    onCleanup(() => io.disconnect());
  });

  const [thumb] = createResource(
    () => (visible() && shouldThumb(props.ext) ? props.path : null),
    async (p) => {
      if (!p) return null;
      return getThumbnail(p, props.size ?? 96);
    },
  );

  return (
    <span class="thumb-cell" ref={ref}>
      <Show when={thumb()} fallback={<span class="icon">{props.fallback}</span>}>
        {(t) => <img class="thumb-img" src={t().data_url} alt="" loading="lazy" decoding="async" />}
      </Show>
    </span>
  );
}
