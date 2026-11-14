"use strict";
// This is the first author script. The unchanged selected original preload must
// already have run at document start. Later ready events cannot repair first.
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
    mode: "facade",
    registered: false,
    routeMarker: null,
    ready: false,
    issued: false,
    expectedCall: 1,
    rawRecords: [],
    terminal: null,
    resource: "not-requested",
    resourceWidth: null,
    error: null,
  };
  const publish = () => {
    const text = JSON.stringify(state);
    if (text.length > 32768)
      throw new Error("bounded resource snapshot exceeded");
    document.documentElement.setAttribute("data-keld-original-probe", text);
  };
  const fail = (error) => {
    state.error = String(error).slice(0, 1024);
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
  let resource;
  const requestResource = () => {
    if (resource) throw new Error("duplicate fixture resource request");
    resource = new Image();
    resource.onload = () => {
      state.resource = "loaded";
      state.resourceWidth = resource.naturalWidth;
      publish();
    };
    resource.onerror = () => {
      state.resource = "failed";
      publish();
    };
    state.resource = "requested";
    publish();
    resource.src = "./x14-held-resource.svg";
  };
  // Exact endpoint-value bytes for the one fixed ASCII argument in the
  // public args-obj marker. This is an observation, never a caller credential.
  const routeMarkerPayload = [
    6, 1, 0, 5, 28, 0, 107, 101, 108, 100, 45, 111, 114, 105, 103, 105, 110, 97,
    108, 45, 114, 111, 117, 116, 101, 45, 114, 101, 97, 100, 121, 45, 118, 49,
  ];
  native.addEventListener("message", (event) => {
    try {
      if (typeof event.data !== "string" || event.data.length > 2048)
        throw new Error("unexpected actual native message bound");
      if (state.rawRecords.length === 12)
        throw new Error("resource observation capacity exhausted");
      state.rawRecords.push({
        sourceIsNativePort: event.source === native,
        text: event.data,
      });
      const value = JSON.parse(event.data);
      if (
        value.v === 2 &&
        value.kind === "event" &&
        value.channel === "args-obj"
      ) {
        if (
          state.routeMarker !== null ||
          event.source !== native ||
          typeof value.document !== "string" ||
          !/^[0-9a-f]{32}$/.test(value.document) ||
          !Number.isInteger(value.delivery) ||
          value.delivery < 1 ||
          value.delivery > 0xffff_ffff ||
          !Array.isArray(value.payload) ||
          value.payload.length !== routeMarkerPayload.length ||
          !value.payload.every(
            (byte, index) => byte === routeMarkerPayload[index],
          )
        )
          throw new Error(
            "actual native route marker is duplicate or malformed",
          );
        state.routeMarker = value.document;
        // The installed runtime receives this event first and posts its normal
        // consumed/no-listener receipt before this handler's microtask runs.
        queueMicrotask(begin);
      }
      if (
        state.issued &&
        value.v === 2 &&
        value.kind === "send-result" &&
        value.message === 1
      ) {
        if (state.terminal !== null)
          throw new Error("duplicate healthy terminal");
        if (value.document !== state.routeMarker)
          throw new Error("terminal changed the native marker document");
        if (value.disposition?.status !== "consumed")
          throw new Error("real initial Original send was not consumed");
        state.terminal = value;
        queueMicrotask(requestResource);
      }
      publish();
    } catch (error) {
      fail(error);
    }
  });
  let started = false;
  const begin = () => {
    if (started || !state.registered || state.routeMarker === null) return;
    started = true;
    state.ready = true;
    try {
      state.issued = true;
      window.electron.sendMessage("app-load-finished", {
        probe: "keld-original-x14",
        path: "facade",
      });
      publish();
    } catch (error) {
      fail(error);
    }
  };
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
