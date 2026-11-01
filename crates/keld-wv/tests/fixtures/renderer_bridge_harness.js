// GH-1095: runs the shipped KEL-142 page facade and isolated-world scripts,
// exactly as keld-wv renders them, against a model of one WebKit document,
// under Bun's JavaScriptCore. The Rust test prepends three constants:
// PAGE_SCRIPT, ISOLATED_SCRIPT and INPUT ({ scenario, ... }).
//
// The model holds only what the scripts touch: one `window` per content world
// sharing one message bus (WebKit delivers a postMessage to the listeners of
// every world, with `event.source` equal to that world's own `window`), the
// isolated world's native message handler, and `pagehide`. It proves the
// scripts' state logic. WebKit's world isolation is proven by the hosted macOS
// tests, not here. A scenario waits only for the bus to drain; nothing sleeps.
"use strict";

// The harness's own Error, so its observer is immune to a scenario that
// replaces the global one.
const GenuineError = Error;

function makeDocument() {
  const tasks = [];
  const listeners = [];
  const native = [];
  const relays = [];
  const makeWindow = () => {
    const win = {
      postMessage(data) {
        const copy = structuredClone(data);
        tasks.push(() => {
          for (const listener of listeners.slice()) {
            if (listener.type === "message") listener.fn({ source: listener.win, data: copy });
          }
        });
      },
      addEventListener(type, fn) {
        listeners.push({ type, fn, win });
      },
    };
    return win;
  };
  const pageWindow = makeWindow();
  const isolatedWindow = makeWindow();
  isolatedWindow.webkit = {
    messageHandlers: {
      __keld_wv_link_v1: { postMessage: (text) => native.push(JSON.parse(text)) },
    },
  };
  const isolatedGlobal = {};
  new Function("window", PAGE_SCRIPT)(pageWindow);
  new Function("window", "globalThis", ISOLATED_SCRIPT)(isolatedWindow, isolatedGlobal);
  // An observer in the page world: every result relay the page could see.
  pageWindow.addEventListener("message", (event) => {
    if (event.data && event.data.__keldRelay === "result-v1") relays.push(event.data);
  });
  return {
    keld: pageWindow.keld,
    native,
    relays,
    bind: (nonce) => isolatedGlobal.__keldBridgeNativeBind(nonce),
    result: (value) => isolatedGlobal.__keldBridgeNativeResult(value),
    pagehide: () => {
      for (const listener of listeners.slice()) {
        if (listener.type === "pagehide" && listener.win === pageWindow) listener.fn({});
      }
    },
    // A page script that skips the facade and relays its own invoke.
    relayInvoke: (call) => pageWindow.postMessage({ __keldRelay: "invoke-v1", ...call }),
    // Runs queued message deliveries and the promise reactions they cause
    // until neither is left.
    async flush() {
      for (;;) {
        await Promise.resolve();
        await Promise.resolve();
        const task = tasks.shift();
        if (task === undefined) return;
        task();
      }
    },
  };
}

function describe(error) {
  const descriptor = Object.getOwnPropertyDescriptor(error, "code");
  return {
    ok: false,
    isError: error instanceof GenuineError,
    name: error.name,
    message: error.message,
    code: error.code === undefined ? null : error.code,
    own: descriptor !== undefined,
    enumerable: descriptor ? descriptor.enumerable : null,
    writable: descriptor ? descriptor.writable : null,
    configurable: descriptor ? descriptor.configurable : null,
    keys: Object.keys(error),
    json: JSON.stringify(error),
    string: String(error),
  };
}

function track(promise) {
  const settled = [];
  promise.then(
    (value) => settled.push({ ok: true, value: Array.from(value) }),
    (error) => settled.push(describe(error)),
  );
  return settled;
}

async function rejection(promise) {
  try {
    await promise;
  } catch (error) {
    return describe(error);
  }
  throw new Error("harness: expected a rejection");
}

const lastInvoke = (doc) => doc.native.filter((envelope) => envelope.kind === "invoke").at(-1);

async function errors(input) {
  const doc = makeDocument();
  doc.bind(input.nonce);
  const out = {};
  out.opts = await rejection(doc.keld.invoke(input.channel, new Uint8Array(), {}));
  out.channel = await rejection(doc.keld.invoke(0, new Uint8Array()));
  out.payload = await rejection(doc.keld.invoke(input.channel, [1]));
  // One host result per entry of input.results, each for its own invoke.
  out.results = [];
  for (const result of input.results) {
    const settled = track(doc.keld.invoke(input.channel, new Uint8Array([1])));
    await doc.flush();
    if (out.busy === undefined) {
      out.busy = await rejection(doc.keld.invoke(input.channel, new Uint8Array([2])));
    }
    const sent = lastInvoke(doc);
    doc.result({ ...result, document: input.nonce, request: sent.request });
    await doc.flush();
    out.results.push(settled);
  }
  const hidden = track(doc.keld.invoke(input.channel, new Uint8Array([3])));
  await doc.flush();
  doc.pagehide();
  await doc.flush();
  out.pagehide = hidden;
  // The oracle's own control: a plain Error carries no code.
  out.control = describe(new Error("KELD-WV-011: control"));
  return out;
}

async function settlement(input) {
  // The predecessor document's invoke is in flight when it navigates away.
  const predecessor = makeDocument();
  predecessor.bind(input.predecessor);
  const predecessorCall = track(predecessor.keld.invoke(input.channel, new Uint8Array([1])));
  await predecessor.flush();
  predecessor.pagehide();
  await predecessor.flush();

  // The successor binds and has its own call in flight with the same id.
  const successor = makeDocument();
  successor.bind(input.successor);
  const successorCall = track(successor.keld.invoke(input.channel, new Uint8Array([2])));
  await successor.flush();
  const sent = lastInvoke(successor);

  // Native then refuses the predecessor's stale invoke into the current world.
  successor.result(input.staleRefusal);
  await successor.flush();
  const afterStale = successorCall.slice();

  // A result for the successor's document but another request id.
  successor.result({ ...input.ownReply, request: sent.request + 1 });
  await successor.flush();
  const afterWrongRequest = successorCall.slice();
  // A page script now relays its own invoke past the facade: the isolated
  // world must still hold the real call in flight and refuse it as busy.
  successor.relayInvoke({ request: 99, channel: input.channel, payload: [9] });
  await successor.flush();
  const nativeInvokes = successor.native.filter((envelope) => envelope.kind === "invoke").length;

  // The successor's own result, then a duplicate of it.
  successor.result({ ...input.ownReply, request: sent.request });
  await successor.flush();
  successor.result({ ...input.ownReply, request: sent.request });
  await successor.flush();

  return {
    predecessor: predecessorCall,
    sent,
    afterStale,
    afterWrongRequest,
    nativeInvokes,
    successor: successorCall,
    relays: successor.relays,
  };
}

async function reset(input) {
  // A host navigate() whose load never commits: the current document stays
  // live and receives the reset result for its own pending call.
  const doc = makeDocument();
  doc.bind(input.nonce);
  const call = track(doc.keld.invoke(input.channel, new Uint8Array([1])));
  await doc.flush();
  doc.result({ ...input.reset, request: lastInvoke(doc).request });
  await doc.flush();
  const afterReset = call.slice();
  // The load commits after all: pagehide finds nothing left to settle.
  doc.pagehide();
  await doc.flush();

  // A successor document drops a reset addressed to its predecessor.
  const successor = makeDocument();
  successor.bind(input.successor);
  const successorCall = track(successor.keld.invoke(input.channel, new Uint8Array([2])));
  await successor.flush();
  successor.result({ ...input.reset, request: lastInvoke(successor).request });
  await successor.flush();
  return { afterReset, settled: call, successor: successorCall };
}

async function tampered(input) {
  // A page script replaces the global Error after the facade installed. Every
  // rejection must still settle, as a genuine coded Error.
  const doc = makeDocument();
  doc.bind(input.nonce);
  const forged = function ForgedError() {
    return Object.freeze({ forged: true });
  };
  const withForgedError = (action) => {
    globalThis.Error = forged;
    try {
      return { threw: null, value: action() };
    } catch (error) {
      return { threw: String(error), value: undefined };
    } finally {
      globalThis.Error = GenuineError;
    }
  };
  const out = {};
  const local = withForgedError(() => doc.keld.invoke(input.channel, new Uint8Array(), {}));
  out.localThrew = local.threw;
  out.local = local.value ? await rejection(local.value) : null;

  const answered = track(doc.keld.invoke(input.channel, new Uint8Array([1])));
  await doc.flush();
  doc.result({ ...input.result, document: input.nonce, request: lastInvoke(doc).request });
  // The relay is a queued message: deliver it while Error is forged.
  globalThis.Error = forged;
  try {
    await doc.flush();
    out.resultThrew = null;
  } catch (error) {
    out.resultThrew = String(error);
  } finally {
    globalThis.Error = GenuineError;
  }
  out.result = answered;

  const hidden = track(doc.keld.invoke(input.channel, new Uint8Array([2])));
  await doc.flush();
  out.pagehideThrew = withForgedError(() => doc.pagehide()).threw;
  await doc.flush();
  out.pagehide = hidden;
  return out;
}

const scenarios = { errors, settlement, reset, tampered };
console.log(JSON.stringify(await scenarios[INPUT.scenario](INPUT)));
