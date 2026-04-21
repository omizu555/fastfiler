// 設定: 検索タブ (Everything 連携)
import { state, setSearchBackend, setEverythingPort, setEverythingScope } from "../../store";
import { everythingPing } from "../../fs";

export default function SearchTab() {
  return (
    <>
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
    </>
  );
}
