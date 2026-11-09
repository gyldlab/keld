/**
 * Bun role fixture for the GH-528 T1 harness (spec gh527 §3, §7).
 *
 * `crates/keld-ipc/tests/worker_link.rs` is the host: it mints `KELD_APP_LINK`,
 * spawns `bun worker-link-role.ts <scenario>` with `KELD_KIPC_TEST_HOOKS=1`,
 * and drives the link with the real `keld_ipc::link::write_frame` writer. This
 * file is the role half. It prints one `KELD_WL key=value` line per
 * observation; the host asserts them. Every wait here is on an observable
 * (a listener count, a settled call, a control word); none is a sleep.
 */
import {
  Worker,
  isMainThread,
  parentPort,
  workerData,
} from "node:worker_threads";

import {
  ECHO_CHANNEL,
  FrameKind,
  LIFECYCLE_CHANNEL,
  WORKER_HEARTBEAT_INTERVAL_MS,
  WORKER_LINK_CONTROL,
  WORKER_LINK_TEST_WORDS,
  WORKER_LIVENESS_WINDOW_MS,
  WorkerLink,
  isCallError,
  kipcError,
  parseAppLink,
  quitAndCloseLink,
  type WorkerLinkOptions,
  type WorkerReceiveTable,
} from "../src/transport.ts";
import {
  openWorkerLinkForTest,
  type WorkerLinkTestHooks,
} from "../src/test-hooks.ts";
import { attachMirror, type MirrorSource } from "../../api/src/mirror.ts";
import { MIRROR_FACT_MARKER, mirrorRetention } from "./mirror-retention.ts";

const encoder = new TextEncoder();
const decoder = new TextDecoder();

function report(key: string, value: string | number | boolean): void {
  console.log(`KELD_WL ${key}=${value}`);
}

function text(value: string): Uint8Array {
  return encoder.encode(value);
}

function requireLink(): string {
  const link = process.env.KELD_APP_LINK;
  if (link === undefined || link.length === 0)
    throw new Error("KELD_APP_LINK is unset");
  return link;
}

function codeOf(err: unknown): string {
  if (isCallError(err)) return err.code;
  return err instanceof Error
    ? `untyped:${err.message}`
    : `untyped:${String(err)}`;
}

/** Runs `fn`; reports `<key>-code` and whether it returned, and returns the code. */
function expectThrow(key: string, fn: () => unknown): string {
  try {
    const value = fn();
    report(`${key}-returned`, true);
    report(
      `${key}-value`,
      value instanceof Uint8Array ? decoder.decode(value) : String(value),
    );
    return "returned";
  } catch (err) {
    const code = codeOf(err);
    report(`${key}-code`, code);
    report(`${key}-returned`, false);
    if (
      err instanceof Error &&
      err.message.includes("KELD-IPC-005") &&
      code !== "KELD-IPC-005"
    ) {
      report(`${key}-cause-005`, true);
    }
    return code;
  }
}

async function expectReject(
  key: string,
  promise: Promise<unknown>,
): Promise<string> {
  try {
    const value = await promise;
    report(`${key}-returned`, true);
    report(
      `${key}-value`,
      value instanceof Uint8Array ? decoder.decode(value) : String(value),
    );
    return "returned";
  } catch (err) {
    const code = codeOf(err);
    report(`${key}-code`, code);
    report(`${key}-returned`, false);
    if (
      err instanceof Error &&
      err.message.includes("KELD-IPC-005") &&
      code !== "KELD-IPC-005"
    ) {
      report(`${key}-cause-005`, true);
    }
    return code;
  }
}

function seqOf(payload: Uint8Array): number {
  return new DataView(
    payload.buffer,
    payload.byteOffset,
    payload.byteLength,
  ).getUint32(0, true);
}

/** Reports count, order, gaps and duplicates of an EVENT sequence numbered from 0. */
function reportOrder(key: string, seqs: readonly number[]): void {
  report(`${key}-events`, seqs.length);
  let inOrder = true;
  let gaps = 0;
  let dups = 0;
  const seen = new Set<number>();
  for (let i = 0; i < seqs.length; i += 1) {
    if (seqs[i] !== i) inOrder = false;
    if (seen.has(seqs[i])) dups += 1;
    seen.add(seqs[i]);
    if (i > 0 && seqs[i] > seqs[i - 1] + 1) gaps += seqs[i] - seqs[i - 1] - 1;
  }
  report(`${key}-in-order`, inOrder);
  report(`${key}-gaps`, gaps);
  report(`${key}-dups`, dups);
}

/** Resolves once `n` events have reached the listener `collect` returns. */
function collector(n: number): {
  seqs: number[];
  accept: (seq: number) => void;
  listener: (payload: Uint8Array) => void;
  all: Promise<void>;
} {
  const seqs: number[] = [];
  let resolve: () => void = () => undefined;
  const all = new Promise<void>((r) => {
    resolve = r;
  });
  if (n === 0) resolve();
  const accept = (seq: number): void => {
    seqs.push(seq);
    if (seqs.length === n) resolve();
  };
  return { seqs, all, accept, listener: (payload) => accept(seqOf(payload)) };
}

const LIFECYCLE_EVENTS: WorkerReceiveTable = {
  eventChannels: [LIFECYCLE_CHANNEL],
  callReceivers: [],
};
const WITH_ECHO_CALLS: WorkerReceiveTable = {
  eventChannels: [LIFECYCLE_CHANNEL],
  callReceivers: [{ policy: "echoReceiver" }],
};

interface Opened {
  link: WorkerLink;
  control: Int32Array;
  words: Int32Array;
}

async function open(
  options: Partial<WorkerLinkOptions> = {},
  hooks: Omit<WorkerLinkTestHooks, "words"> = {},
): Promise<Opened> {
  const words = new Int32Array(
    new SharedArrayBuffer(WORKER_LINK_TEST_WORDS.LENGTH * 4),
  );
  const { link, control } = await openWorkerLinkForTest(
    { link: requireLink(), receive: LIFECYCLE_EVENTS, ...options },
    { ...hooks, words },
  );
  return { link, control, words };
}

function word(control: Int32Array, index: number): number {
  return Atomics.load(control, index) >>> 0;
}

/** Polls a control word until `done` holds; a bounded observable wait, never a sleep. */
function awaitWord(
  control: Int32Array,
  index: number,
  done: (value: number) => boolean,
): void {
  const scratch = new Int32Array(new SharedArrayBuffer(4));
  for (let i = 0; i < 60_000; i += 1) {
    if (done(word(control, index))) return;
    Atomics.wait(scratch, 0, 0, 1);
  }
  throw new Error(`control word ${index} never satisfied the wait`);
}

function nextTask(): Promise<void> {
  return new Promise((resolve) => setImmediate(resolve));
}

// Criteria 1, 2, 3 and 9: the #418 arm-B load during one park.
const ARM_B_EVENTS = 11_000;
async function armB(): Promise<void> {
  const { link } = await open();
  const events = collector(ARM_B_EVENTS);
  link.onEvent(LIFECYCLE_CHANNEL, events.listener);
  let result: unknown;
  try {
    // About 10 s of load; the deadline leaves room for a slow hosted runner.
    result = link.callBlocking(ECHO_CHANNEL, text("arm-b"), 60_000);
  } catch (err) {
    report("call-code", codeOf(err));
    return;
  }
  report("listeners-before-return", events.seqs.length);
  report("thenable", typeof (result as { then?: unknown }).then === "function");
  report("reply", decoder.decode(result as Uint8Array));
  await events.all;
  reportOrder("arm-b", events.seqs);
}

// Criterion 4: state facts applied before return, listeners after the continuation.
async function wakeRule(): Promise<void> {
  const { link } = await open();
  const log: string[] = [];
  let mirror = -1;
  const events = collector(5);
  attachMirror(link, {
    channel: LIFECYCLE_CHANNEL,
    decode: seqOf,
    apply(seq) {
      mirror = seq;
      log.push(`applier:${mirror}`);
    },
    dispatch(seq) {
      log.push(`listener:${seq}`);
      events.accept(seq);
    },
  });
  let timerCallbacks = 0;
  setTimeout(() => {
    timerCallbacks += 1;
    log.push("timer");
  }, 0);
  link.callBlocking(ECHO_CHANNEL, text("wake"), 30_000);
  log.push("return");
  report("getter-after-return", mirror);
  report("timer-callbacks-at-return", timerCallbacks);
  log.push("continuation");
  await events.all;
  report("log", log.filter((entry) => entry !== "timer").join(","));
}

// GH-531/GH-1142: opaque fixture facts use the existing admitted lifecycle
// channel. This proves the generic mirror, not production window subscription.
function mirrorValues(
  mode: "identity" | "primitives" | "reentrant" | "duplicate",
): () => Promise<void> {
  return async () => {
    const { link, control } = await open();
    type Fact = { seq: number } | null | undefined | 0;
    const applied = new Map<number, Fact>();
    const events = collector(mode === "reentrant" ? 6 : 3);
    let decodes = 0,
      applies = 0,
      dispatches = 0,
      inFlight = 0,
      boundChecks = 0,
      secondDispatches = 0;
    let receiverExact = true,
      objectExact = true,
      primitiveExact = true,
      boundValid = true;
    let callbacksCaptured = true,
      payloadsExact = true;
    const observed: string[] = [];
    const checkBound = (): number => {
      const matchingRetained =
        (word(control, WORKER_LINK_CONTROL.W_RECS) -
          word(control, WORKER_LINK_CONTROL.R_RECS)) >>>
        0;
      const held = applies - dispatches + inFlight;
      if (held > matchingRetained) boundValid = false;
      boundChecks += 1;
      return held;
    };
    const source: MirrorSource<Fact> = {
      channel: LIFECYCLE_CHANNEL,
      decode(payload) {
        if (this !== source) receiverExact = false;
        const seq = seqOf(payload);
        if (mode === "identity" && seq !== 0) payloadsExact = false;
        decodes += 1;
        return mode === "primitives"
          ? ([undefined, null, 0] as const)[seq]
          : { seq };
      },
      apply(fact) {
        if (this !== source) receiverExact = false;
        applied.set(applies, fact);
        applies += 1;
        checkBound();
      },
      dispatch(fact) {
        // Report observer failures to the independent Rust oracle. Throwing here
        // would be a user-listener failure and could strand the fixture's wait.
        if (this !== source) receiverExact = false;
        if (
          !applied.has(dispatches) ||
          !Object.is(applied.get(dispatches), fact)
        )
          objectExact = false;
        observed.push(
          fact !== null && typeof fact === "object"
            ? `object:${fact.seq}`
            : String(fact),
        );
        const seq = dispatches;
        applied.delete(seq);
        dispatches += 1;
        inFlight += 1;
        try {
          checkBound();
          if (
            mode === "primitives" &&
            !Object.is(fact, ([undefined, null, 0] as const)[seq])
          )
            primitiveExact = false;
          if (mode === "reentrant" && seq === 0) {
            report(
              "mirror-nested-reply",
              decoder.decode(
                link.callBlocking(ECHO_CHANNEL, text("mirror-nested"), 30_000),
              ),
            );
            report("mirror-nested-held", checkBound());
            report(
              "mirror-nested-retained",
              (word(control, WORKER_LINK_CONTROL.W_RECS) -
                word(control, WORKER_LINK_CONTROL.R_RECS)) >>>
                0,
            );
          }
          events.accept(seq);
        } finally {
          inFlight -= 1;
        }
      },
    };
    attachMirror(link, source);
    if (mode === "duplicate") {
      expectThrow("mirror-duplicate", () =>
        attachMirror(link, {
          ...source,
          dispatch() {
            secondDispatches += 1;
          },
        }),
      );
    }
    // Replacement callbacks report a capture failure but preserve the observation
    // path, so the host sees a direct mismatch instead of a fixture timeout.
    const originalDecode = source.decode,
      originalApply = source.apply,
      originalDispatch = source.dispatch;
    source.decode = (payload) => {
      callbacksCaptured = false;
      return originalDecode.call(source, payload);
    };
    source.apply = (fact) => {
      callbacksCaptured = false;
      originalApply.call(source, fact);
    };
    source.dispatch = (fact) => {
      callbacksCaptured = false;
      originalDispatch.call(source, fact);
    };
    report(
      "mirror-reply",
      decoder.decode(
        link.callBlocking(ECHO_CHANNEL, text("mirror-values"), 30_000),
      ),
    );
    report("mirror-applies-at-return", applies);
    report("mirror-dispatches-at-return", dispatches);
    await events.all;
    report(
      "mirror-healthy",
      decoder.decode(
        await link.call(ECHO_CHANNEL, text("mirror-healthy"), 30_000),
      ),
    );
    report("mirror-receiver-exact", receiverExact);
    report("mirror-object-exact", objectExact);
    report("mirror-primitive-exact", primitiveExact);
    report("mirror-bound-valid", boundValid);
    report("mirror-callbacks-captured", callbacksCaptured);
    report("mirror-payloads-exact", payloadsExact);
    report("mirror-observed-values", observed.join(","));
    reportOrder("mirror", events.seqs);
    report("mirror-decodes", decodes);
    report("mirror-applies", applies);
    report("mirror-dispatches", dispatches);
    report("mirror-pending-values", applied.size);
    report("mirror-in-flight", inFlight);
    report("mirror-bound-checked", boundChecks > 0);
    report("mirror-second-dispatches", secondDispatches);
  };
}

// AC4 observes physical Array slot relations, independently of logical counts.
async function mirrorRelease(): Promise<void> {
  const { link } = await open();
  type Fact = { [MIRROR_FACT_MARKER]: object; seq: number };
  // Plain named properties intentionally keep all facts alive, while creating
  // no Array Index edges. No fact-valued observer arrays/maps are introduced.
  const anchors: Record<string, Fact> = {};
  const events = collector(3);
  let decodes = 0,
    applies = 0,
    getter = -1;
  attachMirror(link, {
    channel: LIFECYCLE_CHANNEL,
    decode(payload): Fact {
      const seq = seqOf(payload);
      const fact = { [MIRROR_FACT_MARKER]: {}, seq };
      anchors[String(seq)] = fact;
      decodes += 1;
      return fact;
    },
    apply(fact) {
      getter = fact.seq;
      applies += 1;
    },
    dispatch(fact) {
      events.accept(fact.seq);
    },
  });
  report(
    "mirror-release-reply",
    decoder.decode(
      link.callBlocking(ECHO_CHANNEL, text("mirror-release"), 30_000),
    ),
  );
  report("mirror-release-getter", getter);
  report("mirror-release-listeners-at-return", events.seqs.length);
  const before = mirrorRetention(Bun.generateHeapSnapshot());
  report("mirror-release-before-facts", before.facts);
  report("mirror-release-before-indexed", before.indexedEdges);
  await events.all;
  // Finish actual callbacks and their task; this is no GC/time deadline.
  await nextTask();
  const after = mirrorRetention(Bun.generateHeapSnapshot());
  report("mirror-release-after-facts", after.facts);
  report("mirror-release-after-indexed", after.indexedEdges);
  report(
    "mirror-release-anchors",
    Object.keys(anchors)
      .map((key) => anchors[key].seq)
      .join(","),
  );
  report("mirror-release-decodes", decodes);
  report("mirror-release-applies", applies);
  reportOrder("mirror-release", events.seqs);
  report(
    "mirror-release-healthy",
    decoder.decode(
      await link.call(ECHO_CHANNEL, text("mirror-release-healthy"), 30_000),
    ),
  );
}

function mirrorFailure(
  mode: "decode" | "apply" | "earlier-answer" | "prior-close",
): () => Promise<void> {
  return async () => {
    let resumeDispatch: () => void = () => {
      throw new Error("dispatch was not retained");
    };
    let dispatchReady: () => void = () => undefined;
    const ready = new Promise<void>((resolve) => {
      dispatchReady = resolve;
    });
    let hold = mode === "prior-close";
    const { link, control } = await open(
      {},
      {
        deferDispatch(task) {
          if (hold) {
            resumeDispatch = task;
            dispatchReady();
          } else task();
        },
      },
    );
    let decodes = 0,
      attempts = 0,
      applies = 0;
    const events = collector(1);
    type Fact = { [MIRROR_FACT_MARKER]: object; seq: number };
    // Keep decoded facts alive without adding indexed observer references, so a
    // failed apply retained by the helper cannot hide behind the source Map.
    const anchors: Record<string, Fact> = {};
    const applied = new Map<number, Fact>();
    attachMirror(link, {
      channel: LIFECYCLE_CHANNEL,
      decode(payload) {
        const seq = seqOf(payload);
        decodes += 1;
        if (seq === 1 && mode !== "apply")
          throw kipcError(
            "KELD-IPC-003",
            "second fixture record is undecodable",
          );
        const fact = { [MIRROR_FACT_MARKER]: {}, seq };
        anchors[String(seq)] = fact;
        return fact;
      },
      apply(fact) {
        attempts += 1;
        if (fact.seq === 1)
          throw kipcError("KELD-IPC-003", "second fixture apply fails");
        applies += 1;
        applied.set(fact.seq, fact);
      },
      dispatch(fact) {
        if (applied.get(fact.seq) !== fact)
          throw new Error("prior fact identity changed");
        applied.delete(fact.seq);
        events.accept(fact.seq);
      },
    });
    const ended = new Promise<Error>((resolve) => link.onEnd(resolve));
    const answer = link.call(ECHO_CHANNEL, text("mirror-answer"), 30_000).then(
      (payload) => report("mirror-answer", decoder.decode(payload)),
      (error) => report("mirror-answer-code", codeOf(error)),
    );
    const unanswered = expectReject(
      "mirror-unanswered",
      link.call(ECHO_CHANNEL, text("mirror-unanswered"), 30_000),
    );
    if (mode === "prior-close") {
      const parked = expectReject(
        "mirror-parked",
        link.call(ECHO_CHANNEL, text("mirror-fault"), 30_000),
      );
      await ready;
      awaitWord(control, WORKER_LINK_CONTROL.W_RECS, (count) => count >= 4);
      link.close();
      hold = false;
      resumeDispatch();
      await parked;
    } else {
      expectThrow("mirror-parked", () =>
        link.callBlocking(ECHO_CHANNEL, text("mirror-fault"), 30_000),
      );
      report("mirror-listeners-at-throw", events.seqs.length);
    }
    await answer;
    await unanswered;
    const end = await ended;
    await nextTask();
    const terminal = mirrorRetention(Bun.generateHeapSnapshot());
    report("mirror-terminal-facts", terminal.facts);
    report("mirror-terminal-indexed", terminal.indexedEdges);
    report(
      "mirror-terminal-anchors",
      Object.keys(anchors)
        .map((key) => anchors[key].seq)
        .join(","),
    );
    report("mirror-end-code", codeOf(end));
    report("mirror-end-cause-005", end.message.includes("KELD-IPC-005"));
    report("mirror-decodes", decodes);
    report("mirror-apply-attempts", attempts);
    report("mirror-applies", applies);
    reportOrder("mirror", events.seqs);
    report("mirror-pending-values", applied.size);
    report(
      "mirror-retained-records",
      (word(control, WORKER_LINK_CONTROL.W_RECS) -
        word(control, WORKER_LINK_CONTROL.R_RECS)) >>>
        0,
    );
    expectThrow("mirror-later", () =>
      link.callBlocking(ECHO_CHANNEL, text("mirror-never"), 1_000),
    );
  };
}

// Criterion 5: close without ERR; records before the close still delivered.
async function closeWake(): Promise<void> {
  const { link } = await open();
  const events = collector(3);
  link.onEvent(LIFECYCLE_CHANNEL, events.listener);
  expectThrow("call", () =>
    link.callBlocking(ECHO_CHANNEL, text("close"), 30_000),
  );
  report("listeners-at-throw", events.seqs.length);
  await events.all;
  reportOrder("close", events.seqs);
  expectThrow("later", () =>
    link.callBlocking(ECHO_CHANNEL, text("later"), 1_000),
  );
}

// Criterion 8: Worker death or wedge wakes a parked call with 025.
function workerFault(kind: "throw" | "exit" | "wedge"): () => Promise<void> {
  return async () => {
    const { link, words } = await open(
      {},
      { onBlockingCall: { kind, delayMs: 50 } },
    );
    expectThrow("call", () =>
      link.callBlocking(ECHO_CHANNEL, text(`fault-${kind}`), 30_000),
    );
    report(
      "liveness-branch",
      Atomics.load(words, WORKER_LINK_TEST_WORDS.LIVENESS_BRANCH),
    );
    report(
      "exit-handler",
      Atomics.load(words, WORKER_LINK_TEST_WORDS.EXIT_HANDLER),
    );
    expectThrow("later", () =>
      link.callBlocking(ECHO_CHANNEL, text("later"), 1_000),
    );
  };
}

// Criterion 10: a full ring fails closed with 026 and keeps what it retained.
function overflow(bound: "bytes" | "records"): () => Promise<void> {
  return async () => {
    const retained = bound === "bytes" ? 1024 : 8;
    const { link } = await open(
      bound === "bytes" ? { ringBytes: 1 << 16 } : { ringRecords: 8 },
    );
    const events = collector(retained);
    link.onEvent(LIFECYCLE_CHANNEL, events.listener);
    expectThrow("call", () =>
      link.callBlocking(ECHO_CHANNEL, text("overflow"), 30_000),
    );
    await events.all;
    await nextTask();
    reportOrder("overflow", events.seqs);
  };
}

// Criterion 11: a second open in the realm, and a second connect to the locator.
// `connectAgain` false is the Windows "second-open" case (#528 T5): the host
// itself proves the OS refuses a second client of the busy pipe, because
// Bun's connect to a busy pipe waits libuv's fixed 30 s before it refuses.
async function secondLink(connectAgain: boolean): Promise<void> {
  const { link } = await open();
  try {
    await WorkerLink.open({ link: requireLink(), receive: LIFECYCLE_EVENTS });
    report("second-open", "returned");
  } catch (err) {
    report("second-open", codeOf(err));
  }
  if (connectAgain) {
    const { endpoint } = parseAppLink(requireLink());
    try {
      const socket = await Bun.connect({
        unix: endpoint,
        socket: { data() {} },
      });
      socket.end();
      report("second-connect", "connected");
    } catch (err) {
      const code = (err as { code?: unknown }).code;
      report(
        "second-connect",
        `refused:${typeof code === "string" ? code : "error"}`,
      );
    }
  }
  report(
    "first-link",
    decoder.decode(link.callBlocking(ECHO_CHANNEL, text("still-up"), 30_000)),
  );
}

// Criterion 12 (i, iii): correlation id selects the reply; a late reply is discarded.
async function correlation(): Promise<void> {
  const { link, control } = await open();
  const other = link.call(LIFECYCLE_CHANNEL, text("other"), 30_000);
  report(
    "blocking",
    decoder.decode(link.callBlocking(LIFECYCLE_CHANNEL, text("mine"), 30_000)),
  );
  report("other", decoder.decode(await other));
  expectThrow("expired", () =>
    link.callBlocking(ECHO_CHANNEL, text("expires"), 200),
  );
  const before = word(control, WORKER_LINK_CONTROL.W_RECS);
  report(
    "fresh",
    decoder.decode(link.callBlocking(ECHO_CHANNEL, text("fresh"), 30_000)),
  );
  report("late-appended", word(control, WORKER_LINK_CONTROL.W_RECS) - before);
}

// Criterion 12 (iv): a state applier's blocking call is refused before any write.
async function innerBlockingCall(): Promise<void> {
  const { link, control } = await open();
  link.setStateApplier(LIFECYCLE_CHANNEL, () => {
    const slot = [
      WORKER_LINK_CONTROL.REPLY_READY,
      WORKER_LINK_CONTROL.REPLY_LEN,
      WORKER_LINK_CONTROL.REPLY_AT,
    ];
    const before = slot.map((index) => word(control, index)).join("/");
    report(
      "blocking-word-at-drain",
      word(control, WORKER_LINK_CONTROL.BLOCKING),
    );
    expectThrow("inner", () =>
      link.callBlocking(ECHO_CHANNEL, text("inner"), 1_000),
    );
    report(
      "slot-unchanged",
      slot.map((index) => word(control, index)).join("/") === before,
    );
    throw new Error("test applier failure after the refused inner call");
  });
  expectThrow("outer", () =>
    link.callBlocking(ECHO_CHANNEL, text("outer"), 30_000),
  );
}

// Criterion 13: every call needs a finite deadline; an expiry leaves the link up.
async function deadlines(): Promise<void> {
  const { link } = await open();
  let refused = 0;
  for (const deadline of [
    0,
    -1,
    Number.NaN,
    Number.POSITIVE_INFINITY,
    24 * 60 * 60 * 1000 + 1,
  ]) {
    if (
      expectThrow(`deadline-${String(deadline)}`, () =>
        link.callBlocking(ECHO_CHANNEL, text("never"), deadline),
      ) === "KELD-IPC-005"
    ) {
      refused += 1;
    }
  }
  report("refused", refused);
  expectThrow("silent", () =>
    link.callBlocking(ECHO_CHANNEL, text("silent"), 200),
  );
  report(
    "after",
    decoder.decode(link.callBlocking(ECHO_CHANNEL, text("after"), 30_000)),
  );
}

// Criteria 14 and 21: a frame the selected policy rejects closes the link (022, cause 005).
async function inboundViolation(): Promise<void> {
  const { link, control } = await open();
  let listeners = 0;
  let appliers = 0;
  link.onEvent(LIFECYCLE_CHANNEL, () => {
    listeners += 1;
  });
  link.setStateApplier(LIFECYCLE_CHANNEL, () => {
    appliers += 1;
  });
  expectThrow("call", () =>
    link.callBlocking(ECHO_CHANNEL, text("violation"), 30_000),
  );
  await nextTask();
  await nextTask();
  report("w-recs", word(control, WORKER_LINK_CONTROL.W_RECS));
  report("reply-ready", word(control, WORKER_LINK_CONTROL.REPLY_READY));
  report("listeners", listeners);
  report("appliers", appliers);
}

// Criterion 14: main's liveness failure and the Worker's close race; one code wins.
async function livenessCloseRace(): Promise<void> {
  const { link, control } = await open(
    {},
    {
      onBlockingCall: { kind: "stall-then-close", delayMs: 0, stallMs: 1_000 },
    },
  );
  const code = expectThrow("call", () =>
    link.callBlocking(ECHO_CHANNEL, text("race"), 30_000),
  );
  const recorded = word(control, WORKER_LINK_CONTROL.STATE);
  report("state-at-throw", recorded);
  report("code-matches-state", code === `KELD-IPC-0${recorded}`);
  const later = expectThrow("later", () =>
    link.callBlocking(ECHO_CHANNEL, text("later"), 1_000),
  );
  report("state-later", word(control, WORKER_LINK_CONTROL.STATE));
  report("later-matches", later === code);
}

// Criterion 18: the blocking reply never needs ring space.
async function replySlot(): Promise<void> {
  const { link } = await open({ ringBytes: 1 << 16, replyBytes: 4_096 });
  const events = collector(1024);
  link.onEvent(LIFECYCLE_CHANNEL, events.listener);
  const reply = link.callBlocking(ECHO_CHANNEL, text("full-ring"), 30_000);
  report("reply-len", reply.byteLength);
  report(
    "reply-pattern",
    reply.every((byte, i) => byte === (i & 0xff)),
  );
  await events.all;
  reportOrder("slot", events.seqs);
}

async function replySlotOversize(): Promise<void> {
  const { link } = await open({ replyBytes: 4_096 });
  expectThrow("call", () =>
    link.callBlocking(ECHO_CHANNEL, text("oversize"), 30_000),
  );
}

/**
 * Criteria 18 and 26: a watchdog test thread counts HEARTBEAT advances after
 * main's deadline compare-and-exchange fails. At the limit it kills the role,
 * so a call that never ends fails the test instead of hanging it.
 */
const WATCHDOG_LIMIT = 2 * (1_000 / 100);
function startWatchdog(
  control: Int32Array,
  words: Int32Array,
): Promise<number> {
  const watchdog = new Worker(new URL(import.meta.url), {
    workerData: {
      watchdog: true,
      control: control.buffer,
      words: words.buffer,
      limit: WATCHDOG_LIMIT,
    },
  });
  return new Promise((resolve) => {
    watchdog.on("message", (count: number) => resolve(count));
  });
}

function watchdogMain(data: {
  control: SharedArrayBuffer;
  words: SharedArrayBuffer;
  limit: number;
}): void {
  const control = new Int32Array(data.control);
  const words = new Int32Array(data.words);
  Atomics.wait(words, WORKER_LINK_TEST_WORDS.DEADLINE_CAS_FAILED, 0);
  const start = Atomics.load(control, WORKER_LINK_CONTROL.HEARTBEAT);
  for (;;) {
    const advances =
      (Atomics.load(control, WORKER_LINK_CONTROL.HEARTBEAT) - start) >>> 0;
    if (Atomics.load(words, WORKER_LINK_TEST_WORDS.TEST_0) === 1) {
      parentPort?.postMessage(advances);
      return;
    }
    if (advances >= data.limit) {
      console.log("KELD_WL watchdog-limit=reached");
      process.kill(process.pid, "SIGKILL");
    }
    Atomics.wait(words, WORKER_LINK_TEST_WORDS.TEST_0, 0, 10);
  }
}

function callEnded(words: Int32Array): void {
  Atomics.store(words, WORKER_LINK_TEST_WORDS.TEST_0, 1);
  Atomics.notify(words, WORKER_LINK_TEST_WORDS.TEST_0);
  // A watchdog still waiting for a deadline failure wakes and reports too.
  Atomics.notify(words, WORKER_LINK_TEST_WORDS.DEADLINE_CAS_FAILED);
}

/**
 * Holds main's deadline compare-and-exchange until the Worker has claimed the
 * reply, so the post-claim path under test runs whatever the host round trip
 * costs; no step depends on a 200 ms deadline outrunning that trip. The hold is
 * bounded by the link itself: it waits in heartbeat-sized slices and gives the
 * park back (returns false) once `STATE` is set or the Worker's heartbeat has
 * not moved for the liveness window, so a Worker that dies before its claim
 * still ends in the park's typed liveness code, never a hang.
 */
function claimFirst(
  words: () => Int32Array,
  control: () => Int32Array,
): () => boolean {
  return () => {
    const w = words();
    const c = control();
    let beat = Atomics.load(c, WORKER_LINK_CONTROL.HEARTBEAT);
    let beatAt = performance.now();
    while (Atomics.load(w, WORKER_LINK_TEST_WORDS.CLAIMED) === 0) {
      if (Atomics.load(c, WORKER_LINK_CONTROL.STATE) !== 0) return false;
      const now = performance.now();
      const current = Atomics.load(c, WORKER_LINK_CONTROL.HEARTBEAT);
      if (current !== beat) {
        beat = current;
        beatAt = now;
      } else if (now - beatAt >= WORKER_LIVENESS_WINDOW_MS) {
        return false;
      }
      Atomics.wait(
        w,
        WORKER_LINK_TEST_WORDS.CLAIMED,
        0,
        WORKER_HEARTBEAT_INTERVAL_MS,
      );
    }
    return true;
  };
}

async function claimStall(): Promise<void> {
  let testWords: Int32Array = new Int32Array(new SharedArrayBuffer(4));
  let testControl: Int32Array = new Int32Array(new SharedArrayBuffer(64));
  const { link, control, words } = await open(
    {},
    {
      claimFault: "stall-until-deadline-cas",
      beforeDeadlineCas: claimFirst(
        () => testWords,
        () => testControl,
      ),
    },
  );
  testWords = words;
  testControl = control;
  const watchdog = startWatchdog(control, words);
  expectThrow("call", () =>
    link.callBlocking(ECHO_CHANNEL, text("stall"), 200),
  );
  callEnded(words);
  report(
    "deadline-cas-failed",
    Atomics.load(words, WORKER_LINK_TEST_WORDS.DEADLINE_CAS_FAILED),
  );
  report("watchdog-advances-below-limit", (await watchdog) < WATCHDOG_LIMIT);
}

// Criterion 26: no unbounded parked wait after a claim.
async function claimSkipPublish(): Promise<void> {
  let testWords: Int32Array = new Int32Array(new SharedArrayBuffer(4));
  let testControl: Int32Array = new Int32Array(new SharedArrayBuffer(64));
  const { link, control, words } = await open(
    {},
    {
      claimFault: "skip-publish",
      beforeDeadlineCas: claimFirst(
        () => testWords,
        () => testControl,
      ),
    },
  );
  testWords = words;
  testControl = control;
  const watchdog = startWatchdog(control, words);
  expectThrow("call", () => link.callBlocking(ECHO_CHANNEL, text("skip"), 300));
  callEnded(words);
  report("state", word(control, WORKER_LINK_CONTROL.STATE));
  report(
    "post-claim-branch",
    Atomics.load(words, WORKER_LINK_TEST_WORDS.POST_CLAIM_BRANCH),
  );
  report("watchdog-advances-below-limit", (await watchdog) < WATCHDOG_LIMIT);
}

// The claim-first hold is bounded: a Worker that dies (process.exit, no exit
// handler) before it claims leaves the park to record 025 through its liveness
// branch; the host never answers, so no claim can happen.
async function claimFirstWorkerDies(): Promise<void> {
  let testWords: Int32Array = new Int32Array(new SharedArrayBuffer(4));
  let testControl: Int32Array = new Int32Array(new SharedArrayBuffer(64));
  const { link, control, words } = await open(
    {},
    {
      onBlockingCall: { kind: "exit", delayMs: 50 },
      beforeDeadlineCas: claimFirst(
        () => testWords,
        () => testControl,
      ),
    },
  );
  testWords = words;
  testControl = control;
  expectThrow("call", () =>
    link.callBlocking(ECHO_CHANNEL, text("dies-before-claim"), 200),
  );
  report("claimed", Atomics.load(words, WORKER_LINK_TEST_WORDS.CLAIMED));
  report(
    "liveness-branch",
    Atomics.load(words, WORKER_LINK_TEST_WORDS.LIVENESS_BRANCH),
  );
}

async function claimThrow(): Promise<void> {
  const { link, control, words } = await open({}, { claimFault: "throw" });
  expectThrow("call", () =>
    link.callBlocking(ECHO_CHANNEL, text("claim-throw"), 30_000),
  );
  report("state", word(control, WORKER_LINK_CONTROL.STATE));
  report(
    "post-claim-branch",
    Atomics.load(words, WORKER_LINK_TEST_WORDS.POST_CLAIM_BRANCH),
  );
  report(
    "deadline-cas-failed",
    Atomics.load(words, WORKER_LINK_TEST_WORDS.DEADLINE_CAS_FAILED),
  );
  report(
    "liveness-branch",
    Atomics.load(words, WORKER_LINK_TEST_WORDS.LIVENESS_BRANCH),
  );
}

// Criterion 19: no record is stranded in the ring; a throwing listener is isolated.
function stranded(throwing: boolean): () => Promise<void> {
  return async () => {
    let idleCalls = 0;
    let idleViolations = 0;
    const { link, control } = await open(
      {},
      {
        onDispatchIdle: (rRecs, wRecs) => {
          idleCalls += 1;
          if (rRecs !== wRecs) idleViolations += 1;
        },
      },
    );
    let uncaught = 0;
    process.on("uncaughtException", () => {
      uncaught += 1;
    });
    const events = collector(2);
    let first = true;
    attachMirror(link, {
      channel: LIFECYCLE_CHANNEL,
      decode: seqOf,
      apply() {},
      dispatch(seq) {
        events.accept(seq);
        if (!first) return;
        first = false;
        // Ask for the second EVENT while this record remains in flight.
        link.sendEvent(LIFECYCLE_CHANNEL, text("send-second"));
        awaitWord(control, WORKER_LINK_CONTROL.W_RECS, (value) => value >= 2);
        if (throwing) throw new Error("test listener failure");
      },
    });
    await events.all;
    await nextTask();
    await nextTask();
    await nextTask();
    reportOrder("stranded", events.seqs);
    report("idle-calls-positive", idleCalls > 0);
    report("idle-violations", idleViolations);
    report("uncaught", uncaught);
    report(
      "link-up",
      decoder.decode(link.callBlocking(ECHO_CHANNEL, text("still-up"), 30_000)),
    );
  };
}

// Criterion 20: outbound frames reach the host in send order.
async function outboundOrder(): Promise<void> {
  const { link, control } = await open({}, { holdMessagesUntilBlocking: true });
  const wBytes = word(control, WORKER_LINK_CONTROL.W_BYTES);
  const wRecs = word(control, WORKER_LINK_CONTROL.W_RECS);
  link.sendEvent(LIFECYCLE_CHANNEL, text("e1"));
  const c1 = link.call(LIFECYCLE_CHANNEL, text("c1"), 30_000);
  report(
    "c2",
    decoder.decode(link.callBlocking(LIFECYCLE_CHANNEL, text("c2"), 30_000)),
  );
  report("w-bytes-delta", word(control, WORKER_LINK_CONTROL.W_BYTES) - wBytes);
  report("w-recs-delta", word(control, WORKER_LINK_CONTROL.W_RECS) - wRecs);
  link.sendEvent(LIFECYCLE_CHANNEL, text("reply-c1"));
  report("c1", decoder.decode(await c1));
}

// Criterion 21: admitted inbound frames, and a PING that never enters the ring.
async function inboundAdmitted(): Promise<void> {
  const { link, control } = await open({ receive: WITH_ECHO_CALLS });
  const events = collector(1);
  link.onEvent(LIFECYCLE_CHANNEL, events.listener);
  let handled = "";
  const answered = new Promise<void>((resolve) => {
    link.setCallHandler(ECHO_CHANNEL, async (payload) => {
      handled = decoder.decode(payload);
      resolve();
      return { kind: FrameKind.Reply, payload: text(`answer:${handled}`) };
    });
  });
  const pending = link.call(LIFECYCLE_CHANNEL, text("pending"), 30_000);
  report("pending", decoder.decode(await pending));
  await events.all;
  await answered;
  await nextTask(); // the handler's answer is posted before the next CALL
  report("event-seq", events.seqs[0]);
  report("handled", handled);
  report(
    "finish",
    decoder.decode(
      link.callBlocking(LIFECYCLE_CHANNEL, text("finish"), 30_000),
    ),
  );
  report("w-recs", word(control, WORKER_LINK_CONTROL.W_RECS));
}

// Criterion 22: the abandoned set is bounded.
async function abandonedCap(): Promise<void> {
  const { link, control } = await open();
  const first = await Promise.all(
    Array.from({ length: 256 }, (_, i) =>
      link.call(LIFECYCLE_CHANNEL, text(`abandon-${i}`), 100).then(
        () => "returned",
        (err: unknown) => codeOf(err),
      ),
    ),
  );
  report("first-006", first.filter((code) => code === "KELD-IPC-006").length);
  const before = word(control, WORKER_LINK_CONTROL.W_RECS);
  link.sendEvent(LIFECYCLE_CHANNEL, text("send-late-replies"));
  report(
    "probe",
    decoder.decode(link.callBlocking(ECHO_CHANNEL, text("probe"), 30_000)),
  );
  report("late-appended", word(control, WORKER_LINK_CONTROL.W_RECS) - before);
  const refill = await Promise.all(
    Array.from({ length: 4 }, (_, i) =>
      link.call(LIFECYCLE_CHANNEL, text(`refill-${i}`), 100).then(
        () => "returned",
        (err: unknown) => codeOf(err),
      ),
    ),
  );
  report("refill-006", refill.filter((code) => code === "KELD-IPC-006").length);
  const long = link.call(LIFECYCLE_CHANNEL, text("long"), 30_000);
  report(
    "crossing",
    await expectReject(
      "crossing",
      link.call(LIFECYCLE_CHANNEL, text("crossing"), 100),
    ),
  );
  await expectReject("long", long);
  expectThrow("later", () =>
    link.callBlocking(ECHO_CHANNEL, text("later"), 30_000),
  );
}

// Criterion 25: a 4 MiB ring opens; byte counters wrap at 2^32 without loss.
async function ring4MiB(): Promise<void> {
  const { link } = await open({ ringBytes: 4 * 1024 * 1024 });
  report(
    "reply",
    decoder.decode(link.callBlocking(ECHO_CHANNEL, text("four-mib"), 30_000)),
  );
}

async function counterWrap(): Promise<void> {
  const start = 2 ** 32 - 64;
  let control: Int32Array = new Int32Array(new SharedArrayBuffer(4));
  const opened = await open(
    { ringBytes: 1 << 16 },
    {
      counterStart: start,
      // Hold the wake drain until the facts written after the reply are in
      // the ring, so a drain that overran REPLY_AT would apply them.
      beforeWakeDrain: () =>
        awaitWord(control, WORKER_LINK_CONTROL.W_RECS, (value) => value >= 8),
    },
  );
  control = opened.control;
  const { link } = opened;
  const applied: number[] = [];
  let intact = true;
  link.setStateApplier(LIFECYCLE_CHANNEL, (payload) => {
    applied.push(seqOf(payload));
  });
  const events = collector(8);
  link.onEvent(LIFECYCLE_CHANNEL, (payload) => {
    const seq = seqOf(payload);
    for (let i = 4; i < payload.length; i += 1)
      if (payload[i] !== ((seq + i) & 0xff)) intact = false;
    events.listener(payload);
  });
  link.callBlocking(ECHO_CHANNEL, text("wrap"), 30_000);
  report("applied-at-return", applied.join(","));
  const replyAt = word(control, WORKER_LINK_CONTROL.REPLY_AT);
  report("reply-at-wrapped", replyAt < start);
  await events.all;
  reportOrder("wrap", events.seqs);
  report("payloads-intact", intact);
  report("applied-final", applied.join(","));
}

// Criterion 27: asynchronous replies and host CALLs are dispatched on main.
async function asyncDispatch(): Promise<void> {
  const { link } = await open();
  const log: string[] = [];
  const events = collector(1);
  link.onEvent(LIFECYCLE_CHANNEL, (payload) => {
    log.push("event");
    events.listener(payload);
  });
  const c1 = link.call(LIFECYCLE_CHANNEL, text("c1"), 30_000).then(
    (bytes) => {
      log.push(`c1:${decoder.decode(bytes)}`);
    },
    (err: unknown) => {
      log.push(`c1:${codeOf(err)}`);
    },
  );
  const c2 = link.call(ECHO_CHANNEL, text("c2"), 30_000).then(
    (bytes) => {
      log.push(`c2:${decoder.decode(bytes)}`);
    },
    (err: unknown) => {
      log.push(`c2:${codeOf(err)}`);
    },
  );
  await Promise.all([c1, c2, events.all]);
  report("log", log.join(","));
  await expectReject("late", link.call(LIFECYCLE_CHANNEL, text("late"), 100));
  link.sendEvent(LIFECYCLE_CHANNEL, text("send-late-reply"));
  report(
    "probe",
    decoder.decode(link.callBlocking(ECHO_CHANNEL, text("probe"), 30_000)),
  );
}

async function hostCallParked(): Promise<void> {
  const { link } = await open({ receive: WITH_ECHO_CALLS });
  let runs = 0;
  const answered = new Promise<void>((resolve) => {
    link.setCallHandler(ECHO_CHANNEL, async (payload) => {
      runs += 1;
      resolve();
      return {
        kind: FrameKind.Reply,
        payload: text(`answer:${decoder.decode(payload)}`),
      };
    });
  });
  link.callBlocking(LIFECYCLE_CHANNEL, text("parked"), 30_000);
  report("handler-runs-at-return", runs);
  await answered;
  await nextTask(); // the handler's answer is posted before the next CALL
  report("handler-runs-after", runs);
  report(
    "finish",
    decoder.decode(
      link.callBlocking(LIFECYCLE_CHANNEL, text("finish"), 30_000),
    ),
  );
}

async function hostCallNoHandler(): Promise<void> {
  const { link } = await open({ receive: WITH_ECHO_CALLS });
  await expectReject(
    "pending",
    link.call(LIFECYCLE_CHANNEL, text("pending"), 30_000),
  );
  expectThrow("later", () =>
    link.callBlocking(LIFECYCLE_CHANNEL, text("later"), 1_000),
  );
}

// Review finding: a reply published between main's REPLY_READY and STATE loads
// (host writes the REPLY, then closes) is returned, never reported as 022.
async function replyThenClose(): Promise<void> {
  let control: Int32Array = new Int32Array(new SharedArrayBuffer(4));
  let first = true;
  const opened = await open(
    {},
    {
      beforeStateCheck: () => {
        if (!first) return;
        first = false;
        // Hold main between the two loads until the Worker has published the
        // reply and then recorded the close.
        awaitWord(control, WORKER_LINK_CONTROL.STATE, (value) => value !== 0);
      },
    },
  );
  control = opened.control;
  expectThrow("call", () =>
    opened.link.callBlocking(ECHO_CHANNEL, text("reply-then-close"), 30_000),
  );
  report("reply-ready-after", word(control, WORKER_LINK_CONTROL.REPLY_READY));
}

// Review finding: an applier may not start a blocking call during dispatch either.
async function applierBlockingInDispatch(): Promise<void> {
  const { link } = await open();
  let inner = "not-called";
  link.setStateApplier(LIFECYCLE_CHANNEL, () => {
    inner = expectThrow("inner", () =>
      link.callBlocking(ECHO_CHANNEL, text("inner"), 1_000),
    );
  });
  const events = collector(1);
  link.onEvent(LIFECYCLE_CHANNEL, events.listener);
  await events.all;
  report("inner-result", inner);
  report(
    "probe",
    decoder.decode(link.callBlocking(ECHO_CHANNEL, text("probe"), 30_000)),
  );
}

// Review finding: a malformed ERR payload closes the link (022, cause 005).
async function malformedErr(): Promise<void> {
  const { link } = await open();
  expectThrow("call", () =>
    link.callBlocking(LIFECYCLE_CHANNEL, text("err"), 30_000),
  );
  expectThrow("later", () =>
    link.callBlocking(LIFECYCLE_CHANNEL, text("later"), 1_000),
  );
}

// WorkerLink.close(): pending calls reject with 022 once retained records are
// delivered; every later call throws 022; the Worker ends the socket.
async function localClose(): Promise<void> {
  const { link, control } = await open();
  const pending = link.call(LIFECYCLE_CHANNEL, text("pending"), 30_000);
  link.close();
  await expectReject("pending", pending);
  report("state", word(control, WORKER_LINK_CONTROL.STATE));
  expectThrow("later", () =>
    link.callBlocking(ECHO_CHANNEL, text("later"), 1_000),
  );
  try {
    link.sendEvent(LIFECYCLE_CHANNEL, text("after-close"));
    report("send-after-close", "sent");
  } catch (err) {
    report("send-after-close", codeOf(err));
  }
  // Stay alive until the host has observed link loss, so the Worker's flush of
  // the CALL posted before close() is what the host sees, not this exit.
  await new Promise<void>((resolve) =>
    process.stdin.once("data", () => resolve()),
  );
}

// §4.6 expiry rule: a reply retained in the ring before a call()'s deadline
// decision is that call's answer. The hook withholds every dispatch task behind
// a fresh timer, so after the park the overdue deadline timer runs first.
async function expiryDuringPark(): Promise<void> {
  const { link } = await open(
    {},
    { deferDispatch: (run) => setTimeout(run, 0) },
  );
  const started = performance.now();
  const early = link.call(LIFECYCLE_CHANNEL, text("early"), 200);
  link.callBlocking(ECHO_CHANNEL, text("long-park"), 30_000);
  report("park-outlasted-deadline", performance.now() - started >= 200);
  await expectReject("early", early);
}

// §4.6: an async call() still unanswered when the link ends rejects with the
// code STATE records, even when its overdue deadline timer runs before the
// dispatch task (the hook withholds the dispatch).
async function expiryAfterClose(): Promise<void> {
  const { link } = await open(
    {},
    { deferDispatch: (run) => setTimeout(run, 0) },
  );
  const early = link.call(LIFECYCLE_CHANNEL, text("early"), 200);
  expectThrow("park", () =>
    link.callBlocking(ECHO_CHANNEL, text("long-park"), 30_000),
  );
  await expectReject("early", early);
}

// GH-528 T2 end-to-end against the keld-core router. The host test passes the
// call's channel and payload (the router's own FS constant and FsRequest
// codec), so this fixture names no channel number of its own.
function t2Call(): { channel: number; payload: Uint8Array } {
  const channel = Number(process.env.KELD_T2_CHANNEL);
  const hexText = process.env.KELD_T2_PAYLOAD_HEX ?? "";
  const payload = Uint8Array.from(hexText.match(/../g) ?? [], (pair) =>
    Number.parseInt(pair, 16),
  );
  return { channel, payload };
}

function hexOf(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join(
    "",
  );
}

// Criteria 6 and 7(a): one blocking host CALL, then an echo CALL that the host
// never answers after a retirement or an accepted Quit; the host's close ends it.
async function t2BlockingCall(): Promise<void> {
  const { link } = await open();
  const { channel, payload } = t2Call();
  try {
    report("call-hex", hexOf(link.callBlocking(channel, payload, 30_000)));
    report("call-returned", true);
  } catch (err) {
    report("call-code", codeOf(err));
    report("call-returned", false);
  }
  await expectReject("after", link.call(ECHO_CHANNEL, text("after"), 10_000));
}

// Criterion 8 (host half): the transport Worker exits while main is parked.
async function t2WorkerDies(): Promise<void> {
  const { link } = await open(
    {},
    { onBlockingCall: { kind: "exit", delayMs: 50 } },
  );
  const { channel, payload } = t2Call();
  expectThrow("call", () => link.callBlocking(channel, payload, 30_000));
}

// GH-528 T3: `onEnd` reports the link's end once, after every retained
// record reached its listener; a listener added later still runs, a removed
// one never does, and a throwing one does not stop the others.
async function onEndReport(): Promise<void> {
  const { link } = await open();
  const log: string[] = [];
  const events = collector(2);
  link.onEvent(LIFECYCLE_CHANNEL, (payload) => {
    events.listener(payload);
    log.push(`event:${seqOf(payload)}`);
  });
  let resolveFirst: () => void = () => undefined;
  const first = new Promise<void>((resolve) => {
    resolveFirst = resolve;
  });
  link.onEnd(() => {
    throw new Error("test-only throwing end listener");
  });
  link.onEnd((err) => {
    log.push(`end:${codeOf(err)}`);
    resolveFirst();
  });
  const removed = link.onEnd(() => log.push("removed-ran"));
  removed();
  let uncaught = 0;
  process.on("uncaughtException", () => {
    uncaught += 1;
  });
  link.sendEvent(LIFECYCLE_CHANNEL, text("ready-for-events"));
  await first;
  await new Promise<void>((resolve) => {
    link.onEnd((err) => {
      log.push(`late:${codeOf(err)}`);
      resolve();
    });
  });
  await nextTask();
  report("order", log.join(","));
  report("uncaught", uncaught);
}

// GH-528 T3 (#636 gate review): the role's Quit is its last call, and it never
// parks (PANEL-P2). The link closes the moment the REPLY settles, so a call
// still pending then rejects with KELD-IPC-022 (which callers treat as the
// host drain's 024) and the host reads EOF at once.
async function quitClose(): Promise<void> {
  const { link } = await open();
  const log: string[] = [];
  const pending = link.call(LIFECYCLE_CHANNEL, text("pending"), 30_000).then(
    () => log.push("pending:returned"),
    (err) => log.push(`pending:${codeOf(err)}`),
  );
  const ended = new Promise<void>((resolve) => {
    link.onEnd((err) => {
      log.push(`end:${codeOf(err)}`);
      resolve();
    });
  });
  await quitAndCloseLink(link, 30_000);
  report("quit", "returned");
  log.push("quit:returned");
  await pending;
  await ended;
  report("order", log.join(","));
}

// GH-528 T3 end to end against the keld-core router: the role's Quit is its
// last call and closes the link on its REPLY (quitAndCloseLink).
async function t3QuitClose(): Promise<void> {
  const { link } = await open();
  const ended = new Promise<string>((resolve) => {
    link.onEnd((err) => resolve(codeOf(err)));
  });
  await quitAndCloseLink(link, 30_000);
  report("quit", "returned");
  report("end-code", await ended);
}

// Fable review of #643: a wedged Worker that holds KICK cannot keep a closed
// link's promises unsettled or the role alive. close() takes the dispatcher
// itself, so the pending call and onEnd settle within a few tasks, and the
// Worker is terminated APP_LINK_IO_DEADLINE_MS after the link finalized.
async function wedgedKick(): Promise<void> {
  let resolveExit: (code: number) => void = () => undefined;
  const exited = new Promise<number>((resolve) => {
    resolveExit = resolve;
  });
  const { link, words } = await open(
    {},
    { wedgeHoldingKick: true, onWorkerExit: (code) => resolveExit(code) },
  );
  const log: string[] = [];
  void link.call(LIFECYCLE_CHANNEL, text("pending"), 30_000).then(
    () => log.push("pending:returned"),
    (err) => log.push(`pending:${codeOf(err)}`),
  );
  link.onEnd((err) => log.push(`end:${codeOf(err)}`));
  // The host's EVENT makes the Worker append, take KICK and wedge.
  awaitWord(words, WORKER_LINK_TEST_WORDS.KICK_HELD, (value) => value === 1);
  link.close();
  for (let turn = 0; turn < 4; turn += 1) await nextTask();
  report("settled-after-close", log.join(","));
  await exited;
  report("worker-ended", true);
}

const SCENARIOS: Record<string, () => Promise<void>> = {
  "t2-blocking-call": t2BlockingCall,
  "t2-worker-dies": t2WorkerDies,
  "on-end": onEndReport,
  "quit-close": quitClose,
  "t3-quit-close": t3QuitClose,
  "wedged-kick": wedgedKick,
  "expiry-after-close": expiryAfterClose,
  "expiry-during-park": expiryDuringPark,
  "claim-first-worker-dies": claimFirstWorkerDies,
  "local-close": localClose,
  "reply-then-close": replyThenClose,
  "applier-blocking-in-dispatch": applierBlockingInDispatch,
  "malformed-err": malformedErr,
  "arm-b": armB,
  "wake-rule": wakeRule,
  "mirror-release": mirrorRelease,
  "mirror-identity": mirrorValues("identity"),
  "mirror-primitives": mirrorValues("primitives"),
  "mirror-reentrant": mirrorValues("reentrant"),
  "mirror-duplicate": mirrorValues("duplicate"),
  "mirror-decode-failure": mirrorFailure("decode"),
  "mirror-apply-failure": mirrorFailure("apply"),
  "mirror-earlier-answer": mirrorFailure("earlier-answer"),
  "mirror-prior-close": mirrorFailure("prior-close"),
  "close-wake": closeWake,
  "worker-throw": workerFault("throw"),
  "worker-exit": workerFault("exit"),
  "worker-wedge": workerFault("wedge"),
  "overflow-bytes": overflow("bytes"),
  "overflow-records": overflow("records"),
  "second-link": () => secondLink(true),
  "second-open": () => secondLink(false),
  correlation,
  "inner-blocking-call": innerBlockingCall,
  deadlines,
  "inbound-violation": inboundViolation,
  "liveness-close-race": livenessCloseRace,
  "reply-slot": replySlot,
  "reply-slot-oversize": replySlotOversize,
  "claim-stall": claimStall,
  "claim-skip-publish": claimSkipPublish,
  "claim-throw": claimThrow,
  stranded: stranded(false),
  "stranded-throwing": stranded(true),
  "outbound-order": outboundOrder,
  "inbound-admitted": inboundAdmitted,
  "abandoned-cap": abandonedCap,
  "ring-4mib": ring4MiB,
  "counter-wrap": counterWrap,
  "async-dispatch": asyncDispatch,
  "host-call-parked": hostCallParked,
  "host-call-no-handler": hostCallNoHandler,
};

if (!isMainThread) {
  const data = workerData as {
    watchdog?: boolean;
    control: SharedArrayBuffer;
    words: SharedArrayBuffer;
    limit: number;
  };
  if (data?.watchdog === true) watchdogMain(data);
} else {
  const name = process.argv[2] ?? "";
  const scenario = SCENARIOS[name];
  if (scenario === undefined) {
    console.error(`unknown worker-link scenario: ${name}`);
    process.exit(2);
  }
  try {
    await scenario();
    report("done", true);
    process.exit(0);
  } catch (err) {
    console.error(
      err instanceof Error ? (err.stack ?? err.message) : String(err),
    );
    process.exit(1);
  }
}
