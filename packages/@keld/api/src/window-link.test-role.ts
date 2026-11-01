/** One disposable realm per real socket consumer scenario. */
import { readSync, writeSync } from "node:fs";
import { LifecycleLink } from "./link.ts";
import { WindowSource } from "./window-source.ts";
import { ECHO_CHANNEL, LIFECYCLE_CHANNEL, WorkerLink, isCallError, linkError } from "../../kipc/src/transport.ts";

function report(key: string, value: unknown): void { writeSync(1, `${key}=${JSON.stringify(value)}\n`); }
function requireFact(condition: unknown, detail: string): asserts condition { if (!condition) throw new Error(detail); }
async function rejected(promise: Promise<unknown>): Promise<Error> {
  try { await promise; } catch (error) { if (error instanceof Error) return error; throw error; }
  throw new Error("expected rejection");
}
function rejectedSync(action: () => unknown): Error {
  try { action(); } catch (error) { if (error instanceof Error) return error; throw error; }
  throw new Error("expected synchronous rejection");
}
const scenario = process.argv[2]!;
const events: string[] = [];
let rawReady = false;
let ready!: () => void;
const readyPromise = new Promise<void>((resolve) => { ready = resolve; });
let ended!: () => void;
const endedPromise = new Promise<void>((resolve) => { ended = resolve; });
let dead!: (error: Error) => void;
const deadPromise = new Promise<Error>((resolve) => { dead = resolve; });
let unexpectedDead = 0;
let transportEnd!: (error: Error) => void;
const transportEnded = new Promise<Error>((resolve) => { transportEnd = resolve; });
if (scenario.startsWith("release-")) {
  // Test-only observation: exact original open, same real WorkerLink return,
  // and one ordinary onEnd listener independent of the candidate registration.
  const actualOpen = WorkerLink.open;
  WorkerLink.open = async (options) => {
    const actualLink = await actualOpen.call(WorkerLink, options);
    actualLink.onEnd((error) => { events.push("transport-end"); transportEnd(error); });
    return actualLink;
  };
}
let observedRawError!: (value: WeakRef<Error>) => void;
const rawErrorObserved = new Promise<WeakRef<Error>>((resolve) => { observedRawError = resolve; });
let rawDiagnostic: { code: string; name: string; message: string } | undefined;
if (scenario.startsWith("subscription-")) {
  const actualOpen = WorkerLink.open;
  WorkerLink.open = async (options) => {
    const actualLink = await actualOpen.call(WorkerLink, options);
    const actualCall = actualLink.call;
    actualLink.call = function(channel, payload, deadline) {
      const raw = actualCall.call(this, channel, payload, deadline);
      if (channel === LIFECYCLE_CHANNEL && payload[0] === 1) {
        // Observe the actual transport-produced rejection, return its exact Promise.
        // This observer's derived Promise fulfills with undefined and stores no Error.
        void raw.then(() => {}, (error: unknown) => {
          requireFact(isCallError(error), "actual RPC did not produce a typed Error");
          rawDiagnostic = { code: error.code, name: error.name, message: error.message };
          if (scenario === "subscription-original-release") {
            requireFact(error.code === "KELD-CORE-042", "actual RPC did not produce the host ERR");
            report("raw-rpc-error", { code: error.code, message: error.message });
            observedRawError(new WeakRef(error));
          }
        });
      }
      return raw;
    };
    return actualLink;
  };
}
class ObservedSource extends WindowSource {
  override end(error: Parameters<WindowSource["end"]>[0]): void { super.end(error); events.push("end"); ended(); }
}
const source = new ObservedSource((fact) => events.push(fact.variant));
const link = await LifecycleLink.connect(process.env.KELD_APP_LINK!, {
  onReady() { rawReady = true; ready(); report("raw-ready", true); },
  onLastWindowClosed() { events.push("last-window-closed"); },
  async onApplicationCall(channel, payload) { requireFact(channel === ECHO_CHANNEL, "unexpected host call channel"); return payload; },
  onLinkDead(error) { unexpectedDead += 1; dead(error); },
}, scenario === "default" ? undefined : source);
report("connected", true);

if (scenario.startsWith("ignored-")) {
  requireFact(process.listenerCount("unhandledRejection") === 0, "child installed an unhandledRejection listener");
  const expectedMessage = "KELD-CORE-042: native path unavailable; select supported host";
  function requireHostError(error: Error): void {
    requireFact(isCallError(error) && error.code === "KELD-CORE-042" && error.message === expectedMessage, "ignored read changed exact awaited host rejection");
    report("awaited-rejection", { code: error.code, message: error.message });
  }
  if (scenario === "ignored-before-err") {
    void link.windowSubscription;
    report("ignored-read", "before-ERR");
    requireHostError(await rejected(link.windowSubscription!));
  } else if (scenario === "ignored-after-err") {
    requireHostError(await rejected(link.windowSubscription!));
    // Host observes this roundtrip after producing ERR and before the ignored read.
    requireFact((await link.callFs(Uint8Array.of(0)))[0] === 1, "after-ERR ordered follow-up failed");
    void link.windowSubscription;
    report("ignored-read", "after-ERR");
    requireHostError(await rejected(link.windowSubscription!));
  } else if (scenario === "ignored-presence-guard") {
    // A natural optional-property guard is itself a getter read whose Promise is ignored.
    if (link.windowSubscription) {
      report("ignored-read", "presence-guard-before-ERR");
      requireHostError(await rejected(link.windowSubscription!));
    } else throw new Error("window completion unexpectedly absent");
  } else throw new Error(`unknown ignored-read scenario ${scenario}`);
  requireFact((await link.callFs(Uint8Array.of(0)))[0] === 1 && source.all().length === 0 && !events.includes("end"), "ignored read lost healthy Link/Source follow-up");
  report("healthy-followup", true);
  await new Promise<void>((resolve) => setImmediate(resolve));
  report("survived-next-turn", true);
  requireFact(process.listenerCount("unhandledRejection") === 0, "child added an unhandledRejection listener");
  link.close();
  await endedPromise;
} else if (scenario === "subscription-original-release") {
  try {
    const runtime = { version: Bun.version, revision: Bun.revision, platform: process.platform, arch: process.arch };
    requireFact(runtime.version === "1.4.2" && runtime.revision === "744846f844374847c902b5e7fd59b4342a51ef99" && runtime.platform === "linux" && runtime.arch === "x64", "unsupported original-error diagnostic runtime");
    const rawRef = await rawErrorObserved;
    const anchors = { link, source, externalError: linkError("KELD-IPC-022", "deliberately retained external Error control") };
    const linkRef = new WeakRef(link);
    const sourceRef = new WeakRef(source);
    const externalRef = new WeakRef(anchors.externalError);
    requireFact((await link.callFs(Uint8Array.of(0)))[0] === 1, "healthy unrelated roundtrip failed before collection");
    const observation = await new Promise<{ originalErrorAlive: boolean; linkAlive: boolean; sourceAlive: boolean; externalErrorAlive: boolean }>((resolve, reject) => {
      setImmediate(() => {
        try {
          // One later completed job and one pinned full GC, with independent roots.
          Bun.gc(true);
          resolve({ originalErrorAlive: rawRef.deref() !== undefined,
            linkAlive: linkRef.deref() === anchors.link,
            sourceAlive: sourceRef.deref() === anchors.source,
            externalErrorAlive: externalRef.deref() === anchors.externalError });
        } catch (error) { reject(error); }
      });
    });
    report("original-error-release", { runtime, ...observation });
    requireFact(observation.linkAlive && observation.sourceAlive && observation.externalErrorAlive, "original-error strong controls failed");
    requireFact(!observation.originalErrorAlive, "Link retained original actual RPC Error after one later-job GC");
    const firstPromise = link.windowSubscription!;
    const first = await rejected(firstPromise);
    const secondPromise = link.windowSubscription!;
    const second = await rejected(secondPromise);
    const expected = "KELD-CORE-042: native path unavailable; select supported host";
    requireFact(isCallError(first) && isCallError(second) && first.code === "KELD-CORE-042" && second.code === first.code && first.message === expected && second.message === expected, "late caller diagnostics lost exact code/full message");
    requireFact(firstPromise !== secondPromise && first !== second, "completion getter retained caller Promise/Error identity");
    const own = Object.getOwnPropertyDescriptor(link, "windowSubscription");
    const inherited = Object.getOwnPropertyDescriptor(LifecycleLink.prototype, "windowSubscription");
    report("completion-observation", { samePromise: firstPromise === secondPromise, sameError: first === second,
      ownProperty: own !== undefined, prototypeGetter: typeof inherited?.get === "function", enumerable: inherited?.enumerable,
      configurable: inherited?.configurable, firstMessage: first.message, secondMessage: second.message });
    requireFact(own === undefined && typeof inherited?.get === "function" && inherited.enumerable === false && inherited.configurable === true, "unexpected completion descriptor");
    requireFact((await link.callFs(Uint8Array.of(0)))[0] === 1 && source.all().length === 0 && !events.includes("end"), "healthy link/source failed after collection");
  } catch (error) { report("diagnostic-failure", error instanceof Error ? error.message : String(error)); throw error; } finally { link.close(); await endedPromise; }
} else if (scenario === "release-close" || scenario === "release-quit") {
  report("gc-runtime", { version: Bun.version, revision: Bun.revision, platform: process.platform, arch: process.arch });
  requireFact(Bun.version === "1.4.2" && Bun.revision === "744846f844374847c902b5e7fd59b4342a51ef99" && process.platform === "linux" && process.arch === "x64", "unsupported P3 diagnostic runtime; requalify before using this witness");
  requireFact((await link.windowSubscription!).accepted, "subscription refused");
  const anchors = { source, externalSnapshot: source.get({ id: 3, incarnation: 301n })! };
  requireFact(anchors.externalSnapshot.window.id === 3, "missing distinct external snapshot control");
  report("park-ready", true);
  const input = new Uint8Array(1);
  requireFact(readSync(0, input, 0, 1, null) === 1, "missing explicit park release");
  const answer = link.callWindowBlocking(Uint8Array.of(1));
  requireFact(answer.length === 1 && answer[0] === 1, "earlier parked answer lost");
  requireFact(source.get({ id: 7, incarnation: 300n })?.state.focused === false, "earlier fact not applied before answer");
  requireFact(!events.includes("Focus"), "listener ran before parked caller resumed");
  function trackCurrent() {
    const snapshot = source.get({ id: 7, incarnation: 300n })!;
    return [new WeakRef(snapshot), new WeakRef(snapshot.window), new WeakRef(snapshot.state), new WeakRef(snapshot.state.bounds)];
  }
  const refs = trackCurrent();
  requireFact(refs.filter((ref) => ref.deref() !== undefined).length === 4, "pre-end WeakRef control failed");
  if (scenario === "release-close") link.close();
  else await link.quit();
  const terminal = await transportEnded;
  requireFact(isCallError(terminal) && terminal.code === "KELD-IPC-022", "independent transport end lost its recorded cause");
  const observation = await new Promise<{ live: number; sourceAlive: boolean; externalAlive: boolean; sequence: string[] }>((resolve, reject) => {
    setImmediate(() => {
      try {
        // One explicit later task, one pinned synchronous diagnostic collection.
        // No sleep, timing expectation, GC loop, retry or full heap output.
        Bun.gc(true);
        resolve({ live: refs.filter((ref) => ref.deref() !== undefined).length,
          sourceAlive: anchors.source === source,
          externalAlive: anchors.externalSnapshot.window.id === 3 && anchors.externalSnapshot.state.bounds.width === 640,
          sequence: [...events] });
      } catch (error) { reject(error); }
    });
  });
  report("release-observation", observation);
  requireFact(observation.sourceAlive && observation.externalAlive, "strong source/external controls failed");
  requireFact(observation.live === 0, "after-drain source still retains current snapshot objects after one later-job GC");
  requireFact(events.join(",") === "Created,Created,Focus,transport-end,end", `wrong actual drain/end order ${events}`);
  requireFact(rejectedSync(() => source.all()).message.startsWith("KELD-IPC-022"), "after-drain source still serves state");
  requireFact(unexpectedDead === 0, "local terminal action reported unexpected death");
} else if (scenario === "default") {
  requireFact(link.windowSubscription === undefined, "default link sent a subscription");
  await readyPromise;
  await link.callFs(Uint8Array.of(0));
  link.close();
} else if (scenario === "replay" || scenario === "empty" || scenario === "subscribed-before-ready") {
  let accepted = false;
  void link.windowSubscription!.then((result) => { accepted = result.accepted; });
  if (scenario !== "subscribed-before-ready") {
    await readyPromise;
    // A real FS round trip is a host-observed barrier before Subscribe is answered.
    await link.callFs(Uint8Array.of(0));
    requireFact(!accepted, "raw Ready incorrectly completed the window subscription");
    report("ready-without-subscription", true);
  }
  const result = await link.windowSubscription!;
  requireFact(result.accepted, "subscription refused");
  const expected = scenario === "empty" ? [] : [3, 7];
  requireFact(JSON.stringify(source.all().map((value) => value.window.id)) === JSON.stringify(expected), "incomplete replay at subscription completion");
  if (scenario === "subscribed-before-ready") {
    requireFact(!rawReady, "window subscription invented raw Ready");
    report("subscribed-without-ready", true);
    await readyPromise;
  }
  for (let count = 0; count < 10; count += 1) source.all();
  report("subscription", result);
  report("windows", source.all().map((value) => value.window.id));
  link.close();
  await endedPromise;
  requireFact(unexpectedDead === 0, "local close reported unexpected link death");
} else if (scenario.startsWith("refusal-")) {
  const result = await link.windowSubscription!;
  requireFact(!result.accepted, "refusal treated as accepted");
  requireFact(source.all().length === 0 && !events.includes("end"), "refusal invalidated the source early");
  report("subscription", result);
  link.close();
  await endedPromise;
} else if (scenario === "malformed") {
  // Deliberately inspect the original rejected promise only after a real round trip.
  await link.callFs(Uint8Array.of(0));
  const error = await rejected(link.windowSubscription!);
  requireFact(error.message.startsWith("KELD-IPC-003") && error.name === "Error" && !("code" in error), "malformed reply did not preserve untyped codec failure");
  report("subscription-error-shape", { name: error.name, hasCode: "code" in error });
  requireFact(source.all().length === 0 && !events.includes("end"), "malformed reply invalidated source before transport drain");
  report("subscription-error", error.message);
  link.close();
  await endedPromise;
} else if (scenario === "subscription-err" || scenario === "subscription-peer-close" || scenario === "subscription-deadline") {
  const error = await rejected(link.windowSubscription!);
  const expected = scenario === "subscription-err" ? "KELD-CORE-042" : scenario === "subscription-peer-close" ? "KELD-IPC-022" : "KELD-IPC-006";
  requireFact(isCallError(error) && error.code === expected, `wrong transport/subscription error ${error.message}`);
  requireFact(rawDiagnostic !== undefined && error.code === rawDiagnostic.code && error.name === rawDiagnostic.name && error.message === rawDiagnostic.message, "subscription diagnostic changed actual RPC code/name/full message");
  report("exact-raw-diagnostic", true);
  report("subscription-error", { code: error.code, message: error.message });
  if (scenario !== "subscription-peer-close") link.close();
  await endedPromise;
} else if (scenario === "blocking-close") {
  requireFact((await link.windowSubscription!).accepted, "subscription refused");
  report("park-ready", true);
  const input = new Uint8Array(1);
  requireFact(readSync(0, input, 0, 1, null) === 1, "missing explicit park release");
  const answer = link.callWindowBlocking(Uint8Array.of(1));
  requireFact(answer.length === 1 && answer[0] === 1, "wrong parked answer");
  requireFact(source.get({ id: 7, incarnation: 300n })?.state.focused === false, "parked reply returned before mirror apply");
  requireFact(!events.includes("Focus"), "window listener ran before parked caller resumed");
  link.close();
  await endedPromise;
  requireFact(events.join(",") === "Created,Focus,end", `wrong close drain order ${events}`);
  requireFact(unexpectedDead === 0, "local close reported unexpected death");
  requireFact(rejectedSync(() => source.all()).message.startsWith("KELD-IPC-022"), "local close did not end source");
  requireFact((await rejected(link.callWindow(Uint8Array.of(1)))).message === "KELD-IPC-001: session is closed", "async call survived close");
  requireFact(rejectedSync(() => link.callWindowBlocking(Uint8Array.of(1))).message === "KELD-IPC-001: session is closed", "parked call survived close");
  report("drain", events);
} else if (scenario === "quit") {
  requireFact((await link.windowSubscription!).accepted, "subscription refused");
  const quit = link.quit();
  requireFact((await rejected(link.callWindow(Uint8Array.of(1)))).message === "KELD-IPC-001: session is closed", "async call survived pending Quit");
  requireFact(rejectedSync(() => link.callWindowBlocking(Uint8Array.of(1))).message === "KELD-IPC-001: session is closed", "parked call survived pending Quit");
  report("quit-guarded", true);
  await quit;
  await endedPromise;
  requireFact(events.join(",") === "Created,Focus,end", `wrong Quit drain order ${events}`);
  requireFact(unexpectedDead === 0, "local Quit reported unexpected death");
  requireFact(rejectedSync(() => source.all()).message.startsWith("KELD-IPC-022"), "Quit did not end source");
  report("drain", events);
} else if (scenario === "earlier-answer") {
  requireFact((await link.windowSubscription!).accepted, "subscription refused");
  const first = link.callWindow(Uint8Array.of(1));
  const pending = link.callWindow(Uint8Array.of(2));
  const pendingError = rejected(pending);
  const answer = await first;
  requireFact(answer.length === 1 && answer[0] === 9, "earlier retained answer lost");
  const error = await pendingError;
  requireFact(isCallError(error) && error.code === "KELD-IPC-022" && error.message.includes("KELD-IPC-005"), "unanswered caller lost source-failure cause");
  const recorded = await deadPromise;
  requireFact(isCallError(recorded) && recorded.code === "KELD-IPC-005", "link-death cause remapped");
  await endedPromise;
  requireFact(rejectedSync(() => source.all()).message.startsWith("KELD-IPC-005"), "failed mirror kept serving state");
  requireFact(events.join(",") === "Created,end", "failed fact dispatched or prior fact lost");
  requireFact((await rejected(link.callWindow(Uint8Array.of(1)))) === recorded, "session lost exact recorded error identity");
  report("earlier-answer", true);
} else throw new Error(`unknown consumer scenario ${scenario}`);
report("done", true);
