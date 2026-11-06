/** Standalone lifetime regression; no link attachment or heap-byte/native claim. */
import { WindowSource, type WindowSnapshot } from "./window-source.ts";
import { encodeWindowEvent } from "./window-codec.ts";
import { linkError } from "@keld/kipc";
import { heapStats } from "bun:jsc";

function requireFact(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}
const runtime = {
  version: Bun.version,
  revision: Bun.revision,
  platform: process.platform,
  arch: process.arch,
};
requireFact(
  process.argv[3] === runtime.version &&
    process.argv[4] === runtime.revision &&
    process.argv[5] === runtime.platform &&
    process.argv[6] === runtime.arch,
  "Source release child must use the same Bun runtime as its parent; check the helper arguments and executable.",
);

// Bun.gc(true) is the pinned synchronous diagnostic hook. It finalizes
// synchronous JS execution around a full collection, not a portable ECMAScript
// collection guarantee. Each collection below runs once in a completed later job.
// Exercised primary: oven-sh/bun@744846f8
// src/jsc/bindings/bindings.cpp:4969–4993. This lifetime oracle does not qualify
// heap-byte budgets on a future runtime; the repository setup owns its Bun pin.
function nextJob<T>(run: () => T): Promise<T> {
  return new Promise((resolve, reject) =>
    setImmediate(() => {
      try {
        resolve(run());
      } catch (error) {
        reject(error);
      }
    }),
  );
}
function track(snapshot: WindowSnapshot): readonly WeakRef<object>[] {
  return [
    new WeakRef(snapshot),
    new WeakRef(snapshot.window),
    new WeakRef(snapshot.state),
    new WeakRef(snapshot.state.bounds),
  ];
}
function live(refs: readonly WeakRef<object>[]): number {
  return refs.filter((ref) => ref.deref() !== undefined).length;
}

// Same source/runtime/workload in both arms. Caller-owned snapshot retention
// is the negative control; raw JSC heapSize is an observation, never a cap.
const historyMode = /^history:(0|1|1024):(retire|retain)$/.exec(
  process.argv[2]!,
);
if (historyMode) {
  const count = Number(historyMode[1]);
  const retaining = historyMode[2] === "retain";
  const roots = (() => {
    const witness = { reason: "shared ordinary callback control" };
    const callback = () => {
      requireFact(witness.reason.length > 0, "callback root missing");
    };
    return {
      source: new WindowSource(callback),
      ordinary: [callback],
      weak: new WeakRef(witness),
    };
  })();
  const tracked: Array<readonly WeakRef<object>[]> = [];
  const held: WindowSnapshot[] = [];
  await nextJob(() => {
    for (let id = 1; id <= count; id += 1) {
      const window = { id, incarnation: BigInt(id) };
      roots.source.apply({
        variant: "Created",
        window,
        state: {
          bounds: { x: 0, y: 0, width: 640, height: 480 },
          focused: false,
          maximized: false,
          full_screen: false,
        },
        pending_close: null,
      });
      const terminal = roots.source.decode(
        encodeWindowEvent({ variant: "Destroyed", window }),
      );
      roots.source.apply(terminal);
      const value = roots.source.get(window)!;
      tracked.push(track(value));
      if (retaining) held.push(value);
      roots.source.dispatch(terminal);
    }
  });
  const settled = await nextJob(() => {
    Bun.gc(true);
    const stats = heapStats();
    return {
      objectsLive: tracked.reduce((sum, refs) => sum + live(refs), 0),
      heapSize: stats.heapSize,
      objectCount: stats.objectCount,
      snapshotsVisible: roots.source.all().length,
      sharedCallbackAlive: roots.weak.deref() !== undefined,
    };
  });
  console.log(
    JSON.stringify({
      runtime,
      history: count,
      retaining,
      live: 0,
      pending: 0,
      facts: 0,
      held: held.length,
      settled,
    }),
  );
  requireFact(
    settled.objectsLive === (retaining ? count * 4 : 0),
    "settled Source graph release oracle failed",
  );
  requireFact(
    settled.snapshotsVisible === 0 && settled.sharedCallbackAlive,
    "retirement/callback controls failed",
  );
  await nextJob(() =>
    roots.source.end(linkError("KELD-IPC-022", "history after drain")),
  );
  await nextJob(() => {
    Bun.gc(true);
    requireFact(
      roots.weak.deref() !== undefined,
      "independent ordinary callback root must still retain its shared capture after Source.end",
    );
    roots.ordinary[0]!();
    roots.ordinary.length = 0;
    held.length = 0;
  });
  await nextJob(() => {
    Bun.gc(true);
    requireFact(
      roots.weak.deref() === undefined,
      "shared callback control must release after its independent root is removed",
    );
    requireFact(
      tracked.every((refs) => live(refs) === 0),
      "caller-held graph control must release when its owner drops it",
    );
  });
  process.exit(0);
}

throw new Error("unknown Source history mode");
