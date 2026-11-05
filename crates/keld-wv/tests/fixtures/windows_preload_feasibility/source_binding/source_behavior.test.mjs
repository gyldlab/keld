// Preparation evidence only. No production classifier, facade, bridge or native oracle.
import assert from "node:assert/strict";
import test from "node:test";
import {
  CORPUS,
  caseById,
  runCase,
  authenticPreload,
} from "./source_behavior_support.mjs";

test("manual corpus retains the 32 pinned source-shape cases", () => {
  const expected = [
    "F-A",
    "F-B",
    "F-B1",
    "F-B2",
    "F-C",
    "F-D",
    "F-E",
    "F-F",
    "F-G",
    "F-G1",
    "F-H",
    "F-I",
    "F-J",
    "F-K",
    "F-L",
    "F-M",
    "F-N",
    "F-O",
    "F-P",
    "F-Q",
    "F-R",
    "F-S",
    "F-T",
    "F-U",
    "F-V",
    "F-W",
    "F-X",
    "F-Y",
    "F-Z",
    "F-Z1",
    "F-Z2",
    "F-Z3",
  ];
  assert.deepEqual(
    CORPUS.cases.map((item) => item.id),
    expected,
  );
  assert.match(CORPUS.scope, /not classifier output/);
});

test("F-B forwards every argument after the channel", () => {
  const h = runCase("F-B");
  const payload = [{ tag: "first" }, "second", { tag: "third" }];
  h.api.sendMessage("fixture-channel", ...payload);
  assert.deepEqual(h.sends, [["fixture-channel", ...payload]]);
  assert.deepEqual(caseById("F-B").expected_rules, []);
});

test("F-B1 literal sink forwards every argument", () => {
  const h = runCase("F-B1");
  h.api.doA("one", "two", "three");
  assert.deepEqual(h.sends, [["a", "one", "two", "three"]]);
});

test("F-C omission and F-J truncation visibly lose supplied arguments", () => {
  const omitted = runCase("F-C");
  omitted.api.openFile("first", "second", "third");
  assert.deepEqual(omitted.sends, [["open-file"]]);
  assert.deepEqual(caseById("F-C").expected_rules, ["a"]);
  const truncated = runCase("F-J");
  truncated.api.sendMessage("fixture-channel", "first", "second", "third");
  assert.deepEqual(truncated.sends, [["fixture-channel", "first"]]);
  assert.deepEqual(caseById("F-J").expected_rules, ["a"]);
});

test("F-B2 excludes only the event and preserves all inbound values", () => {
  const h = runCase("F-B2");
  const received = [];
  h.api.listen("evt", (...values) => received.push(values));
  h.events.get("evt")({ privateEvent: true }, { one: 1 }, "two", "three");
  assert.deepEqual(received, [[{ one: 1 }, "two", "three"]]);
});

test("inbound filter and field selection have different callback outputs", () => {
  const filtered = runCase("F-F");
  filtered.events.get("evt")({}, { ok: false }, "hidden");
  assert.deepEqual(filtered.callbacks, []);
  filtered.events.get("evt")({}, { ok: true }, "visible");
  assert.deepEqual(filtered.callbacks, [[{ ok: true }, "visible"]]);
  const selected = runCase("F-L");
  const received = [];
  selected.api.onUser((...values) => received.push(values));
  selected.events.get("user")(
    {},
    { name: "name", private: "lost" },
    "trailing",
  );
  assert.deepEqual(received, [["name"]]);
});

test("constant false contextIsolated reaches only public branch", () => {
  const h = runCase("F-O");
  assert.deepEqual(Object.keys(h.exposures), []);
  h.context.window.api.pub("one", "two");
  assert.deepEqual(h.sends, [["pub", "one", "two"]]);
  assert.equal(h.context.process.contextIsolated, false);
});

test("private state and deferred callbacks control actual sink reachability", () => {
  for (const [id, method] of [
    ["F-R", "deleteAll"],
    ["F-Z", "maybe"],
  ]) {
    const closed = runCase(id);
    closed.api[method]("value");
    assert.deepEqual(closed.sends, []);
    const open = runCase(id, { unlocked: true });
    open.api[method]("value");
    assert.equal(open.sends.length, 1);
  }
  for (const [id, method] of [
    ["F-T", "later"],
    ["F-Y", "checked"],
  ]) {
    const h = runCase(id);
    h.api[method]("one", "two");
    assert.deepEqual(h.sends, []);
    assert.equal(h.pending.length, 1);
    h.pending[0]();
    assert.equal(h.sends.length, 1);
    assert.deepEqual(h.sends[0].slice(1), ["one", "two"]);
  }
});

test("authentic mainResp routes success data and consumes each request record", () => {
  const h = authenticPreload();
  const success = [];
  const error = [];
  const request = { action: "fixture" };
  h.api.request(
    request,
    (...values) => success.push(values),
    (...values) => error.push(values),
  );
  const data = { visible: true };
  h.emit(
    "mainResp",
    { reqId: request.reqId, data, ignored: "field" },
    "trailing",
  );
  assert.deepEqual(success, [[data]]);
  assert.deepEqual(error, []);
  assert.throws(
    () => h.emit("mainResp", { reqId: request.reqId, data }),
    /callback/,
  );
});

test("authentic mainResp error selects msg and e instead of whole response", () => {
  const h = authenticPreload();
  const success = [];
  const errors = [];
  const request = { action: "fixture" };
  h.api.request(
    request,
    (...values) => success.push(values),
    (...values) => errors.push(values),
  );
  const detail = { code: "fixture-error" };
  h.emit(
    "mainResp",
    {
      reqId: request.reqId,
      error: true,
      msg: "message",
      e: detail,
      data: "lost",
    },
    "trailing",
  );
  assert.deepEqual(success, []);
  assert.deepEqual(errors, [["message", detail]]);
});

test("authentic fileChanged filters unmatched path and selects curr/prev", () => {
  const h = authenticPreload();
  const changes = [];
  h.api.request(
    {
      action: "watchFile",
      path: "owned-path",
      listener: (...values) => changes.push(values),
    },
    () => {},
    () => {},
  );
  h.emit("fileChanged", {
    path: "other-path",
    curr: "ignored-current",
    prev: "ignored-previous",
  });
  assert.deepEqual(changes, []);
  const current = { version: 2 };
  const previous = { version: 1 };
  h.emit(
    "fileChanged",
    { path: "owned-path", curr: current, prev: previous, other: "lost" },
    "trailing",
  );
  assert.deepEqual(changes, [[current, previous]]);
});

test("authentic registerMsgListener drops trailing inbound values", () => {
  const h = authenticPreload();
  const callbacks = [];
  h.api.registerMsgListener("fixture-event", (...values) =>
    callbacks.push(values),
  );
  const first = { visible: 1 };
  h.emit("fixture-event", first, "second", "third");
  assert.deepEqual(callbacks, [[first]]);
});

test("authentic listenOnce drops trailing values and delivers once", () => {
  const h = authenticPreload();
  const callbacks = [];
  h.api.listenOnce("fixture-once", (...values) => callbacks.push(values));
  const first = { visible: 1 };
  h.emit("fixture-once", first, "second", "third");
  h.emit("fixture-once", "later");
  assert.deepEqual(callbacks, [[first]]);
});
