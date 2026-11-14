/** Local API behavior only; this model is never Host/policy/native qualification. */
import { expect, test } from "bun:test";
import { fileURLToPath } from "node:url";
function observe(phase: string, schema = 5) {
  const run = Bun.spawnSync({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      fileURLToPath(
        new URL("../fixtures/session_recording_model.ts", import.meta.url),
      ),
      phase,
      String(schema),
    ],
    stdout: "pipe",
    stderr: "pipe",
    timeout: 4000,
  });
  expect(run.exitCode, run.stdout.toString() + run.stderr.toString()).toBe(0);
  expect(run.stderr.toString()).toBe("");
  return JSON.parse(run.stdout.toString());
}
function methodRefusal(method: string) {
  return {
    status: "threw",
    code: "KELD-COMPAT-013",
    member: `session.defaultSession.webRequest.${method}`,
    ticket: "GH-553",
  };
}
test("pre-Ready defaultSession remains the typed GH553 property refusal", () => {
  const value = observe("pre-ready");
  expect(value.result).toEqual({
    status: "threw",
    code: "KELD-COMPAT-013",
    member: "session.defaultSession",
    ticket: "GH-553",
  });
  expect(value.diagnostics).toEqual([]);
  expect(value.sourceReads).toBe(0);
});
test("old metadata and Ready without a prepared-source descriptor never accept hooks", () => {
  for (const phase of ["missing", "missing-descriptor"]) {
    const value = observe(phase);
    expect(value.stable).toBe(true);
    expect(value.results).toEqual([
      methodRefusal("onHeadersReceived"),
      methodRefusal("onBeforeRequest"),
    ]);
    expect(value.diagnostics).toEqual([]);
    expect(value.listenerCalls).toBe(0);
    expect(value.sourceReads).toBe(0);
  }
});
test("local qualified model records two exact diagnostics, invokes and retains zero listeners", () => {
  const value = observe("qualified");
  expect(value.stable).toBe(true);
  expect(value.results).toEqual([
    { status: "returned", value: "undefined" },
    { status: "returned", value: "undefined" },
  ]);
  expect(value.listenerCalls).toBe(0);
  expect(value.retained).toEqual([false, false]);
  expect(value.diagnostics).toEqual(
    ["onHeadersReceived", "onBeforeRequest"].map(
      (method) =>
        `Keld ▲ GH-553 session.defaultSession.webRequest.${method}: listener ignored; verified Host response CSP and native resource confinement replace this hook.`,
    ),
  );
  expect(value.sourceReads).toBe(2);
  for (const method of [
    "onBeforeSendHeaders",
    "onSendHeaders",
    "onResponseStarted",
    "onBeforeRedirect",
    "onCompleted",
    "onErrorOccurred",
  ])
    expect(value.unsupported[method]).toEqual(methodRefusal(method));
});
test("null registration keeps native policy and records exactly once per call", () => {
  const value = observe("null");
  expect(value.results).toEqual([
    { status: "returned", value: "undefined" },
    { status: "returned", value: "undefined" },
  ]);
  expect(value.diagnostics).toHaveLength(2);
  expect(value.listenerCalls).toBe(0);
});
test("ended Source propagates its actual fault and records no accepted registration", () => {
  const value = observe("dead-source");
  expect(value.results).toEqual([
    { status: "threw", code: "KELD-IPC-005" },
    { status: "threw", code: "KELD-IPC-005" },
  ]);
  expect(value.diagnostics).toEqual([]);
  expect(value.listenerCalls).toBe(0);
});

test("malformed registrations refuse without getters, callbacks or diagnostics", () => {
  const value = observe("invalid");
  expect(value.results).toEqual([
    methodRefusal("onHeadersReceived"),
    methodRefusal("onBeforeRequest"),
  ]);
  expect(value.getters).toBe(0);
  expect(value.listenerCalls).toBe(0);
  expect(value.diagnostics).toEqual([]);
});
test("diagnostic sink failure propagates without reporting accepted registration", () => {
  const value = observe("diagnostic-fault");
  expect(value.results).toEqual([
    { status: "threw", code: "fixture-diagnostic-sink" },
    { status: "threw", code: "fixture-diagnostic-sink" },
  ]);
  expect(value.diagnostics).toEqual([]);
  expect(value.listenerCalls).toBe(0);
});

test("installed schema7 retains the same Ready, descriptor and live-source boundary", () => {
  const qualified = observe("qualified", 7);
  expect(qualified.results).toEqual([
    { status: "returned", value: "undefined" },
    { status: "returned", value: "undefined" },
  ]);
  expect(qualified.diagnostics).toHaveLength(2);
  expect(qualified.listenerCalls).toBe(0);
  expect(qualified.retained).toEqual([false, false]);
  expect(qualified.sourceReads).toBe(2);
  const missing = observe("missing-descriptor", 7);
  expect(missing.results).toEqual([
    methodRefusal("onHeadersReceived"),
    methodRefusal("onBeforeRequest"),
  ]);
  expect(missing.diagnostics).toEqual([]);
  expect(missing.sourceReads).toBe(0);
  const beforeReady = observe("pre-ready", 7);
  expect(beforeReady.result).toEqual({
    status: "threw",
    code: "KELD-COMPAT-013",
    member: "session.defaultSession",
    ticket: "GH-553",
  });
  expect(beforeReady.sourceReads).toBe(0);
  const ended = observe("dead-source", 7);
  expect(ended.results).toEqual([
    { status: "threw", code: "KELD-IPC-005" },
    { status: "threw", code: "KELD-IPC-005" },
  ]);
  expect(ended.diagnostics).toEqual([]);
  expect(ended.listenerCalls).toBe(0);
});

test("Windows schema8 keeps exact recorded hooks and every availability refusal", () => {
  const accepted = observe("qualified", 8);
  expect(accepted.results).toEqual([
    { status: "returned", value: "undefined" },
    { status: "returned", value: "undefined" },
  ]);
  expect(accepted.diagnostics).toEqual(
    ["onHeadersReceived", "onBeforeRequest"].map(
      (method) =>
        "Keld ▲ GH-553 session.defaultSession.webRequest." +
        method +
        ": listener ignored; verified Host response CSP and native resource confinement replace this hook.",
    ),
  );
  expect(accepted.listenerCalls).toBe(0);
  expect(accepted.retained).toEqual([false, false]);
  expect(accepted.sourceReads).toBe(2);
  for (const method of Object.keys(accepted.unsupported))
    expect(accepted.unsupported[method]).toEqual(methodRefusal(method));
  for (const phase of [
    "missing-descriptor",
    "unsupported-descriptor",
    "desktop-disabled",
  ]) {
    const denied = observe(phase, 8);
    expect(denied.results).toEqual([
      methodRefusal("onHeadersReceived"),
      methodRefusal("onBeforeRequest"),
    ]);
    expect(denied.diagnostics).toEqual([]);
    expect(denied.listenerCalls).toBe(0);
    expect(denied.sourceReads).toBe(0);
  }
  const before = observe("pre-ready", 8);
  expect(before.result).toEqual({
    status: "threw",
    code: "KELD-COMPAT-013",
    member: "session.defaultSession",
    ticket: "GH-553",
  });
  expect(before.sourceReads).toBe(0);
  const ended = observe("dead-source", 8);
  expect(ended.results).toEqual([
    { status: "threw", code: "KELD-IPC-005" },
    { status: "threw", code: "KELD-IPC-005" },
  ]);
  expect(ended.diagnostics).toEqual([]);
  expect(ended.listenerCalls).toBe(0);
  const invalid = observe("invalid", 8);
  expect(invalid.results).toEqual([
    methodRefusal("onHeadersReceived"),
    methodRefusal("onBeforeRequest"),
  ]);
  expect(invalid.getters).toBe(0);
  expect(invalid.diagnostics).toEqual([]);
  const failedDiagnostic = observe("diagnostic-fault", 8);
  expect(failedDiagnostic.results).toEqual([
    { status: "threw", code: "fixture-diagnostic-sink" },
    { status: "threw", code: "fixture-diagnostic-sink" },
  ]);
  expect(failedDiagnostic.diagnostics).toEqual([]);
  expect(failedDiagnostic.listenerCalls).toBe(0);
});
