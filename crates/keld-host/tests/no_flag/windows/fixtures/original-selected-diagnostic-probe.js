"use strict";
// The existing original preload and existing endpoint channels carry every effect.
(() => {
  const names = ["request", "registerMsgListener", "sendMessage", "listenOnce"];
  let configuration = null,
    configurationError = null;
  try {
    const text = localStorage.getItem(".configuration");
    configuration = text === null ? null : JSON.parse(text);
  } catch (error) {
    configurationError = String(error);
  }
  const first = Object.freeze({
    readyState: document.readyState,
    firstAuthorScript: document.currentScript === document.scripts[0],
    authorScriptCount: document.scripts.length,
    top: window === window.top,
    href: location.href,
    processType:
      typeof window.process === "object" ? window.process.type : null,
    versions:
      typeof window.process === "object" && window.process.versions
        ? { ...window.process.versions }
        : null,
    methods: Object.fromEntries(
      names.map((name) => [name, typeof window.electron?.[name]]),
    ),
    adaptiveColors: configuration?.defaultAdaptiveColors ?? null,
    configurationError,
  });
  const state = {
    first,
    registered: false,
    routeReady: false,
    issued: false,
    rawChallenge: null,
    rawRecords: [],
    targetDelivery: null,
    entered: 0,
    error: null,
  };
  const publish = () => {
    const text = JSON.stringify(state);
    if (text.length > 32768)
      throw new Error("bounded original diagnostic probe");
    document.documentElement.setAttribute("data-keld-original-probe", text);
  };
  const fail = (error) => {
    if (state.error === null) state.error = String(error).slice(0, 1024);
    publish();
  };
  publish();
  window.addEventListener("error", (event) => fail(event.message));
  window.addEventListener("unhandledrejection", (event) => fail(event.reason));
  const native = window.chrome?.webview;
  if (!native || typeof native.addEventListener !== "function") {
    fail("actual Windows native port is missing");
    return;
  }
  const begin = () => {
    if (state.issued || !state.registered || !state.routeReady) return;
    state.issued = true;
    try {
      window.electron.registerMsgListener("args-obj", (value) => {
        if (value !== "keld-x10-selected-target-v1" || state.entered !== 0)
          throw new Error("duplicate or foreign target Event");
        state.entered += 1;
        publish();
        // Genuine page listener failure; generated nativeEvent selects Refused.
        throw new Error("x10-selected-diagnostic-v1");
      });
      window.electron.sendMessage("app-load-finished", {
        probe: "keld-original-x10-selected",
        path: "facade",
      });
      publish();
    } catch (error) {
      fail(error);
    }
  };
  native.addEventListener("message", (event) => {
    try {
      if (
        event.source !== native ||
        typeof event.data !== "string" ||
        event.data.length > 2048
      )
        throw new Error("unexpected actual native observation");
      if (state.rawRecords.length >= 12)
        throw new Error("native observation capacity exhausted");
      state.rawRecords.push({ sourceIsNativePort: true, text: event.data });
      const value = JSON.parse(event.data);
      if (value.v === 2 && typeof value.document === "string") {
        if (!/^[0-9a-f]{32}$/.test(value.document))
          throw new Error("invalid actual document marker");
        if (
          state.rawChallenge !== null &&
          state.rawChallenge !== value.document
        )
          throw new Error("original document changed");
        state.rawChallenge = value.document;
      }
      if (
        value.v === 2 &&
        (value.kind === "bound" || value.kind === "route-state") &&
        value.ready === true
      ) {
        state.routeReady = true;
        queueMicrotask(begin);
      }
      if (value.v === 2 && value.kind === "event") {
        if (
          value.channel !== "args-obj" ||
          state.targetDelivery !== null ||
          !Number.isInteger(value.delivery) ||
          value.delivery <= 0
        )
          throw new Error("unexpected actual target Event");
        state.targetDelivery = value.delivery;
      }
      publish();
    } catch (error) {
      fail(error);
    }
  });
  document.addEventListener(
    "__keld_app_ready_data_v1",
    () => {
      state.registered = true;
      publish();
      queueMicrotask(begin);
    },
    { once: true },
  );
  document.dispatchEvent(new CustomEvent("__keld_page_ready_query_v1"));
})();
