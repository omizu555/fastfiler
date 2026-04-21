// 設定: プリセットタブ (ワークスペースの保存/読込/エクスポート/インポート)
import { For, Show, createSignal } from "solid-js";
import {
  state,
  savePreset,
  applyPreset,
  deletePreset,
  renamePreset,
  exportPresetsJson,
  importPresetsJson,
} from "../../store";

const formatTs = (ts: number) => {
  const d = new Date(ts);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
};

export default function PresetsTab() {
  const [newPresetName, setNewPresetName] = createSignal("");
  const [importText, setImportText] = createSignal("");
  const [showImport, setShowImport] = createSignal(false);

  const doSavePreset = () => {
    const name = newPresetName().trim() || `プリセット ${state.presets.length + 1}`;
    savePreset(name);
    setNewPresetName("");
  };

  const doExport = async () => {
    const json = exportPresetsJson();
    try {
      await navigator.clipboard.writeText(json);
      alert(`${state.presets.length} 件のプリセットをクリップボードにコピーしました`);
    } catch {
      window.prompt("プリセット JSON (Ctrl+C でコピー):", json);
    }
  };

  const doImport = (mode: "merge" | "replace") => {
    try {
      const n = importPresetsJson(importText(), mode);
      alert(`${n} 件のプリセットを取り込みました`);
      setImportText("");
      setShowImport(false);
    } catch (e) {
      alert(`取り込みエラー: ${e}`);
    }
  };

  return (
    <>
      <p class="muted small" style={{ "margin": "0 0 8px" }}>
        現在のタブ・ペイン・パネル配置をスナップショットとして保存し、後で一括復元できます。
      </p>

      <div class="setting-row">
        <input
          type="text"
          placeholder="プリセット名 (例: コーディング用 / 写真整理用)"
          value={newPresetName()}
          onInput={(e) => setNewPresetName(e.currentTarget.value)}
          style={{ "flex": "1" }}
        />
        <button onClick={doSavePreset} style={{ "margin-left": "8px" }}>💾 現在の配置を保存</button>
      </div>

      <Show
        when={state.presets.length > 0}
        fallback={<div class="muted" style={{ "padding": "16px 0" }}>保存済みプリセットはまだありません</div>}
      >
        <table class="hk-table">
          <thead>
            <tr><th>名前</th><th>保存日時</th><th>タブ数</th><th></th></tr>
          </thead>
          <tbody>
            <For each={state.presets}>
              {(p) => (
                <tr>
                  <td>{p.name}</td>
                  <td class="muted small">{formatTs(p.savedAt)}</td>
                  <td class="muted small">{p.snapshot.tabs.length}</td>
                  <td style={{ "text-align": "right", "white-space": "nowrap" }}>
                    <button onClick={() => {
                      if (confirm(`「${p.name}」を適用しますか？\n現在のタブ・ペイン構成は失われます。`)) applyPreset(p.id);
                    }}>適用</button>
                    <button style={{ "margin-left": "4px" }} onClick={() => {
                      const n = prompt("新しい名前", p.name);
                      if (n != null) renamePreset(p.id, n);
                    }}>名称変更</button>
                    <button style={{ "margin-left": "4px" }} onClick={() => {
                      if (confirm(`「${p.name}」を削除しますか？`)) deletePreset(p.id);
                    }}>削除</button>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </Show>

      <hr />
      <div class="setting-row">
        <label>共有 / バックアップ</label>
        <button onClick={doExport}>📤 エクスポート (クリップボード)</button>
        <button style={{ "margin-left": "8px" }} onClick={() => setShowImport((v) => !v)}>
          📥 インポート…
        </button>
      </div>

      <Show when={showImport()}>
        <div class="setting-row" style={{ "flex-direction": "column", "align-items": "stretch" }}>
          <textarea
            rows={6}
            placeholder="エクスポートした JSON を貼り付け"
            value={importText()}
            onInput={(e) => setImportText(e.currentTarget.value)}
            style={{ "width": "100%", "font-family": "monospace", "font-size": "12px" }}
          />
          <div style={{ "margin-top": "6px", "text-align": "right" }}>
            <button onClick={() => doImport("merge")}>追加で取り込む</button>
            <button style={{ "margin-left": "8px" }} onClick={() => {
              if (confirm("既存プリセットをすべて置き換えます。よろしいですか？")) doImport("replace");
            }}>置き換え取り込み</button>
          </div>
        </div>
      </Show>
    </>
  );
}
