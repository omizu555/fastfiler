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
  setWorkspaceLayout,
  toggleWorkspaceTree,
  setWorkspaceTabsWidth,
  setWorkspaceTreeWidth,
} from "../store";
import { everythingPing, pluginsDirPath, revealInExplorer } from "../fs";
import { defaultHotkeys, eventToCombo, hotkeyLabels } from "../hotkeys";
import type { HotkeyAction, LinkChannel } from "../types";

interface Props {
  open: boolean;
  onClose: () => void;
}

type TabId = "general" | "search" | "links" | "hotkeys";

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
          </nav>

          <section class="modal-body">
            <Show when={tab() === "general"}>
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
                <label for="cfg-layout">タブサイドバー位置</label>
                <select
                  id="cfg-layout"
                  value={state.workspace.layout}
                  onChange={(e) => setWorkspaceLayout(e.currentTarget.value as never)}
                >
                  <option value="tabsLeft">左 (既定)</option>
                  <option value="tabsRight">右</option>
                  <option value="tabsHidden">非表示</option>
                </select>
                <small class="muted">Ctrl+B で循環切替</small>
              </div>
              <div class="setting-row">
                <label>ツリーパネル</label>
                <label class="inline">
                  <input
                    type="checkbox"
                    checked={state.workspace.showTree}
                    onChange={() => toggleWorkspaceTree()}
                  />
                  表示する (Ctrl+Shift+E)
                </label>
              </div>
              <div class="setting-row">
                <label for="cfg-tabsw">タブサイドバー幅</label>
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
                <label for="cfg-treew">ツリーパネル幅</label>
                <input
                  id="cfg-treew"
                  type="number"
                  min={140}
                  max={600}
                  value={state.workspace.treeWidth}
                  onChange={(e) => setWorkspaceTreeWidth(parseInt(e.currentTarget.value || "240", 10))}
                /> px
                <small class="muted">パネル右端をドラッグでも変更可</small>
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
