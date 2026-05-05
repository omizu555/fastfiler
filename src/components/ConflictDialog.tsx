import { Show, createSignal } from "solid-js";

export type ConflictChoice = "overwrite" | "rename" | "skip" | "cancel";

interface ConflictOptions {
  /** 操作種別 (表示用): copy / move */
  op: "copy" | "move";
  /** 衝突する件数 */
  conflictCount: number;
  /** 全体件数 (情報用) */
  totalCount: number;
  /** 代表となる衝突ファイル名 (最大 3 件まで表示) */
  sampleNames: string[];
  /** 宛先ディレクトリ (情報用) */
  destDir: string;
}

interface ConflictState extends ConflictOptions {
  resolve: (v: ConflictChoice) => void;
}

const [current, setCurrent] = createSignal<ConflictState | null>(null);

export function openConflict(opts: ConflictOptions): Promise<ConflictChoice> {
  const prev = current();
  if (prev) prev.resolve("cancel");
  return new Promise((resolve) => setCurrent({ ...opts, resolve }));
}

export default function ConflictDialog() {
  const close = (choice: ConflictChoice) => {
    const c = current();
    if (!c) return;
    setCurrent(null);
    c.resolve(choice);
  };

  return (
    <Show when={current()}>
      {(c) => (
        <div class="modal-backdrop" onMouseDown={(e) => { if (e.target === e.currentTarget) close("cancel"); }}>
          <div class="modal prompt-modal" onMouseDown={(e) => e.stopPropagation()}>
            <div class="modal-head">
              <strong>{c().op === "copy" ? "コピー先" : "移動先"}に同名のファイル/フォルダがあります</strong>
            </div>
            <div class="modal-body">
              <div style={{ "margin-bottom": "8px" }}>
                {c().conflictCount} 件の衝突 (全 {c().totalCount} 件中)
              </div>
              <ul style={{ "margin": "4px 0 8px 16px", "padding": "0", "max-height": "120px", "overflow": "auto" }}>
                {c().sampleNames.slice(0, 3).map((n) => <li>{n}</li>)}
                <Show when={c().sampleNames.length > 3}>
                  <li class="muted">…他 {c().sampleNames.length - 3} 件</li>
                </Show>
              </ul>
              <div class="muted" style={{ "font-size": "12px" }}>
                宛先: {c().destDir}
              </div>
            </div>
            <div class="modal-foot">
              <button onClick={() => close("cancel")}>キャンセル</button>
              <span class="spacer" />
              <button onClick={() => close("skip")} title="衝突した項目だけスキップして残りを処理">スキップ</button>
              <button onClick={() => close("rename")} title="自動で名前を付けてコピー (例: name (2).ext)">別名でコピー</button>
              <button class="primary" onClick={() => close("overwrite")} title="既存の項目を上書き (元に戻せません)">
                上書き
              </button>
            </div>
          </div>
        </div>
      )}
    </Show>
  );
}
