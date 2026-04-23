// 設定: 基本タブ (テーマ/アクセント/アイコン/タブ列数/隠しファイル/サムネ/ターミナル/ワークスペース配置)
import { For, Show, createResource } from "solid-js";
import {
  state,
  setTheme,
  setAccentColor,
  setIconSet,
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
  setToastPosition,
} from "../../store";
import { loadSystemFonts, fallbackFonts } from "../../font-list";

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
  return (
    <>
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
        <label for="cfg-toast-pos">通知の表示位置</label>
        <select
          id="cfg-toast-pos"
          value={state.toastPosition}
          onChange={(e) => setToastPosition(e.currentTarget.value as "popup" | "statusbar")}
        >
          <option value="popup">右下にポップアップ (既定)</option>
          <option value="statusbar">ステータスバー内</option>
        </select>
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
      <Show when={false}><></></Show>
    </>
  );
}
