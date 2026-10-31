/** Independent original-Error lifetime oracle; no source/link/heap introspection. */
import { WindowSource } from "./window-source.ts";
import { isCallError, linkError } from "@keld/kipc";

function requireFact(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}
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
const runtime = {
  version: Bun.version,
  revision: Bun.revision,
  platform: process.platform,
  arch: process.arch,
};
requireFact(
  process.argv[2] === runtime.version &&
    process.argv[3] === runtime.revision &&
    process.argv[4] === runtime.platform &&
    process.argv[5] === runtime.arch,
  "Diagnostic release child must use its parent's Bun runtime.",
);
const faultSource = new WindowSource(() => {});
const endSource = new WindowSource(() => {});
const anchors = {
  faultSource,
  endSource,
  externalError: linkError(
    "KELD-IPC-022",
    "deliberately retained external Error control",
  ),
};
const externalRef = new WeakRef(anchors.externalError);
const faultRef = await nextJob(() => {
  try {
    faultSource.decode(Uint8Array.of(7));
  } catch (error) {
    requireFact(
      isCallError(error) && error.code === "KELD-IPC-005",
      "missing actual source-created fault",
    );
    return new WeakRef(error);
  }
  throw new Error("source fault unexpectedly succeeded");
});
const endRef = await nextJob(() => {
  const original = linkError(
    "KELD-IPC-022",
    "actual supplied terminal Error release witness",
  );
  endSource.end(original);
  return new WeakRef(original);
});
const observation = await nextJob(() => {
  // The repository owns the runtime pin. This is one later-job diagnostic full
  // collection, with independent strong controls, not a portable collection or
  // physical-byte guarantee. Exercised hook: Bun@744846f8 bindings.cpp:4969–4993.
  Bun.gc(true);
  return {
    runtime,
    faultErrorAlive: faultRef.deref() !== undefined,
    endErrorAlive: endRef.deref() !== undefined,
    externalErrorAlive: externalRef.deref() === anchors.externalError,
    sourcesAlive:
      anchors.faultSource === faultSource && anchors.endSource === endSource,
  };
});
console.log(JSON.stringify(observation));
requireFact(
  !observation.faultErrorAlive && !observation.endErrorAlive,
  "source retained original diagnostic Errors after one later-job GC",
);
requireFact(
  observation.externalErrorAlive && observation.sourcesAlive,
  "diagnostic release strong source/external controls failed",
);
