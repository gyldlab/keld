"use strict";
// Runs as an external author script IN the initial about:blank child.
// It never invokes or borrows the parent's native port.
(() => {
  const native = window.chrome?.webview;
  const documentEpoch = document.currentScript?.dataset.document;
  const state = {
    top: window === window.top,
    href: location.href,
    scriptUrl: document.currentScript?.src ?? null,
    ownPort: !!native && typeof native.postMessage === "function",
    frameworkMethods: Object.fromEntries(
      ["request", "registerMsgListener", "sendMessage", "listenOnce"].map(
        (name) => [name, typeof window.electron?.[name]],
      ),
    ),
    sent: false,
    error: null,
  };
  const publish = () => {
    const text = JSON.stringify(state);
    if (text.length > 4096)
      throw new Error("bounded child diagnostic exceeded");
    document.documentElement.setAttribute("data-keld-frame-stimulus", text);
  };
  try {
    if (state.top || state.href !== "about:blank")
      throw new Error("stimulus is not the initial child document");
    if (!state.ownPort) throw new Error("child has no actual native port");
    if (!/^[0-9a-f]{32}$/.test(documentEpoch ?? ""))
      throw new Error("actual top epoch stimulus is missing");
    const text = JSON.stringify({
      v: 1,
      kind: "call",
      document: documentEpoch,
      call: 2,
      method: "sendMessage",
      args: [
        "app-load-finished",
        {
          k: "object",
          entries: [
            ["probe", "keld-original-x3"],
            ["path", "frame"],
          ],
        },
      ],
    });
    // A same-format raw call using the already observed Legacy page epoch.
    // Calling return is only a page claim; actual Frame2 ingress is mandatory.
    native.postMessage(text);
    state.sent = true;
  } catch (error) {
    state.error = String(error).slice(0, 1024);
  }
  publish();
})();
