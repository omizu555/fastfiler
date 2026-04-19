// FastFiler Plugin SDK (browser ES module / classic <script>)
// 利用例:
//   <script src="../sdk.js"></script>
//   <script>
//     ff.notify("hello");
//     ff.fs.readDir("C:\\").then(entries => ...);
//     ff.on("pane.changed", e => console.log(e));
//     ff.registerContextMenuItem({ id: "copy-path", label: "パスをコピー", when: "any" });
//   </script>
//
// アプリとの通信は window.parent への postMessage を使用。
(function () {
  const handlers = new Map(); // id -> {resolve, reject}
  const eventHandlers = new Map(); // topic -> Set<fn>
  let seq = 0;

  function send(capability, args) {
    const id = ++seq;
    return new Promise((resolve, reject) => {
      handlers.set(id, { resolve, reject });
      window.parent.postMessage(
        { __ff: "invoke", id, capability, args: args || {} },
        "*",
      );
    });
  }

  window.addEventListener("message", (ev) => {
    const data = ev.data;
    if (!data || typeof data !== "object") return;
    if (data.__ff === "result" && handlers.has(data.id)) {
      const h = handlers.get(data.id);
      handlers.delete(data.id);
      if (data.ok) h.resolve(data.result);
      else h.reject(new Error(data.error || "plugin invoke failed"));
    } else if (data.__ff === "event" && data.topic) {
      const set = eventHandlers.get(data.topic);
      if (set) for (const fn of set) {
        try { fn(data.payload); } catch (e) { console.error(e); }
      }
    }
  });

  const ff = {
    invoke: send,
    on(topic, fn) {
      if (!eventHandlers.has(topic)) eventHandlers.set(topic, new Set());
      eventHandlers.get(topic).add(fn);
    },
    off(topic, fn) {
      const s = eventHandlers.get(topic);
      if (s) s.delete(fn);
    },
    notify(message, level = "info") {
      return send("ui.notify", { message, level });
    },
    fs: {
      readDir(path)               { return send("fs.read.dir",   { path }); },
      readText(path)              { return send("fs.read.text",  { path }); },
      writeText(path, content)    { return send("fs.write.text", { path, content }); },
      mkdir(path, recursive=true) { return send("fs.mkdir",      { path, recursive }); },
      rename(from, to)            { return send("fs.rename",     { from, to }); },
      copy(from, to)              { return send("fs.copy",       { from, to }); },
      move(from, to)              { return send("fs.move",       { from, to }); },
      delete(paths, permanent=false) { return send("fs.delete",  { paths, permanent }); },
      stat(path)                  { return send("fs.stat",       { path }); },
    },
    pane: {
      getActive()              { return send("pane.getActive", {}); },
      setPath(path, paneId)    { return send("pane.setPath", { path, paneId }); },
    },
    shell: {
      open(path)               { return send("shell.open", { path }); },
    },
    storage: {
      get(key)                 { return send("storage.get", { key }); },
      set(key, value)          { return send("storage.set", { key, value }); },
    },
    registerContextMenuItem(item) {
      return send("ui.contextMenu.register", item);
    },
  };

  window.ff = ff;
})();
