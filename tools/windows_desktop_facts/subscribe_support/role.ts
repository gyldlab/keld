/**
 * W4 test role. Loads the unmodified S1 LifecycleLink/WorkerLink.
 * Only the surrounding test Host supplies frames; no Windows Host activation.
 */
import assert from "node:assert/strict";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const sharedRoot = process.env.W4_SHARED_ROOT;
assert.ok(
  sharedRoot,
  "W4_SHARED_ROOT must identify verified immutable S1 files",
);
const transport = await import(
  pathToFileURL(join(sharedRoot, "packages/@keld/kipc/src/transport.ts")).href
);
const { LifecycleLink } = await import(
  pathToFileURL(join(sharedRoot, "packages/@keld/api/src/link.ts")).href
);
assert.equal(
  transport.nativeFacadeBootSelected,
  process.env.KELD_NATIVE_WINDOW_FACADE === "1",
);
assert.equal(
  transport.hostLaunchFacts,
  undefined,
  "no fabricated Host launch facts",
);

type Fact = { variant: string; epoch?: bigint };
type Source = {
  desktop(): unknown;
  decode(bytes: Uint8Array): unknown;
  apply(fact: unknown): void;
  dispatch(fact: unknown): void;
};
type Session = { windowSource?: Source; close(): void };
let session: Session | undefined;
let settled = false;
let settledCode: string | undefined;
let readyCount = 0;
let factCount = 0;
let lastFact: Fact | undefined;
let connectedAt = 0;

function code(error: unknown): string {
  return transport.isCallError(error)
    ? (error as { code: string }).code
    : "untyped:" + String(error);
}
function send(value: object): void {
  if (!process.send)
    throw new Error("test role must have its owned IPC parent");
  process.send(value);
}
function snapshot(): { value?: unknown; error?: string; hasSource: boolean } {
  const source = session?.windowSource;
  if (!source) return { hasSource: false };
  try {
    return { hasSource: true, value: source.desktop() };
  } catch (error) {
    return { hasSource: true, error: code(error) };
  }
}
const nextTurn = () => new Promise<void>((resolve) => setImmediate(resolve));

async function retire() {
  const source = session?.windowSource;
  session?.close();
  if (!source) return { source: false, staleApply: "not-applicable" };
  let terminal = "";
  await transport.withIoDeadline(
    (async () => {
      for (;;) {
        try {
          source.desktop();
        } catch (error) {
          terminal = code(error);
          return;
        }
        await nextTurn();
      }
    })(),
    transport.APP_LINK_IO_DEADLINE_MS,
  );
  let staleApply = "no-fact";
  if (lastFact) {
    try {
      source.apply(lastFact);
      staleApply = "accepted";
    } catch (error) {
      staleApply = code(error);
    }
    const before = factCount;
    source.dispatch(lastFact);
    assert.equal(
      factCount,
      before,
      "retired source must not revive its callback",
    );
  }
  return { source: true, terminal, staleApply, facts: factCount };
}

process.on("message", (input: unknown) => {
  void (async () => {
    assert.ok(input && typeof input === "object");
    const message = input as { command: string; id: number; count?: number };
    if (message.command === "checkpoint") {
      await nextTurn();
      send({
        kind: "response",
        id: message.id,
        settled,
        settledCode,
        readyCount,
        factCount,
        snapshot: snapshot(),
      });
    } else if (message.command === "getters") {
      const source = session?.windowSource;
      assert.ok(source, "getter check requires the real subscribed source");
      const before = factCount;
      const originalCall = transport.WorkerLink.prototype.call;
      const originalBlocking = transport.WorkerLink.prototype.callBlocking;
      let hostCalls = 0;
      const denyCall = () => {
        hostCalls += 1;
        throw new Error("a desktop getter crossed the Host call boundary");
      };
      transport.WorkerLink.prototype.call = denyCall;
      transport.WorkerLink.prototype.callBlocking = denyCall;
      let first: unknown;
      try {
        for (let index = 0; index < (message.count ?? 1000); index += 1) {
          const current = source.desktop();
          if (index === 0) first = current;
          assert.deepEqual(current, first);
        }
      } finally {
        transport.WorkerLink.prototype.call = originalCall;
        transport.WorkerLink.prototype.callBlocking = originalBlocking;
      }
      send({
        kind: "response",
        id: message.id,
        reads: message.count ?? 1000,
        hostCalls,
        callbacksDuringReads: factCount - before,
        snapshot: first,
      });
    } else if (message.command === "retire") {
      send({ kind: "response", id: message.id, ...(await retire()) });
    } else if (message.command === "finish") {
      const cleanup = await retire();
      console.log(JSON.stringify({ cleanup, pid: process.pid }));
      process.exit(0);
    } else {
      throw new Error("unknown test command " + message.command);
    }
  })().catch((error) => {
    send({ kind: "fatal", error: String(error) });
    console.error(error);
    process.exit(1);
  });
});

void LifecycleLink.connect(process.env.W4_TEST_LINK!, {
  onConnected(value: Session) {
    session = value;
    connectedAt = performance.now();
    send({ kind: "connected", snapshot: snapshot() });
  },
  onReady() {
    readyCount += 1;
    send({ kind: "ready", readyCount, snapshot: snapshot() });
  },
  onLastWindowClosed() {
    send({ kind: "last-window-closed" });
  },
  onWindowFact(fact: Fact) {
    lastFact = fact;
    factCount += 1;
    send({
      kind: "fact",
      variant: fact.variant,
      epoch: fact.epoch?.toString(),
      snapshot: snapshot(),
    });
  },
  onLinkDead(error: unknown) {
    send({ kind: "dead", code: code(error), snapshot: snapshot() });
  },
  async onApplicationCall(_channel: number, payload: Uint8Array) {
    return payload;
  },
}).then(
  () => {
    settled = true;
    send({ kind: "settled", snapshot: snapshot() });
  },
  (error: unknown) => {
    settled = true;
    settledCode = code(error);
    send({
      kind: "rejected",
      code: settledCode,
      elapsedMs: connectedAt ? performance.now() - connectedAt : null,
      snapshot: snapshot(),
    });
  },
);
