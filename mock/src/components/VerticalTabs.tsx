import { For } from "solid-js";
import {
  state,
  setActiveTab,
  closeTab,
  addTab,
  setTabColumns,
} from "../store";

export default function VerticalTabs() {
  return (
    <aside class="vtabs">
      <div class="vtabs-head">
        <strong>タブ</strong>
        <div class="col-ctl">
          <span>列:</span>
          <For each={[1, 2, 3, 4]}>
            {(n) => (
              <button
                classList={{ active: state.tabColumns === n }}
                onClick={() => setTabColumns(n)}
              >
                {n}
              </button>
            )}
          </For>
        </div>
        <button class="add" onClick={addTab}>
          ＋ 新規
        </button>
      </div>
      <div
        class="vtabs-grid"
        style={{ "grid-template-columns": `repeat(${state.tabColumns}, 1fr)` }}
      >
        <For each={state.tabs}>
          {(t) => (
            <div
              classList={{ vtab: true, active: state.activeTabId === t.id }}
              onClick={() => setActiveTab(t.id)}
              title={t.title}
            >
              <span class="vtab-title">{t.title}</span>
              <button
                class="vtab-close"
                onClick={(e) => {
                  e.stopPropagation();
                  closeTab(t.id);
                }}
              >
                ×
              </button>
            </div>
          )}
        </For>
      </div>
      <div class="vtabs-foot">
        <small>
          連動グループ:{" "}
          <For each={state.linkGroups}>
            {(g) => (
              <span class="lg-chip" style={{ background: g.color }}>
                {g.name}
              </span>
            )}
          </For>
        </small>
      </div>
    </aside>
  );
}
