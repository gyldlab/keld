// Real facade/API/transport conformance for approved GH451 D2. No mocks, synthetic emits,
// Host endpoint, Ready/window facts, native code or original application.
import assert from "node:assert/strict";

const scenario = process.argv[2];
assert.equal(process.env.KELD_APP_LINK, undefined);
const { app } = await import("../src/app.ts");
type QuitEvent = { readonly defaultPrevented: boolean; preventDefault(): void };
const trace: string[] = [];
const errors: string[] = [];
const callbackAssertionFailures: unknown[] = [];
let calls = 0;
let nested: Promise<void> | undefined;
const events: QuitEvent[] = [];

// Production isolates callback throws. Keep assertion failures independently
// visible so that fail-closed listener handling cannot make a broken test pass.
function callbackCheck<T>(check: () => T): T {
  try {
    return check();
  } catch (error) {
    callbackAssertionFailures.push(error);
    throw error;
  }
}

function checked(event: unknown): QuitEvent {
  const actual = callbackCheck(() => {
    assert.equal(typeof event, "object");
    assert.notEqual(event, null);
    const actual = event as QuitEvent;
    assert.equal(typeof actual.preventDefault, "function");
    assert.equal(typeof actual.defaultPrevented, "boolean");
    return actual;
  });
  events.push(actual);
  calls++;
  trace.push("before-quit");
  return actual;
}

const veto = (event: unknown): void => {
  const actual = checked(event);
  callbackCheck(() => {
    assert.equal(actual.defaultPrevented, false);
    actual.preventDefault();
    assert.equal(actual.defaultPrevented, true);
  });
};

async function request(): Promise<Promise<void>[]> {
  const pending = app.quit();
  assert.ok(pending instanceof Promise);
  trace.push("returned");
  try {
    await pending;
  } catch (error) {
    errors.push(error instanceof Error ? error.message : String(error));
  }
  return [pending];
}

if (scenario === "veto" || scenario === "native-pre-ready-veto") {
  app.on("before-quit", veto);
  await request();
  assert.equal(calls, 1);
  assert.deepEqual(trace, ["before-quit", "returned"]);
  assert.deepEqual(errors, []);
} else if (scenario === "repeated-veto") {
  app.on("before-quit", veto);
  const [first] = await request();
  const [second] = await request();
  assert.notEqual(first, second);
  assert.notEqual(events[0], events[1]);
  assert.equal(calls, 2);
  assert.deepEqual(errors, []);
} else if (scenario === "reentrant-veto") {
  app.on("before-quit", (event: unknown) => {
    const actual = checked(event);
    nested = app.quit();
    actual.preventDefault();
  });
  const [outer] = await request();
  assert.equal(nested, outer);
  assert.equal(calls, 1);
  assert.deepEqual(errors, []);
} else if (scenario === "throw-retry") {
  const failure = new Error("test-owned callback failure");
  const throws = (event: unknown): void => {
    checked(event);
    throw failure;
  };
  app.on("before-quit", throws);
  await request();
  app.off("before-quit", throws);
  app.on("before-quit", veto);
  await request();
  assert.equal(calls, 2);
  assert.deepEqual(errors, []);
} else if (scenario === "listener-snapshot") {
  let removedCalls = 0;
  const removed = (event: unknown): void => {
    callbackCheck(() =>
      assert.equal((event as QuitEvent).defaultPrevented, true),
    );
    removedCalls++;
  };
  app.on("before-quit", (event: unknown) => {
    veto(event);
    app.removeListener("before-quit", removed);
  });
  app.on("before-quit", removed);
  await request();
  await request();
  assert.equal(removedCalls, 1);
  assert.equal(calls, 2);
  assert.deepEqual(errors, []);
} else if (scenario === "coalesced-terminal-failure") {
  app.on("before-quit", checked);
  const first = app.quit();
  const second = app.quit();
  assert.equal(first, second);
  await assert.rejects(first, /^Error: KELD-IPC-007:/);
  assert.equal(calls, 1);
  await assert.rejects(app.quit(), /^Error: KELD-IPC-007:/);
  assert.equal(calls, 2);
} else if (
  scenario === "native-pre-ready-refusal" ||
  scenario === "missing-link-refusal"
) {
  app.on("before-quit", checked);
  await request();
  assert.equal(calls, 1);
  assert.equal(errors.length, 1);
  assert.match(
    errors[0]!,
    scenario === "native-pre-ready-refusal"
      ? /^KELD-COMPAT-012:/
      : /^KELD-IPC-007:/,
  );
} else {
  assert.fail(`unknown scenario ${scenario}`);
}
assert.deepEqual(
  callbackAssertionFailures,
  [],
  "callback assertions must survive production exception isolation",
);
assert.equal(app.isReady(), false, "no fabricated Host Ready");
console.log(
  JSON.stringify({
    scenario,
    status: "PASS",
    calls,
    errors,
    trace,
    proof:
      "genuine app.quit callback dispatch; real canonical API and transport, absent Host endpoint",
  }),
);
