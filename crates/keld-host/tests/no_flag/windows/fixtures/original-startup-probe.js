"use strict";
// Secondary page observations only. Native callback/entry/clock traces are scored
// independently; this page cannot create a registration or a native receipt.
// The canonical page facade publishes process even when the hostile original
// preload withholds its internal export. No process-absence inference is made.
(() => {
  const names = ["request", "registerMsgListener", "sendMessage", "listenOnce"];
  const first = Object.freeze({
    top: window === window.top,
    readyState: document.readyState,
    authorScriptCount: document.scripts.length,
    firstAuthorScript: document.currentScript === document.scripts[0],
    processType:
      typeof window.process === "object" ? window.process.type : null,
    methods: Object.fromEntries(
      names.map((name) => [name, typeof window.electron?.[name]]),
    ),
  });
  const state = {
    first,
    bounds: 0,
    bindMessagesSent: 0,
    appReady: 0,
    domObserved: false,
    error: null,
  };
  const publish = () => {
    const text = JSON.stringify(state);
    if (text.length > 4096) throw new Error("startup probe bound exceeded");
    document.documentElement.setAttribute("data-keld-original-probe", text);
  };
  const fail = (error) => {
    state.error = String(error).slice(0, 512);
    publish();
  };
  window.addEventListener("error", (event) => fail(event.message));
  window.addEventListener("unhandledrejection", (event) => fail(event.reason));
  document.addEventListener(
    "DOMContentLoaded",
    () => {
      state.domObserved = true;
      publish();
    },
    { once: true },
  );
  document.addEventListener("__keld_app_ready_data_v1", () => {
    state.appReady += 1;
    publish();
  });
  publish();
  const native = window.chrome?.webview;
  if (
    !native ||
    typeof native.addEventListener !== "function" ||
    typeof native.postMessage !== "function"
  ) {
    fail("actual Windows native port is missing");
    return;
  }
  native.addEventListener("message", (event) => {
    try {
      if (event.source !== native) throw new Error("foreign native-port event");
      if (typeof event.data !== "string" || event.data.length > 4096)
        throw new Error("unexpected native observation shape/bound");
      const value = JSON.parse(event.data);
      if (value.kind !== "bound") return;
      state.bounds += 1;
      if (state.bounds > 2)
        throw new Error("more than the admitted initial and DOM retry");
      if (state.bounds === 1) {
        for (let i = 0; i < 2; i += 1) {
          native.postMessage(JSON.stringify({ v: 2, kind: "bind" }));
          state.bindMessagesSent += 1;
        }
      }
      publish();
    } catch (error) {
      fail(error);
    }
  });
})();
