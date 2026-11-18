/** Isolated Bun main realm. Never executes an original Electron application. */
import { strict as assert } from "node:assert";

const mode = process.env.KELD_TEST_DEFAULT_APP_CASE;
assert.ok(
  mode === "development-empty" ||
    mode === "development-file" ||
    mode === "missing-facts",
);

// Observe the actual Bun executable/script tuple, not a reconstructed argv.
const original = process.argv;
const snapshot = [...original];
const descriptor = Object.getOwnPropertyDescriptor(process, "argv");
assert.ok(
  descriptor?.configurable,
  "the test must observe the actual argv property",
);
let argvGetterReads = 0;
Object.defineProperty(process, "argv", {
  configurable: true,
  enumerable: descriptor.enumerable,
  get() {
    argvGetterReads += 1;
    return original;
  },
});
const { app } = await import("./index.ts");
const readsDuringFacadeImport = argvGetterReads;
Object.defineProperty(process, "argv", descriptor);
const { hostLaunchFacts } = await import("@keld/kipc");

if (mode === "missing-facts") {
  assert.equal(hostLaunchFacts, undefined);
  for (const [fact, read] of [
    ["appPath", () => app.getAppPath()],
    ["isPackaged", () => app.isPackaged],
  ] as const) {
    assert.throws(read, {
      name: "AppSessionFactError",
      code: "KELD-COMPAT-040",
      fact,
    });
  }
} else {
  assert.equal(hostLaunchFacts?.kind, "development");
}

assert.equal(
  readsDuringFacadeImport,
  0,
  "facade initialization must not derive launch mode from argv",
);
assert.strictEqual(
  process.argv,
  original,
  "the original Bun argv array must be retained",
);
assert.deepEqual(
  process.argv,
  snapshot,
  "no argument may be inserted, removed or rewritten",
);

const file = mode === "development-file" ? ["input.drawio"] : [];
assert.deepEqual(
  snapshot.slice(2),
  file,
  "Bun test role must start with executable and actual entry only",
);
const before = [null, ...snapshot]
  .slice(2)
  .filter((token): token is string => token !== null);
assert.deepEqual(
  before,
  [snapshot[1], ...file],
  "old absent-defaultApp branch makes the script a file",
);

const proc = process as typeof process & { defaultApp?: boolean };
const own = Object.getOwnPropertyDescriptor(proc, "defaultApp");
assert.equal(proc.defaultApp, true);
assert.ok(
  own,
  "the Bun main role declares its argument shape independently of Host facts",
);
assert.equal(own.value, true);
assert.equal(own.writable, false, "Electron defaultApp is read-only");
assert.equal(own.enumerable, true);
assert.equal(own.configurable, false);
assert.equal(
  typeof own.get,
  "undefined",
  "the result must not be a lazy argv getter",
);
// The original main's documented branch followed by parseDrawioArgs' slice(2).
const effective = (
  proc.defaultApp === true ? [...snapshot] : [null, ...snapshot]
)
  .slice(2)
  .filter((token): token is string => token !== null);
assert.deepEqual(effective, file);
console.log(
  JSON.stringify({
    mode,
    defaultApp: proc.defaultApp ?? null,
    own: own !== undefined,
    hostLaunchFactsPresent: hostLaunchFacts !== undefined,
    originalArgvPreserved: process.argv === original,
    readsDuringFacadeImport,
    oldParsedCount: before.length,
    parsedCount: effective.length,
  }),
);
