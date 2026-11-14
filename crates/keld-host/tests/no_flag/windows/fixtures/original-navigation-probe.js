"use strict";
// Fixed first-author page; ordinary endpoint values supply no native authority.
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
  const documentKind = new URL(location.href).searchParams.get("document");
  if (documentKind !== "A" && documentKind !== "B")
    throw new Error("fixed A or B document required");
  const state = {
    documentKind,
    rawACanaries: 0,
    first,
    mode: "facade",
    registered: false,
    routeMarker: null,
    routeMarkerKind: null,
    routeDelivery: null,
    rawChallenge: null,
    ready: false,
    issued: false,
    expectedCall: 2,
    rawRecords: [],
    terminal: null,
    targetDelivery: null,
    entered: 0,
    error: null,
  };
  const publish = () => {
    const text = JSON.stringify(state);
    if (text.length > 32768) throw new Error("bounded probe snapshot exceeded");
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
  // Fixed canonical endpoint-value byte observations, never a new codec/grammar.
  const readyPayload = [
    6, 1, 0, 5, 28, 0, 107, 101, 108, 100, 45, 111, 114, 105, 103, 105, 110, 97,
    108, 45, 114, 111, 117, 116, 101, 45, 114, 101, 97, 100, 121, 45, 118, 49,
  ];
  const targetPayload = [
    6, 1, 0, 5, 18, 0, 107, 101, 108, 100, 45, 120, 49, 48, 45, 116, 97, 114,
    103, 101, 116, 45, 118, 49,
  ];
  const matches = (actual, expected) =>
    Array.isArray(actual) &&
    actual.length === expected.length &&
    actual.every((byte, index) => byte === expected[index]);
  let started = false;
  const begin = () => {
    if (started || !state.registered || state.routeMarker === null) return;
    started = true;
    state.ready = true;
    try {
      // The real original Function registers the listener before the next
      // original send call; its actual send-result is the parent's ready barrier.
      window.electron.registerMsgListener("args-obj", (value) => {
        if (
          documentKind !== "A" ||
          value !== "keld-x10-target-v1" ||
          state.entered !== 0
        )
          throw new Error("duplicate or foreign target Event");
        state.entered += 1;
        publish();
      });
      state.issued = true;
      window.electron.sendMessage("app-load-finished", {
        probe: "keld-original-x10-nav",
        path: "facade",
        document: documentKind,
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
      // The complete actual message was captured above before any JSON,
      // kind, document, epoch or channel check. The parent rechecks every row.
      const value = JSON.parse(event.data);
      if (matches(value.payload, targetPayload)) {
        state.rawACanaries += 1;
        if (documentKind === "B")
          throw new Error("B raw native port received the A canary");
      }
      if (
        documentKind === "B" &&
        value.v === 2 &&
        value.kind === "route-state" &&
        value.ready === true
      ) {
        if (
          typeof value.document !== "string" ||
          !/^[0-9a-f]{32}$/.test(value.document) ||
          state.routeMarker !== null
        )
          throw new Error("invalid or duplicate B route-ready CONTROL");
        state.routeMarker = value.document;
        state.routeMarkerKind = "route-state";
        state.rawChallenge = value.document;
        queueMicrotask(begin);
      }
      if (
        value.v === 2 &&
        value.kind === "event" &&
        value.channel === "args-obj"
      ) {
        if (
          typeof value.document !== "string" ||
          !/^[0-9a-f]{32}$/.test(value.document) ||
          !Number.isInteger(value.delivery) ||
          value.delivery < 1 ||
          value.delivery > 0xffff_ffff
        )
          throw new Error("invalid exact native Event key");
        if (matches(value.payload, readyPayload)) {
          if (documentKind !== "A" || state.routeMarker !== null)
            throw new Error("duplicate or non-A ready Event");
          state.routeMarker = value.document;
          state.routeMarkerKind = "event";
          state.rawChallenge = value.document;
          state.routeDelivery = value.delivery;
          queueMicrotask(begin);
        } else if (matches(value.payload, targetPayload)) {
          if (
            value.document !== state.routeMarker ||
            state.targetDelivery !== null
          )
            throw new Error("target Event changed original document");
          state.targetDelivery = value.delivery;
        } else {
          throw new Error("foreign args-obj Event");
        }
      }
      if (
        state.issued &&
        value.v === 2 &&
        ((value.kind === "app-refused" && value.call === state.expectedCall) ||
          (value.kind === "send-result" && value.message === 1))
      ) {
        if (state.terminal !== null || value.document !== state.routeMarker)
          throw new Error("duplicate or foreign initial native terminal");
        state.terminal = value;
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
