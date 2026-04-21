// 設定: 連動タブ (LinkGroup チャネル編集)
import { For } from "solid-js";
import { state, setLinkGroupChannel } from "../../store";
import type { LinkChannel } from "../../types";

const channelLabels: Record<LinkChannel, string> = {
  path: "パス移動",
  selection: "選択",
  scroll: "スクロール",
  sort: "ソート",
};

export default function LinksTab() {
  return (
    <>
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
    </>
  );
}
