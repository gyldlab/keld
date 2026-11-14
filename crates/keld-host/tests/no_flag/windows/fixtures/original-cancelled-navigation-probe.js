"use strict";
// First author script; existing native transport is observation and scheduling only.
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
    issuedCount: 0,
    expectedCall: 1,
    rawChallenge: null,
    rawRecords: [],
    markers: [],
    terminal: null,
    terminals: [],
    cancelRequested: false,
    cancelTarget: null,
    resumed: false,
    hrefAfterCancel: null,
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
  if (
    !native ||
    typeof native.addEventListener !== "function" ||
    typeof native.postMessage !== "function"
  ) {
    fail("actual Windows native port is missing");
    return;
  }
  // Exact endpoint-value bytes of three fixed ASCII arguments. These are
  // observations of public args-obj sends, not a generic codec or credential.
  const markerPayloads = {
    ready: [
      6, 1, 0, 5, 28, 0, 107, 101, 108, 100, 45, 111, 114, 105, 103, 105, 110,
      97, 108, 45, 114, 111, 117, 116, 101, 45, 114, 101, 97, 100, 121, 45, 118,
      49,
    ],
    cancel: [
      6, 1, 0, 5, 17, 0, 107, 101, 108, 100, 45, 120, 57, 45, 99, 97, 110, 99,
      101, 108, 45, 118, 49,
    ],
    resume: [
      6, 1, 0, 5, 17, 0, 107, 101, 108, 100, 45, 120, 57, 45, 114, 101, 115,
      117, 109, 101, 45, 118, 49,
    ],
  };
  const issue = (phase) => {
    const expected = phase === "before" ? 0 : 1;
    if (
      !state.registered ||
      state.routeMarker === null ||
      state.issuedCount !== expected ||
      state.terminals.length !== expected
    )
      throw new Error("one original call after each observed native barrier");
    state.issued = true;
    state.expectedCall = expected + 1;
    state.issuedCount += 1;
    window.electron.sendMessage("app-load-finished", {
      probe: "keld-original-x9",
      phase,
    });
    publish();
  };
  let started = false;
  const begin = () => {
    if (started || !state.registered || state.routeMarker === null) return;
    started = true;
    state.ready = true;
    try {
      issue("before");
    } catch (error) {
      fail(error);
    }
  };
  const act = (phase) => {
    try {
      if (phase === "ready") {
        begin();
      } else if (phase === "cancel") {
        if (!started || state.cancelRequested || state.terminals.length !== 1)
          throw new Error("cancel requires completed first A call");
        state.cancelRequested = true;
        const target = new URL("./x9-cancelled-target.html", first.href);
        if (
          target.href === location.href ||
          target.pathname === new URL(first.href).pathname
        )
          throw new Error("cancellation target must be a distinct path");
        state.cancelTarget = target.href;
        publish();
        // Ordinary page navigation, with no Host-created request/header.
        // This target is not a replacement fixture or a same-URI schedule.
        location.assign(target.href);
      } else if (phase === "resume") {
        if (
          !state.cancelRequested ||
          state.resumed ||
          state.terminals.length !== 1
        )
          throw new Error("resume requires the single cancelled navigation");
        state.resumed = true;
        state.hrefAfterCancel = location.href;
        if (location.href !== first.href)
          throw new Error("document A changed URL before the second effect");
        issue("after");
      }
    } catch (error) {
      fail(error);
    }
  };
  native.addEventListener("message", (event) => {
    try {
      if (typeof event.data !== "string" || event.data.length > 2048)
        throw new Error("unexpected native observation shape/bound");
      if (state.rawRecords.length === 12)
        throw new Error("native observation capacity exhausted");
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
        const phase = Object.keys(markerPayloads).find(
          (name) =>
            Array.isArray(value.payload) &&
            value.payload.length === markerPayloads[name].length &&
            value.payload.every(
              (byte, index) => byte === markerPayloads[name][index],
            ),
        );
        const expected = ["ready", "cancel", "resume"][state.markers.length];
        if (
          !phase ||
          phase !== expected ||
          event.source !== native ||
          typeof value.document !== "string" ||
          !/^[0-9a-f]{32}$/.test(value.document) ||
          !Number.isInteger(value.delivery) ||
          value.delivery < 1 ||
          value.delivery > 0xffff_ffff ||
          (state.routeMarker !== null &&
            value.document !== state.routeMarker) ||
          (state.markers.length > 0 &&
            value.delivery <= state.markers[state.markers.length - 1].delivery)
        )
          throw new Error(
            "actual native marker order, document or payload changed",
          );
        if (phase === "ready") state.routeMarker = value.document;
        state.markers.push({
          phase,
          document: value.document,
          delivery: value.delivery,
        });
        // The installed handler posts its normal consumed/no-listener receipt
        // before this microtask. A marker is only the fixture scheduling fact.
        queueMicrotask(() => act(phase));
      }
      if (
        value.v === 2 &&
        value.kind === "app-refused" &&
        (value.call === 1 || value.call === 2)
      )
        throw new Error("actual original call was refused: " + event.data);
      if (value.v === 2 && value.kind === "send-result") {
        if (
          value.document !== state.routeMarker ||
          value.message !== state.terminals.length + 1 ||
          value.message > state.issuedCount ||
          value.disposition?.status !== "consumed"
        )
          throw new Error(
            "actual send terminal is duplicate, refused or not A",
          );
        state.terminal = value;
        state.terminals.push(value);
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
