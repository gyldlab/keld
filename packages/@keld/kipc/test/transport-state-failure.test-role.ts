/** Child-only hostile thrown values exercise the real WorkerLink wake drain. */
import assert from "node:assert/strict";
import {
  ECHO_CHANNEL,
  LIFECYCLE_CHANNEL,
  WorkerLink,
  isCallError,
  kipcError,
  linkError,
  withIoDeadline,
} from "../src/transport.ts";

const mode = process.argv[2];
let hooks = 0;
const hostile = new Proxy(new Error("untrusted proxy message"), {
  getOwnPropertyDescriptor() {
    hooks += 1;
    if (mode === "proxy-return")
      return { configurable: true, value: "KELD-IPC-003: proxy diagnostic" };
    throw new Error("descriptor trap escaped");
  },
  get() {
    hooks += 1;
    throw new Error("property trap escaped");
  },
  getPrototypeOf() {
    hooks += 1;
    throw new Error("prototype trap escaped");
  },
});
const revoked = Proxy.revocable(new Error("revoked"), {});
revoked.revoke();
const accessor = new Error();
Object.defineProperties(accessor, {
  message: {
    get() {
      hooks += 1;
      throw new Error("message accessor escaped");
    },
  },
  code: {
    get() {
      hooks += 1;
      throw new Error("code accessor escaped");
    },
  },
});
const thrown: unknown =
  mode === "revoked"
    ? revoked.proxy
    : mode === "accessor"
      ? accessor
      : mode === "typed"
        ? linkError("KELD-IPC-003", "typed framework failure")
        : mode === "message"
          ? kipcError("KELD-IPC-003", "message framework failure")
          : mode === "string"
            ? "primitive thrown text"
            : mode === "object"
              ? { code: "KELD-IPC-003", message: "plain object" }
              : hostile;
const detail =
  mode === "typed"
    ? "typed framework failure"
    : mode === "message"
      ? "message framework failure"
      : mode === "object"
        ? "plain object"
        : "state applier failed";
const cause =
  mode === "typed" || mode === "message" || mode === "object"
    ? "KELD-IPC-003"
    : "KELD-IPC-005";
const link = await WorkerLink.open({
  link: process.env.KELD_APP_LINK!,
  receive: { eventChannels: [LIFECYCLE_CHANNEL], callReceivers: [] },
});
const encoder = new TextEncoder();
const decoder = new TextDecoder();
let applied = 0;
const delivered: number[] = [];
link.setStateApplier(LIFECYCLE_CHANNEL, (payload) => {
  applied += 1;
  if (payload[0] === 1) throw thrown;
});
link.onEvent(LIFECYCLE_CHANNEL, (payload) => delivered.push(payload[0]));
const ended = new Promise<Error>((resolve) => link.onEnd(resolve));
try {
  const earlier = link.call(ECHO_CHANNEL, encoder.encode("earlier"), 10_000);
  const pending = link
    .call(ECHO_CHANNEL, encoder.encode("unanswered"), 10_000)
    .then(
      () => "unexpected reply",
      (error: unknown) => error,
    );
  let parked: unknown;
  try {
    link.callBlocking(ECHO_CHANNEL, encoder.encode("fault"), 10_000);
    assert.fail("failed state application returned a reply");
  } catch (error) {
    parked = error;
  }
  assert.equal(hooks, 0, "state failure inspection ran application hooks");
  assert.ok(isCallError(parked));
  assert.equal(parked.code, "KELD-IPC-022");
  assert.ok(
    parked.message.includes(
      `${cause}: the state applier for channel ${LIFECYCLE_CHANNEL} threw: ${detail}`,
    ),
  );
  assert.equal(delivered.length, 0, "listeners ran before synchronous return");
  const [answer, unanswered, end] = await withIoDeadline(
    Promise.all([earlier, pending, ended]),
    10_000,
  );
  assert.equal(decoder.decode(answer), "earlier-answer");
  assert.ok(isCallError(unanswered));
  assert.equal(unanswered.code, "KELD-IPC-022");
  assert.equal(unanswered.message, parked.message);
  assert.ok(isCallError(end));
  assert.equal(end.code, cause);
  assert.equal(applied, 2);
  assert.deepEqual(delivered, [0]);
  assert.throws(
    () => link.callBlocking(ECHO_CHANNEL, encoder.encode("later"), 1_000),
    (error: unknown) =>
      isCallError(error) &&
      error.code === "KELD-IPC-022" &&
      error.message === parked.message,
  );
  assert.equal(hooks, 0);
  console.log(
    JSON.stringify({ mode, hooks, applied, delivered, cause, detail }),
  );
} finally {
  link.close();
}
