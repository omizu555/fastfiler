// 設定: 基本タブ (テーマ/アクセント/アイコン/タブ列数/隠しファイル/サムネ/ターミナル/ワークスペース配置)
import { For, Show, createResource, createSignal, onMount } from "solid-js";
import {
  state,
  setTheme,
  setAccentColor,
  setIconSet,
  setThemePreset,
  setIconPack,
  setTerminalShell,
  setTerminalFont,
  setTerminalFontSize,
  setUiFont,
  setUiFontSize,
  setPanelSlot,
  setWorkspaceTabsWidth,
  setWorkspaceTreeWidth,
  setSamePanelStack,
  setHidePaneToolbar,
} from "../../store";
import { loadSystemFonts, fallbackFonts } from "../../font-list";
import { openWithShell } from "../../fs";
import { templatesDirPath, refreshUserTemplates } from "../../templates";
import { userCommandsDir, refreshUserCommands, userCommands, userCommandsError } from "../../user-commands";
import { listLoadedPacks, loadCustomPacks, clearCustomIconCache } from "../../icon-custom";
import { clearSystemIconCache } from "../../icon-system";
import { THEME_CHOICES, ICON_CHOICES, findThemeChoice, findIconChoice, groupBy } from "../../theme-options";
import { invoke } from "@tauri-apps/api/core";

interface Props {
  columns: number;
  hidden: boolean;
  thumbs: boolean;
  onColumnsChange: (n: number) => void;
  onHiddenChange: (v: boolean) => void;
  onThumbsChange: (v: boolean) => void;
}

export default function GeneralTab(props: Props) {
  const [fonts] = createResource(() => loadSystemFonts());
  const fontList = () => fonts() ?? fallbackFonts();

  // v1.12: シェル統合 (フォルダ既定ハンドラ)
  const [shellAssoc, setShellAssoc] = createSignal(false);
  const [shellAssocBusy, setShellAssocBusy] = createSignal(false);
  const refreshShellAssoc = async () => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const v = await invoke<boolean>("shell_assoc_status");
      setShellAssoc(!!v);
    } catch {/* ignore */}
  };
  onMount(() => { void refreshShellAssoc(); });
  const toggleShellAssoc = async (enable: boolean) => {
    setShellAssocBusy(true);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      if (enable) await invoke("shell_assoc_enable");
      else await invoke("shell_assoc_disable");
      await refreshShellAssoc();
    } catch (e) {
      alert(`シェル統合の切替に失敗しました: ${e}`);
      await refreshShellAssoc();
    } finally {
      setShellAssocBusy(false);
    }
  };

  return (
    <>
      <div class="setting-row">
        <label for="cfg-theme">テーマ</label>
        <select
          id="cfg-theme"
          value={`${state.theme}|${state.themePreset}`}
          onChange={(e) => {
            const c = findThemeChoice(e.currentTarget.value);
            if (!c) return;
            setTheme(c.theme);
            setThemePreset(c.preset);
          }}
        >
          <For each={groupBy(THEME_CHOICES)}>
            {([group, items]) => (
              <optgroup label={group}>
                <For each={items}>
                  {(c) => <option value={c.value}>{c.label}</option>}
                </For>
              </optgroup>
            )}
          </For>
        </select>
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
        <label for="cfg-icon">アイコン</label>
        <select
          id="cfg-icon"
          value={`${state.iconSet}|${state.iconPack}`}
          onChange={(e) => {
            const v = e.currentTarget.value;
            const c = findIconChoice(v);
            if (c) {
              setIconSet(c.iconSet);
              setIconPack(c.iconPack);
              return;
            }
            // カスタムパック (`colored|custom:xxx`)
            const [s, p] = v.split("|");
            if (s && p) {
              setIconSet(s as never);
              setIconPack(p as never);
            }
          }}
        >
          <For each={groupBy(ICON_CHOICES)}>
            {([group, items]) => (
              <optgroup label={group}>
                <For each={items}>
                  {(c) => <option value={c.value}>{c.label}</option>}
                </For>
              </optgroup>
            )}
          </For>
          <Show when={listLoadedPacks().length > 0}>
            <optgroup label="カスタム">
              <For each={listLoadedPacks()}>
                {(p) => <option value={`colored|custom:${p.id}`}>Custom: {p.manifest.name ?? p.id}</option>}
              </For>
            </optgroup>
          </Show>
        </select>
      </div>
      <div class="setting-row">
        <label>アイコン パック フォルダ</label>
        <button
          class="ghost"
          onClick={async () => {
            try {
              const dir = await invoke<string>("icons_dir");
              await openWithShell(dir);
            } catch (e) {
              alert(`アイコンパック フォルダを開けませんでした: ${e}`);
            }
          }}
        >📂 アイコンパック フォルダを開く</button>
        <button
          class="ghost"
          style={{ "margin-left": "6px" }}
          onClick={() => { clearCustomIconCache(); clearSystemIconCache(); void loadCustomPacks(); }}
        >🔄 再読み込み</button>
        <small class="muted" style={{ "margin-left": "8px" }}>%APPDATA%\fastfiler\icons\</small>
      </div>

      <div class="setting-row">
        <label>新規ファイル テンプレート</label>
        <button
          class="ghost"
          onClick={async () => {
            try {
              const p = await templatesDirPath();
              await openWithShell(p);
            } catch (e) {
              alert(`テンプレフォルダを開けませんでした: ${e}`);
            }
          }}
        >📂 テンプレ フォルダを開く</button>
        <button
          class="ghost"
          style={{ "margin-left": "6px" }}
          onClick={() => { void refreshUserTemplates(); }}
        >🔄 再読込</button>
        <small class="muted" style={{ "margin-left": "8px" }}>
          %APPDATA%\fastfiler\templates にファイルを置くと「新規ファイル」サブメニューに表示
        </small>
      </div>

      <div class="setting-row">
        <label>ユーザー コマンド (v1.13)</label>
        <button
          class="ghost"
          onClick={async () => {
            try {
              const p = await userCommandsDir();
              await openWithShell(p);
            } catch (e) {
              alert(`コマンド フォルダを開けませんでした: ${e}`);
            }
          }}
        >📁 コマンド フォルダを開く</button>
        <button
          class="ghost"
          style={{ "margin-left": "6px" }}
          onClick={() => { void refreshUserCommands(); }}
        >🔄 再読込</button>
        <small class="muted" style={{ "margin-left": "8px" }}>
          %APPDATA%\fastfiler\commands\commands.json で右クリックメニューに項目追加
        </small>
        <div style={{ "margin-top": "6px", "padding-left": "0" }}>
          {userCommandsError() ? (
            <small style={{ color: "var(--danger, #e55)" }}>
              ⚠ パースエラー: {userCommandsError()}
            </small>
          ) : userCommands().length === 0 ? (
            <small class="muted">未登録 (commands.json.sample をコピー → commands.json にリネーム)</small>
          ) : (
            <small class="muted">登録中: {userCommands().length} 件 — {userCommands().map((c) => c.label).join(" / ")}</small>
          )}
        </div>
      </div>

      <div class="setting-row">
        <label for="cfg-cols">タブ列数</label>
        <input
          id="cfg-cols"
          type="number"
          min={1}
          max={8}
          value={props.columns}
          onInput={(e) => props.onColumnsChange(parseInt(e.currentTarget.value || "1", 10))}
        />
        <small class="muted">1〜8 列。即時反映（必要なら下のボタンで再読み込み）</small>
      </div>

      <div class="setting-row">
        <label for="cfg-hidden">隠しファイル</label>
        <label class="inline">
          <input
            id="cfg-hidden"
            type="checkbox"
            checked={props.hidden}
            onChange={(e) => props.onHiddenChange(e.currentTarget.checked)}
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
            checked={props.thumbs}
            onChange={(e) => props.onThumbsChange(e.currentTarget.checked)}
          />
          画像/動画/PDF などをサムネイル表示する
        </label>
      </div>

      <div class="setting-row">
        <label for="cfg-hide-toolbar">タブ/ツリー上部</label>
        <label class="inline">
          <input
            id="cfg-hide-toolbar"
            type="checkbox"
            checked={state.hidePaneToolbar}
            onChange={(e) => setHidePaneToolbar(e.currentTarget.checked)}
          />
          タブバー / ツリーのヘッダ部分を非表示にする
        </label>
      </div>

      <div class="setting-row">
        <label>プラグイン</label>
        <small class="muted">「プラグイン」タブで一覧 / 有効化 / インポート / 削除ができます</small>
      </div>

      <hr />
      <h3 class="settings-subhead">UI フォント</h3>
      <datalist id="font-options">
        <For each={fontList()}>{(f) => <option value={f} />}</For>
      </datalist>
      <div class="setting-row">
        <label for="cfg-ui-font">UI フォント</label>
        <input
          id="cfg-ui-font"
          type="text"
          list="font-options"
          placeholder="Yu Gothic UI"
          value={state.uiFont ?? ""}
          onChange={(e) => setUiFont(e.currentTarget.value.trim() || null)}
          style={{ "min-width": "220px" }}
        />
        <label for="cfg-ui-fs" style={{ "margin-left": "12px" }}>サイズ</label>
        <input
          id="cfg-ui-fs"
          type="number" min={9} max={24}
          value={state.uiFontSize}
          onInput={(e) => setUiFontSize(parseInt(e.currentTarget.value || "13", 10))}
          style={{ "width": "60px" }}
        />
        <small class="muted">
          {fonts.loading ? "システムフォント取得中…" : `候補 ${fontList().length} 件`}
        </small>
      </div>

      <hr />
      <h3 class="settings-subhead">ターミナル</h3>
      <div class="setting-row">
        <label for="cfg-term-shell">既定シェル</label>
        <input
          id="cfg-term-shell"
          type="text"
          placeholder="(空欄で OS 既定: cmd.exe / $SHELL)"
          value={state.terminalShell ?? ""}
          onChange={(e) => setTerminalShell(e.currentTarget.value.trim() || null)}
          style={{ "min-width": "220px" }}
        />
        <small class="muted">例: powershell.exe / pwsh / wt.exe</small>
      </div>
      <div class="setting-row">
        <label for="cfg-term-font">フォント</label>
        <input
          id="cfg-term-font"
          type="text"
          list="font-options"
          placeholder="Cascadia Mono, Consolas, monospace"
          value={state.terminalFont ?? ""}
          onChange={(e) => setTerminalFont(e.currentTarget.value.trim() || null)}
          style={{ "min-width": "220px" }}
        />
        <label for="cfg-term-fs" style={{ "margin-left": "12px" }}>サイズ</label>
        <input
          id="cfg-term-fs"
          type="number" min={8} max={36}
          value={state.terminalFontSize}
          onInput={(e) => setTerminalFontSize(parseInt(e.currentTarget.value || "13", 10))}
          style={{ "width": "60px" }}
        />
        <small class="muted">変更は次のセッションから適用</small>
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
      <div class="setting-row">
        <label>シェル統合 (実験的)</label>
        <label class="inline">
          <input
            type="checkbox"
            checked={shellAssoc()}
            disabled={shellAssocBusy()}
            onChange={(e) => { void toggleShellAssoc(e.currentTarget.checked); }}
          />
          フォルダ既定ハンドラとして登録 (Excel リンク等を新規タブで開く)
        </label>
        <button
          class="ghost"
          style={{ "margin-left": "8px" }}
          onClick={async () => {
            try {
              const { invoke } = await import("@tauri-apps/api/core");
              const out = await invoke<string>("shell_assoc_diagnose");
              alert(out);
            } catch (e) { alert(`診断失敗: ${e}`); }
          }}
        >🔍 診断</button>
      </div>
      <div class="setting-row" style={{ "margin-top": "-8px" }}>
        <span></span>
        <small class="muted">
          ⚠ ON にすると、デスクトップやエクスプローラのフォルダ ダブルクリックも FastFiler が開きます。
          OFF で Windows 標準の動作に戻ります (HKCU スコープ・管理者権限不要)。
        </small>
      </div>
      <Show when={false}><></></Show>
    </>
  );
}
