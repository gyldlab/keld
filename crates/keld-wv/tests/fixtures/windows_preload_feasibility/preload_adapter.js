// Native timing fixture ONLY. This is not KELD's shipping Electron facade.
// No Host object, grant, KIPC session, role token or source authority is exposed.
(() => {
  "use strict";
  if (window !== window.top || location.protocol !== "file:") return;
  const options = "__W1_FIXTURE_OPTIONS__";
  const post = chrome.webview.postMessage.bind(chrome.webview);
  const listeners = new Map();
  const facade = Object.freeze({
    contextBridge: Object.freeze({
      exposeInMainWorld(key, value) {
        if (key !== "electron" && key !== "process")
          throw new Error("Unexpected fixture exposure");
        Object.defineProperty(window, key, {
          value: Object.freeze(value),
          enumerable: true,
          configurable: false,
          writable: false,
        });
      },
    }),
    ipcRenderer: Object.freeze({
      on(channel, listener) {
        if (!listeners.has(channel)) listeners.set(channel, []);
        listeners.get(channel).push({ listener, once: false });
      },
      once(channel, listener) {
        if (!listeners.has(channel)) listeners.set(channel, []);
        listeners.get(channel).push({ listener, once: true });
      },
      send(channel, ...args) {
        post(
          JSON.stringify({ kind: "preload-sink-observation", channel, args }),
        );
      },
    }),
  });
  const fixtureProcess = Object.freeze({
    type: "renderer",
    versions: Object.freeze({}),
    contextIsolated: false,
    argv: Object.freeze(["--initial-adaptive-colors=auto"]),
  });
  if (options.existingConfiguration !== null) {
    localStorage.setItem(
      ".configuration",
      JSON.stringify({ defaultAdaptiveColors: options.existingConfiguration }),
    );
  }
  const requireFixture = (name) => {
    if (name !== "electron")
      throw new Error("No other fixture module is available");
    return facade;
  };
  ((require, process) => {
    /* INSERT_AUTHENTIC_PRELOAD_BYTES */
  })(requireFixture, fixtureProcess);
  // Plain test observations; neither field is a capability or an authority token.
  Object.defineProperty(window, "__w1FixtureObservation", {
    value: Object.freeze({
      contextIsolated: fixtureProcess.contextIsolated,
      sourceDigest: options.sourceDigest,
    }),
  });
  post(
    JSON.stringify({
      kind: "preload-complete",
      sourceDigest: options.sourceDigest,
      registeredChannels: [...listeners.keys()],
      contextIsolated: fixtureProcess.contextIsolated,
    }),
  );
})();
