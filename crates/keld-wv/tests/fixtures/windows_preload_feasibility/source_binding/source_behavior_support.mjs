// Test-only stubs. VM execution here is not an isolation or Host-authority claim.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import vm from "node:vm";

export const ROOT = new URL(".", import.meta.url);
export const CORPUS = JSON.parse(
  readFileSync(
    process.env.W1_FIXTURE_CORPUS ?? new URL("classifier_cases.json", ROOT),
    "utf8",
  ),
);
export const caseById = (id) => {
  const item = CORPUS.cases.find((entry) => entry.id === id);
  assert.ok(item, "missing fixture " + id);
  return item;
};

// These authored snippets exercise source-shape consequences, not a classifier.
export function runCase(id, extra = {}) {
  assert.ok(!["F-E", "F-M", "F-N"].includes(id), "source-data-only fixture");
  const sends = [];
  const events = new Map();
  const callbacks = [];
  const exposures = {};
  const pending = [];
  const context = vm.createContext(
    {
      send: (...values) => sends.push(values),
      on: (channel, callback) => events.set(channel, callback),
      cb: (...values) => callbacks.push(values),
      contextBridge: {
        exposeInMainWorld: (name, value) => {
          exposures[name] = value;
        },
      },
      process: { contextIsolated: false },
      window: {},
      ALLOWED: new Set(["allowed"]),
      unlocked: false,
      locked: false,
      featureOn: false,
      L: {},
      n: 1,
      USER: "fixture-user",
      token: undefined,
      setTimeout: (callback) => pending.push(callback),
      ready: (callback) => pending.push(callback),
      ...extra,
    },
    { codeGeneration: { strings: false, wasm: false } },
  );
  const api = new vm.Script(caseById(id).source, {
    filename: id + ".fixture.js",
  }).runInContext(context, { timeout: 1000 });
  return { api, sends, events, callbacks, exposures, pending, context };
}

// Only the pinned 3,201-byte preload runs. Main/config/packaging never execute.
export function authenticPreload() {
  const source = readFileSync(
    new URL("vendor/drawio/src/main/electron-preload.js", ROOT),
  );
  assert.equal(source.length, 3201);
  assert.equal(
    createHash("sha256").update(source).digest("hex"),
    "1091fd21ef727a52ef89bde814aa662eed4a0c3ce32beb858a068a96b285bf98",
  );
  const exposures = {};
  const sends = [];
  const listeners = [];
  const ipcRenderer = {
    send: (...values) => sends.push(values),
    on: (channel, callback) =>
      listeners.push({ channel, callback, once: false }),
    once: (channel, callback) =>
      listeners.push({ channel, callback, once: true }),
  };
  const context = vm.createContext(
    {
      require: (name) => {
        assert.equal(
          name,
          "electron",
          "fixture permits only the Electron stub",
        );
        return {
          ipcRenderer,
          contextBridge: {
            exposeInMainWorld: (key, value) => {
              exposures[key] = value;
            },
          },
        };
      },
      process: { argv: [], type: "renderer", versions: {} },
      window: { navigation: null },
      console: {
        error: (...values) => {
          throw Error("unexpected preload error: " + values);
        },
      },
    },
    { codeGeneration: { strings: false, wasm: false } },
  );
  new vm.Script(source.toString("utf8"), {
    filename: "pinned-electron-preload.js",
  }).runInContext(context, { timeout: 1000 });
  assert.equal(typeof exposures.electron?.request, "function");
  function emit(channel, ...payload) {
    const matches = listeners.filter((entry) => entry.channel === channel);
    for (const entry of matches) {
      if (entry.once) listeners.splice(listeners.indexOf(entry), 1);
      entry.callback(Object.freeze({ fixtureEvent: true }), ...payload);
    }
  }
  return { api: exposures.electron, sends, emit };
}
