// 設定: ホットキータブ
// capturing 状態は親 (SettingsDialog) で onkeydown と一緒に管理しているので props で受ける
import { For } from "solid-js";
import { state, resetHotkeys } from "../../store";
import { defaultHotkeys, hotkeyLabels } from "../../hotkeys";
import type { HotkeyAction } from "../../types";

interface Props {
  capturing: HotkeyAction | null;
  onCapture: (a: HotkeyAction) => void;
}

export default function HotkeysTab(props: Props) {
  return (
    <>
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
                    classList={{ capturing: props.capturing === act }}
                    onClick={() => props.onCapture(act)}
                  >
                    {props.capturing === act ? "（キーを押してください）" : (state.hotkeys[act] || "(未設定)")}
                  </button>
                </td>
                <td class="muted small">{defaultHotkeys[act]}</td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
    </>
  );
}
