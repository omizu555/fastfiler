// 設定ダイアログの「プラグイン」タブ
// (元: SettingsDialog.tsx に inline 定義されていたものを切り出し)
import { For, Show, createSignal, onMount } from "solid-js";
import { listPluginsWithStatus, importPluginZip, deletePlugin, pluginsDirPath, revealInExplorer } from "../../fs";
import { setPluginEnabled, isPluginEnabled } from "../../store";
import type { PluginStatus } from "../../types";

export default function PluginsTab() {
  const [items, setItems] = createSignal<PluginStatus[]>([]);
  const [busy, setBusy] = createSignal(false);
  const [msg, setMsg] = createSignal<string>("");

  const reload = async () => {
    setItems(await listPluginsWithStatus());
  };
  onMount(() => { void reload(); });

  const onImport = async () => {
    setBusy(true); setMsg("");
    try {
      const m = await import("@tauri-apps/plugin-dialog");
      const sel = await m.open({ multiple: false, filters: [{ name: "Plugin ZIP", extensions: ["zip"] }] });
      if (typeof sel === "string") {
        const id = await importPluginZip(sel);
        setMsg(`インポートしました: ${id}`);
        await reload();
      }
    } catch (e) {
      setMsg(`インポート失敗: ${e}`);
    } finally { setBusy(false); }
  };

  const onDelete = async (id: string | null | undefined) => {
    if (!id) return;
    if (!confirm(`プラグイン "${id}" を削除しますか？\n(フォルダを完全に削除します)`)) return;
    setBusy(true); setMsg("");
    try {
      await deletePlugin(id);
      setPluginEnabled(id, false);
      setMsg(`削除しました: ${id}`);
      await reload();
    } catch (e) {
      setMsg(`削除失敗: ${e}`);
    } finally { setBusy(false); }
  };

  const onReloadAll = async () => {
    setBusy(true); setMsg("");
    try {
      await reload();
      setMsg("再読込しました (iframe を更新するにはプラグインパネルを再オープンしてください)");
    } finally { setBusy(false); }
  };

  return (
    <div class="settings-section">
      <div class="setting-row">
        <button onClick={onImport} disabled={busy()}>📦 ZIP からインポート…</button>
        <button onClick={onReloadAll} disabled={busy()}>⟳ 再読込</button>
        <button onClick={async () => { try { await revealInExplorer(await pluginsDirPath()); } catch (e) { alert(`${e}`); } }}>📂 フォルダを開く</button>
        <small class="muted">{msg()}</small>
      </div>
      <table class="plugin-table" style={{ width: "100%", "border-collapse": "collapse", "margin-top": "8px" }}>
        <thead>
          <tr style={{ "text-align": "left", "border-bottom": "1px solid var(--border)" }}>
            <th style={{ padding: "4px" }}>有効</th>
            <th style={{ padding: "4px" }}>名前 / ID</th>
            <th style={{ padding: "4px" }}>Ver</th>
            <th style={{ padding: "4px" }}>capabilities</th>
            <th style={{ padding: "4px" }}>状態</th>
            <th style={{ padding: "4px" }}></th>
          </tr>
        </thead>
        <tbody>
          <For each={items()}>
            {(p) => (
              <tr style={{ "border-bottom": "1px solid var(--border)" }}>
                <td style={{ padding: "4px" }}>
                  <Show when={p.id} fallback={<span class="muted">—</span>}>
                    <input
                      type="checkbox"
                      checked={p.id ? isPluginEnabled(p.id) : false}
                      onChange={(e) => p.id && setPluginEnabled(p.id, e.currentTarget.checked)}
                      disabled={!!p.error || !p.manifest}
                    />
                  </Show>
                </td>
                <td style={{ padding: "4px" }}>
                  <div>{p.manifest?.name ?? "(no name)"}</div>
                  <small class="muted">{p.id ?? p.dir}</small>
                </td>
                <td style={{ padding: "4px" }}>{p.manifest?.version ?? "—"}</td>
                <td style={{ padding: "4px", "font-size": "11px" }}>
                  <span class="muted">{(p.manifest?.capabilities ?? []).join(", ") || "—"}</span>
                </td>
                <td style={{ padding: "4px", color: p.error ? "var(--danger, #c66)" : "var(--ok, #6c6)" }}>
                  {p.error ? `⚠ ${p.error}` : "OK"}
                </td>
                <td style={{ padding: "4px" }}>
                  <Show when={p.id}>
                    <button onClick={() => onDelete(p.id)} disabled={busy()} title="削除">🗑</button>
                  </Show>
                </td>
              </tr>
            )}
          </For>
          <Show when={items().length === 0}>
            <tr><td colSpan={6} class="muted" style={{ padding: "16px", "text-align": "center" }}>プラグインがありません</td></tr>
          </Show>
        </tbody>
      </table>
    </div>
  );
}
