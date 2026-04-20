import { For, Show, createSignal, onCleanup, onMount } from "solid-js";
import {
  state,
  setTabColumns,
  setShowHidden,
  setShowThumbnails,
  setLinkGroupChannel,
  setHotkey,
  resetHotkeys,
  setSearchBackend,
  setEverythingPort,
  setEverythingScope,
  setWorkspaceTabsWidth,
  setWorkspaceTreeWidth,
  setPanelSlot,
  setSamePanelStack,
  setTheme,
  setAccentColor,
  setIconSet,
  savePreset,
  applyPreset,
  deletePreset,
  renamePreset,
  exportPresetsJson,
  importPresetsJson,
} from "../store";
import { everythingPing, pluginsDirPath, revealInExplorer } from "../fs";
import { defaultHotkeys, eventToCombo, hotkeyLabels } from "../hotkeys";
import type { HotkeyAction, LinkChannel } from "../types";

interface Props {
  open: boolean;
  onClose: () => void;
}

type TabId = "general" | "search" | "links" | "hotkeys" | "presets";

const channelLabels: Record<LinkChannel, string> = {
  path: "パス移動",
  selection: "選択",
  scroll: "スクロール",
  sort: "ソート",
};

export default function SettingsDialog(props: Props) {
  const [tab, setTab] = createSignal<TabId>("general");
  const [columns, setColumns] = createSignal(state.tabColumns);
  const [hidden, setHidden] = createSignal(state.showHidden);
  const [thumbs, setThumbs] = createSignal(state.showThumbnails);
  const [dirty, setDirty] = createSignal(false);
  const [capturing, setCapturing] = createSignal<HotkeyAction | null>(null);
  const [newPresetName, setNewPresetName] = createSignal("");
  const [importText, setImportText] = createSignal("");
  const [showImport, setShowImport] = createSignal(false);

  const formatTs = (ts: number) => {
    const d = new Date(ts);
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  };

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
      // フォールバック: prompt で表示
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

  const reset = () => {
    setColumns(state.tabColumns);
    setHidden(state.showHidden);
    setThumbs(state.showThumbnails);
    setDirty(false);
  };

  let prevOpen = false;
  const sync = () => {
    if (props.open && !prevOpen) reset();
    prevOpen = props.open;
  };

  onMount(() => {
    sync();
    const onKey = (e: KeyboardEvent) => {
      if (!props.open) return;
      if (capturing()) {
        // ホットキーキャプチャ中：このダイアログ内で吸収
        const combo = eventToCombo(e);
        if (combo) {
          e.preventDefault();
          e.stopPropagation();
          setHotkey(capturing()!, combo);
          setCapturing(null);
        } else if (e.key === "Escape") {
          setCapturing(null);
        }
        return;
      }
      if (e.key === "Escape") props.onClose();
    };
    window.addEventListener("keydown", onKey, true);
    onCleanup(() => window.removeEventListener("keydown", onKey, true));
  });

  const apply = () => {
    const n = Math.min(8, Math.max(1, Math.floor(columns())));
    setTabColumns(n);
    setShowHidden(hidden());
    setShowThumbnails(thumbs());
    setDirty(false);
    props.onClose();
  };

  const applyAndReload = () => {
    apply();
    setTimeout(() => location.reload(), 50);
  };

  return (
    <Show when={(sync(), props.open)}>
      <div class="modal-backdrop" onClick={props.onClose}>
        <div class="modal modal-lg" onClick={(e) => e.stopPropagation()}>
          <header class="modal-head">
            <strong>設定</strong>
            <span class="spacer" />
            <button class="modal-close" onClick={props.onClose}>×</button>
          </header>

          <nav class="settings-tabs">
            <button classList={{ active: tab() === "general" }} onClick={() => setTab("general")}>基本</button>
            <button classList={{ active: tab() === "search" }} onClick={() => setTab("search")}>検索</button>
            <button classList={{ active: tab() === "links" }} onClick={() => setTab("links")}>連動</button>
            <button classList={{ active: tab() === "hotkeys" }} onClick={() => setTab("hotkeys")}>ホットキー</button>
            <button classList={{ active: tab() === "presets" }} onClick={() => setTab("presets")}>プリセット</button>
          </nav>

          <section class="modal-body">
            <Show when={tab() === "general"}>
              <div class="setting-row">
                <label>テーマ</label>
                <label class="inline">
                  <input type="radio" name="theme" value="system"
                    checked={state.theme === "system"}
                    onChange={() => setTheme("system")} /> OS依存
                </label>
                <label class="inline" style={{ "margin-left": "10px" }}>
                  <input type="radio" name="theme" value="light"
                    checked={state.theme === "light"}
                    onChange={() => setTheme("light")} /> ☀ ライト
                </label>
                <label class="inline" style={{ "margin-left": "10px" }}>
                  <input type="radio" name="theme" value="dark"
                    checked={state.theme === "dark"}
                    onChange={() => setTheme("dark")} /> 🌙 ダーク
                </label>
              </div>

              <div class="setting-row">
                <label for="cfg-accent">アクセント色</label>
                <input
                  id="cfg-accent"
                  type="color"
                  value={state.accentColor ?? "#3b82f6"}
                  onInput={(e) => setAccentColor(e.currentTarget.value)}
                  style={{ "width": "48px", "height": "28px", "padding": "0", "border": "1px solid var(--border)" }}
                />
                <button class="ghost" style={{ "margin-left": "8px" }} onClick={() => setAccentColor(null)}>
                  既定に戻す
                </button>
                <small class="muted" style={{ "margin-left": "8px" }}>
                  ボタン/選択行などのハイライト色
                </small>
              </div>

              <div class="setting-row">
                <label>アイコンセット</label>
                <label class="inline">
                  <input type="radio" name="iconset" value="emoji"
                    checked={state.iconSet === "emoji"}
                    onChange={() => setIconSet("emoji")} /> 📁 既定
                </label>
                <label class="inline" style={{ "margin-left": "10px" }}>
                  <input type="radio" name="iconset" value="colored"
                    checked={state.iconSet === "colored"}
                    onChange={() => setIconSet("colored")} /> 🎨 拡張子別
                </label>
                <label class="inline" style={{ "margin-left": "10px" }}>
                  <input type="radio" name="iconset" value="minimal"
                    checked={state.iconSet === "minimal"}
                    onChange={() => setIconSet("minimal")} /> ▸ ミニマル
                </label>
              </div>

              <div class="setting-row">
                <label for="cfg-cols">タブ列数</label>
                <input
                  id="cfg-cols"
                  type="number"
                  min={1}
                  max={8}
                  value={columns()}
                  onInput={(e) => {
                    setColumns(parseInt(e.currentTarget.value || "1", 10));
                    setDirty(true);
                  }}
                />
                <small class="muted">1〜8 列。即時反映（必要なら下のボタンで再読み込み）</small>
              </div>

              <div class="setting-row">
                <label for="cfg-hidden">隠しファイル</label>
                <label class="inline">
                  <input
                    id="cfg-hidden"
                    type="checkbox"
                    checked={hidden()}
                    onChange={(e) => { setHidden(e.currentTarget.checked); setDirty(true); }}
                  />
                  表示する
                </label>
              </div>

              <div class="setting-row">
                <label for="cfg-thumbs">サムネイル</label>
                <label class="inline">
                  <input
                    id="cfg-thumbs"
                    type="checkbox"
                    checked={thumbs()}
                    onChange={(e) => { setThumbs(e.currentTarget.checked); setDirty(true); }}
                  />
                  画像/動画/PDF などをサムネイル表示する
                </label>
              </div>

              <div class="setting-row">
                <label>プラグイン</label>
                <button onClick={async () => {
                  try { await revealInExplorer(await pluginsDirPath()); } catch (e) { alert(`開けません: ${e}`); }
                }}>📂 プラグインフォルダを開く</button>
                <small class="muted">manifest.json を含むフォルダを置くと自動検出されます</small>
              </div>

              <hr />
              <h3 class="settings-subhead">ワークスペース配置</h3>
              <div class="setting-row">
                <label for="cfg-tabs-slot">タブパネル位置</label>
                <select
                  id="cfg-tabs-slot"
                  value={state.workspace.panelDock?.tabs.slot ?? "left"}
                  onChange={(e) => setPanelSlot("tabs", e.currentTarget.value as never)}
                >
                  <option value="left">左</option>
                  <option value="right">右</option>
                  <option value="top">上</option>
                  <option value="bottom">下</option>
                  <option value="hidden">非表示</option>
                </select>
                <small class="muted">Ctrl+B で循環切替</small>
              </div>
              <div class="setting-row">
                <label for="cfg-tree-slot">ツリーパネル位置</label>
                <select
                  id="cfg-tree-slot"
                  value={state.workspace.panelDock?.tree.slot ?? "hidden"}
                  onChange={(e) => setPanelSlot("tree", e.currentTarget.value as never)}
                >
                  <option value="left">左</option>
                  <option value="right">右</option>
                  <option value="top">上</option>
                  <option value="bottom">下</option>
                  <option value="hidden">非表示</option>
                </select>
                <small class="muted">Ctrl+Shift+E で表示/非表示</small>
              </div>
              <div class="setting-row">
                <label for="cfg-tabsw">タブパネルサイズ</label>
                <input
                  id="cfg-tabsw"
                  type="number"
                  min={140}
                  max={600}
                  value={state.workspace.tabsWidth}
                  onChange={(e) => setWorkspaceTabsWidth(parseInt(e.currentTarget.value || "240", 10))}
                /> px
              </div>
              <div class="setting-row">
                <label for="cfg-treew">ツリーパネルサイズ</label>
                <input
                  id="cfg-treew"
                  type="number"
                  min={140}
                  max={600}
                  value={state.workspace.treeWidth}
                  onChange={(e) => setWorkspaceTreeWidth(parseInt(e.currentTarget.value || "240", 10))}
                /> px
                <small class="muted">パネル端をドラッグでも変更可</small>
              </div>
              <div class="setting-row">
                <label for="cfg-stack">同じ位置に複数あるとき</label>
                <label class="checkline">
                  <input
                    id="cfg-stack"
                    type="checkbox"
                    checked={!!state.workspace.samePanelStack}
                    onChange={(e) => setSamePanelStack(e.currentTarget.checked)}
                  />
                  縦/横に積み重ねる (OFF: 並列で 3 列表示)
                </label>
              </div>
            </Show>

            <Show when={tab() === "search"}>
              <p class="muted small">
                Everything (voidtools) の HTTP Server を有効化すると、`Ctrl+F` 検索を瞬時に行えます。<br />
                Everything → Tools → Options → HTTP Server を <b>Enable</b> にしてポートを設定してください (既定 80)。
              </p>
              <div class="setting-row">
                <label>検索バックエンド</label>
                <label class="inline">
                  <input type="radio" name="bk" value="builtin"
                    checked={state.searchBackend === "builtin"}
                    onChange={() => setSearchBackend("builtin")} /> 内蔵 (再帰列挙)
                </label>
                <label class="inline" style={{ "margin-left": "10px" }}>
                  <input type="radio" name="bk" value="everything"
                    checked={state.searchBackend === "everything"}
                    onChange={() => setSearchBackend("everything")} /> Everything HTTP
                </label>
              </div>
              <div class="setting-row">
                <label for="cfg-ev-port">Everything ポート</label>
                <input id="cfg-ev-port" type="number" min={1} max={65535}
                  value={state.everythingPort}
                  onChange={(e) => setEverythingPort(parseInt(e.currentTarget.value || "80", 10))} />
                <button onClick={async () => {
                  const ok = await everythingPing(state.everythingPort);
                  alert(ok ? `✅ Everything 応答 OK (port ${state.everythingPort})` : `❌ 応答なし (port ${state.everythingPort})\nEverything が起動していて HTTP Server が有効か確認してください`);
                }}>接続テスト</button>
              </div>
              <div class="setting-row">
                <label>検索範囲</label>
                <label class="inline">
                  <input type="checkbox" checked={state.everythingScope}
                    onChange={(e) => setEverythingScope(e.currentTarget.checked)} />
                  現在のフォルダ以下に限定する (path:"…" で絞り込み)
                </label>
                <small class="muted">OFF にすると全ドライブから検索</small>
              </div>
            </Show>

            <Show when={tab() === "links"}>
              <p class="muted small">
                ペインに割り当てた連動グループの伝搬チャネルを編集できます。
                <br />path=パス移動 / selection=選択 / scroll=スクロール / sort=ソート（将来対応）
              </p>
              <table class="link-table">
                <thead>
                  <tr>
                    <th>グループ</th>
                    <For each={(["path", "selection", "scroll", "sort"] as LinkChannel[])}>
                      {(c) => <th>{channelLabels[c]}</th>}
                    </For>
                  </tr>
                </thead>
                <tbody>
                  <For each={state.linkGroups}>
                    {(g) => (
                      <tr>
                        <td>
                          <span class="lg-chip" style={{ background: g.color }}>{g.name}</span>
                        </td>
                        <For each={(["path", "selection", "scroll", "sort"] as LinkChannel[])}>
                          {(c) => (
                            <td>
                              <input
                                type="checkbox"
                                checked={g.channels[c]}
                                onChange={(e) => setLinkGroupChannel(g.id, c, e.currentTarget.checked)}
                              />
                            </td>
                          )}
                        </For>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </Show>

            <Show when={tab() === "hotkeys"}>
              <div class="hk-bar">
                <small class="muted">行右の入力欄をクリック → 押したいキーで上書き / Esc でキャンセル</small>
                <span class="spacer" />
                <button onClick={() => { if (confirm("既定のホットキーに戻しますか？")) resetHotkeys(); }}>初期値に戻す</button>
              </div>
              <table class="hk-table">
                <thead>
                  <tr><th>動作</th><th>キー</th><th>既定</th></tr>
                </thead>
                <tbody>
                  <For each={Object.keys(state.hotkeys) as HotkeyAction[]}>
                    {(act) => (
                      <tr>
                        <td>{hotkeyLabels[act]}</td>
                        <td>
                          <button
                            class="hk-cell"
                            classList={{ capturing: capturing() === act }}
                            onClick={() => setCapturing(act)}
                          >
                            {capturing() === act ? "（キーを押してください）" : (state.hotkeys[act] || "(未設定)")}
                          </button>
                        </td>
                        <td class="muted small">{defaultHotkeys[act]}</td>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </Show>

            <Show when={tab() === "presets"}>
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
            </Show>
          </section>

          <footer class="modal-foot">
            <small class="muted">{dirty() ? "未保存の変更あり" : ""}</small>
            <span class="spacer" />
            <button onClick={props.onClose}>キャンセル</button>
            <button onClick={applyAndReload}>適用して再読み込み</button>
            <button class="primary" onClick={apply}>OK</button>
          </footer>
        </div>
      </div>
    </Show>
  );
}
