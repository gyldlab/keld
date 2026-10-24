/** Standalone lifetime regression; no link attachment or heap-byte/native claim. */
import { WindowSource, type WindowSnapshot } from "./window-source.ts";
import { encodeWindowEvent } from "./window-codec.ts";
import { isCallError, linkError } from "../../kipc/src/transport.ts";

function requireFact(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}
const runtime = { version: Bun.version, revision: Bun.revision, platform: process.platform, arch: process.arch };
requireFact(process.argv[3] === runtime.version && process.argv[4] === runtime.revision && process.argv[5] === runtime.platform && process.argv[6] === runtime.arch, "Source release child must use the same Bun runtime as its parent; check the helper arguments and executable.");

// Bun.gc(true) is the pinned synchronous diagnostic hook. It finalizes
// synchronous JS execution around a full collection, not a portable ECMAScript
// collection guarantee. Each collection below runs once in a completed later job.
// Exercised primary: oven-sh/bun@744846f8
// src/jsc/bindings/bindings.cpp:4969–4993. This lifetime oracle does not qualify
// heap-byte budgets on a future runtime; the repository setup owns its Bun pin.
function nextJob<T>(run: () => T): Promise<T> {
  return new Promise((resolve, reject) => setImmediate(() => {
    try { resolve(run()); } catch (error) { reject(error); }
  }));
}
function track(snapshot: WindowSnapshot): readonly WeakRef<object>[] {
  return [new WeakRef(snapshot), new WeakRef(snapshot.window), new WeakRef(snapshot.state), new WeakRef(snapshot.state.bounds)];
}
function live(refs: readonly WeakRef<object>[]): number {
  return refs.filter((ref) => ref.deref() !== undefined).length;
}

const source = new WindowSource(() => { throw new Error("standalone source must not dispatch"); });
const anchors: { source: WindowSource; externalSnapshot: WindowSnapshot | undefined } = { source, externalSnapshot: undefined };
const window = { id: 7, incarnation: 300n };
const refs: Array<readonly WeakRef<object>[]> = [];
await nextJob(() => {
  // The parent supplies the Rust-owned event-created vector, without a second
  // window decoder or a replacement for the actual source implementation.
  const fact = source.decode(Uint8Array.from(Buffer.from(process.argv[2]!, "hex")));
  requireFact(fact.variant === "Created" && fact.window.id === 7 && fact.window.incarnation === 300n, "wrong Rust Created fixture");
  source.apply(fact);
  refs.push(track(source.get(window)!));
  for (let index = 0; index < 32; index += 1) {
    source.apply(source.decode(encodeWindowEvent({ variant: "Focus", window, focused: index % 2 !== 0 })));
    refs.push(track(source.get(window)!));
  }
  source.apply(source.decode(encodeWindowEvent({ variant: "Created", window: { id: 3, incarnation: 900n }, state: fact.state, pending_close: null })));
  anchors.externalSnapshot = source.get({ id: 3, incarnation: 900n });
  requireFact(refs.reduce((count, graph) => count + live(graph), 0) === 132, "same-job WeakRef pre-collection control failed");
});

const replacement = await nextJob(() => {
  Bun.gc(true);
  const observation = {
    trackedGraphs: refs.length,
    oldObjectsLive: refs.slice(0, -1).reduce((count, graph) => count + live(graph), 0),
    currentObjectsLive: live(refs[refs.length - 1]!),
    sourceAlive: anchors.source === source,
    externalSnapshotAlive: anchors.externalSnapshot?.window.id === 3 && anchors.externalSnapshot.state.bounds.width === 640,
    census: source.all().length,
  };
  requireFact(observation.oldObjectsLive === 0, "source retained replaced snapshot objects after one later-job GC");
  requireFact(observation.currentObjectsLive === 4 && observation.sourceAlive && observation.externalSnapshotAlive && observation.census === 2, "replacement strong current/source/external controls failed");
  return observation;
});

const terminal = linkError("KELD-IPC-022", "standalone source release regression ended");
await nextJob(() => source.end(terminal));
const end = await nextJob(() => {
  Bun.gc(true);
  let readsDenied = false;
  try { source.all(); } catch (error) { readsDenied = isCallError(error) && error.code === terminal.code && error.message === terminal.message; }
  const observation = {
    oldObjectsLive: refs.slice(0, -1).reduce((count, graph) => count + live(graph), 0),
    currentObjectsLive: live(refs[refs.length - 1]!),
    sourceAlive: anchors.source === source,
    externalSnapshotAlive: anchors.externalSnapshot?.window.id === 3 && anchors.externalSnapshot.state.bounds.width === 640,
    readsDenied,
  };
  requireFact(observation.oldObjectsLive === 0 && observation.currentObjectsLive === 0, "source retained current snapshot objects after end and one later-job GC");
  requireFact(observation.sourceAlive && observation.externalSnapshotAlive && observation.readsDenied, "end strong source/external/invalidation controls failed");
  return observation;
});
console.log(JSON.stringify({ runtime, replacement, end }));
