/**
 * Observer for the GH-565 "no Host traffic" criterion (spec §3 AC4).
 *
 * `LifecycleLink.connect`, the one funnel every Host path of the facade uses
 * (`@keld/api/src/app.ts` ensureLink), is replaced by a recorder *before* the facade
 * loads. The observer then imports the facade and touches every own member of the nine
 * link-time bindings (`ipcMain` is PR #1365's, spec D5): it calls each function with no
 * arguments, reads each accessor, constructs each class with `{}` and, where construction
 * succeeds, touches the prototype members on that instance. The parent sets
 * `KELD_APP_LINK`, so a stray `ensureLink()` reaches the recorder instead of failing early
 * on the unset variable. It prints one `GH565_NO_LINK` line and exits 0, or exits 2 when a
 * binding is missing.
 * `src/module-bindings.test.ts` holds the oracle; this file derives nothing from the
 * member table under test.
 */
import { writeSync } from "node:fs";
import { LifecycleLink } from "../src/link.ts";

let connects = 0;
LifecycleLink.connect = (async () => {
  connects += 1;
  throw new Error("GH565_UNEXPECTED_CONNECT");
}) as typeof LifecycleLink.connect;

const electron = await import("../src/index.ts");

const BINDINGS = [
  "Menu",
  "shell",
  "dialog",
  "session",
  "screen",
  "ClipboardItem",
  "clipboard",
  "nativeImage",
  "nativeTheme",
] as const;

/** Own names every function has; they are not Electron members. */
const FUNCTION_OWN = new Set(["length", "name", "prototype"]);

let touched = 0;
let threw = 0;
let rejected = 0;
const pending: Promise<void>[] = [];

function touch(target: object, key: string, receiver: object): void {
  touched += 1;
  let value: unknown;
  try {
    value = Reflect.get(target, key, receiver);
  } catch {
    threw += 1;
    return;
  }
  if (typeof value !== "function") return;
  let result: unknown;
  try {
    result = Reflect.apply(value, receiver, []);
  } catch {
    threw += 1;
    return;
  }
  if (result instanceof Promise) {
    pending.push(
      result.then(
        () => {},
        () => {
          rejected += 1;
        },
      ),
    );
  }
}

for (const name of BINDINGS) {
  const binding: unknown = electron[name];
  if (
    (typeof binding !== "object" && typeof binding !== "function") ||
    binding === null
  ) {
    writeSync(2, `GH565_BINDING_MISSING ${name}\n`);
    process.exit(2);
  }
  const skip = typeof binding === "function" ? FUNCTION_OWN : new Set<string>();
  for (const key of Object.getOwnPropertyNames(binding)) {
    if (!skip.has(key)) touch(binding, key, binding);
  }
  if (typeof binding === "function") {
    touched += 1;
    let instance: object | undefined;
    try {
      instance = Reflect.construct(binding, [{}]) as object;
    } catch {
      threw += 1;
    }
    const prototype = Reflect.get(binding, "prototype") as object;
    for (const key of Object.getOwnPropertyNames(prototype)) {
      if (key !== "constructor") touch(prototype, key, instance ?? prototype);
    }
  }
}

await Promise.all(pending);
writeSync(
  1,
  `GH565_NO_LINK connect=${connects} touched=${touched} threw=${threw} rejected=${rejected}\n`,
);
process.exit(0);
