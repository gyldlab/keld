"use strict";
// First external author script under the unchanged selected file-source CSP.
// Native Frame2 observation, not this DOM state, must prove child ingress.
(() => {
  const native = window.chrome?.webview;
  const state = {
    first: {
      top: window === window.top,
      href: location.href,
      firstAuthorScript: document.currentScript === document.scripts[0],
      authorScriptCount: document.scripts.length,
      readyState: document.readyState,
      methods: Object.fromEntries(
        ["request", "registerMsgListener", "sendMessage", "listenOnce"].map(
          (name) => [name, typeof window.electron?.[name]],
        ),
      ),
    },
    ready: false,
    rawChallenge: null,
    frameInserted: false,
    armed: false,
    frame: null,
    topIssued: false,
    rawRecords: [],
    terminal: null,
    error: null,
  };
  let frame;
  let started = false;
  let challengeQueued = false;
  const publish = () => {
    const text = JSON.stringify(state);
    if (text.length > 32768) throw new Error("bounded X3 snapshot exceeded");
    document.documentElement.setAttribute("data-keld-original-probe", text);
  };
  const fail = (error) => {
    state.error = String(error).slice(0, 1024);
    publish();
  };
  publish();
  window.addEventListener("error", (event) => fail(event.message));
  window.addEventListener("unhandledrejection", (event) => fail(event.reason));
  if (
    !state.first.top ||
    !native ||
    typeof native.addEventListener !== "function" ||
    typeof native.postMessage !== "function"
  ) {
    fail("actual top document/native port is missing");
    return;
  }

  const coordinate = (value) => {
    try {
      if (value?.probe !== "keld-original-x3")
        throw new Error("foreign fixture coordination");
      if (value.phase === "arm-frame") {
        if (!frame || state.armed || !state.rawChallenge)
          throw new Error("frame arm is out of order");
        state.armed = true;
        const childDocument = frame.contentDocument;
        if (
          !childDocument ||
          frame.hasAttribute("src") ||
          frame.hasAttribute("srcdoc") ||
          frame.contentWindow.location.href !== "about:blank"
        ) {
          throw new Error(
            "initial child document changed before script insertion",
          );
        }
        const script = childDocument.createElement("script");
        script.async = false;
        script.dataset.document = state.rawChallenge;
        script.src = new URL("./original-frame-child.js", location.href).href;
        script.onload = () => {
          try {
            const text = childDocument.documentElement.getAttribute(
              "data-keld-frame-stimulus",
            );
            if (text === null || text.length > 4096)
              throw new Error("bounded child diagnostic is missing");
            state.frame = JSON.parse(text);
            if (state.frame.error !== null) throw new Error(state.frame.error);
            publish();
          } catch (error) {
            fail(error);
          }
        };
        script.onerror = () => fail("child external script did not load");
        childDocument.head.appendChild(script);
      } else if (value.phase === "top-control") {
        if (!state.armed || state.topIssued)
          throw new Error("top control is out of order");
        // Finish this real M->R event before the next original call is issued.
        queueMicrotask(() => {
          try {
            state.topIssued = true;
            window.electron.sendMessage("app-load-finished", {
              probe: "keld-original-x3",
              path: "top",
            });
            publish();
          } catch (error) {
            fail(error);
          }
        });
      } else {
        throw new Error("unknown fixed fixture phase");
      }
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
        event.data.length > 2048 ||
        state.rawRecords.length === 12
      ) {
        throw new Error("actual native observation shape/bound failed");
      }
      state.rawRecords.push({ text: event.data, topIssued: state.topIssued });
      const value = JSON.parse(event.data);
      if (
        value.v === 2 &&
        value.kind === "app-call" &&
        value.call === 1 &&
        value.method === "registerMsgListener"
      ) {
        if (challengeQueued || !/^[0-9a-f]{32}$/.test(value.document))
          throw new Error("actual document epoch is duplicate or malformed");
        challengeQueued = true;
        state.rawChallenge = value.document;
        // The installed runtime registered earlier and invokes the unchanged
        // original Function before this microtask creates the empty iframe.
        queueMicrotask(() => {
          try {
            const insert = () => {
              try {
                if (frame) throw new Error("duplicate initial child");
                frame = document.createElement("iframe");
                frame.setAttribute("title", "Keld X3 initial blank child");
                state.frameInserted = true;
                // No src, srcdoc, navigation, data URL or inline script.
                document.body.appendChild(frame);
                publish();
              } catch (error) {
                fail(error);
              }
            };
            if (document.body) insert();
            else
              document.addEventListener("DOMContentLoaded", insert, {
                once: true,
              });
          } catch (error) {
            fail(error);
          }
        });
      }
      if (
        value.v === 2 &&
        value.kind === "app-call" &&
        value.call === 2 &&
        !state.topIssued
      ) {
        throw new Error("child operation reached the top original Function");
      }
      if (
        state.topIssued &&
        value.v === 2 &&
        ((value.kind === "app-refused" && value.call === 2) ||
          (value.kind === "send-result" && value.message === 1))
      ) {
        if (state.terminal !== null || value.document !== state.rawChallenge) {
          throw new Error("top control terminal is duplicate or foreign");
        }
        state.terminal = value;
      }
      publish();
    } catch (error) {
      fail(error);
    }
  });

  const begin = () => {
    if (started) return;
    started = true;
    state.ready = true;
    try {
      // Already one of the reviewed13 grants; no original mainResp request is
      // fabricated, and ungranted fileChanged is not used.
      window.electron.registerMsgListener("args-obj", coordinate);
      publish();
    } catch (error) {
      fail(error);
    }
  };
  document.addEventListener("__keld_app_ready_data_v1", begin, { once: true });
  document.dispatchEvent(new CustomEvent("__keld_page_ready_query_v1"));
})();
