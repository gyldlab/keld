import { readFileSync } from "node:fs";
import assert from "node:assert/strict";
// Exercise the exact production pure helpers; no Bun/AST/native claim.
const source = readFileSync(
  new URL("./closed-app.ts", import.meta.url),
  "utf8",
);
const start = source.indexOf("function reservedTransport(");
const end = source.indexOf("const MAX_FILES", start);
const functions = source
  .slice(start, end)
  .replaceAll(": string", "")
  .replaceAll(": boolean", "")
  .replace(".at(-1)!", ".at(-1)");
const { reservedTransport, transportAlias } = new Function(
  functions + ";return {reservedTransport,transportAlias};",
)();
for (const name of [
  "kipc-transport.ts",
  "src/KIPC-TRANSPORT.js",
  "src/Kipc-Transport.TS",
])
  assert.equal(reservedTransport(name), true);
assert.equal(transportAlias("src/main.ts", "src/kipc-transport.ts"), true);
assert.equal(transportAlias("src/main.ts", "src/kipc-transport.js"), true);
assert.equal(transportAlias("main.js", "kipc-transport.js"), true);
assert.equal(transportAlias("src/main.js", "src/kipc-transport.ts"), false);
assert.equal(transportAlias("main.js", "kipc-transport.ts"), false);
assert.equal(transportAlias("src/main.ts", "other/kipc-transport.ts"), false);
assert.equal(transportAlias("src/main.ts", "src/KIPC-TRANSPORT.ts"), false);
console.log(
  "production alias helper tests passed (not compiler/native evidence)",
);

const resolverStart = source.indexOf("function relativeTarget(");
const resolverEnd = source.indexOf(
  "export interface ClosedApp {",
  resolverStart,
);
assert.ok(resolverStart !== -1 && resolverEnd > resolverStart);
const resolverFns = source
  .slice(resolverStart, resolverEnd)
  .replaceAll(": ReadonlyMap<string, string>", "")
  .replaceAll(": string", "")
  .replace("return candidates[0]!;", "return candidates[0];");
const { relativeTarget, resolveInventoryMember } = new Function(
  resolverFns + ";return {relativeTarget,resolveInventoryMember};",
)();
const admitted = new Map([
  ["src/main.ts", ""],
  ["src/other.js", ""],
  ["src/empty.ts", ""],
]);
assert.equal(relativeTarget("src/kipc.ts", "./main"), "src/main");
assert.equal(resolveInventoryMember("src/main", admitted), "src/main.ts");
assert.equal(resolveInventoryMember("src/other", admitted), "src/other.js");
assert.equal(resolveInventoryMember("src/main.ts", admitted), "src/main.ts");
assert.equal(resolveInventoryMember("src/empty", admitted), "src/empty.ts");
assert.throws(() => resolveInventoryMember("src/missing", admitted), /absent/);
assert.throws(
  () => resolveInventoryMember("src/missing.ts", admitted),
  /absent/,
);
assert.throws(
  () =>
    resolveInventoryMember(
      "src/ambiguous",
      new Map([
        ["src/ambiguous.ts", ""],
        ["src/ambiguous.js", ""],
      ]),
    ),
  /absent/,
);
assert.throws(
  () =>
    resolveInventoryMember(
      "src/missing.ts",
      new Map([["src/missing.ts.js", ""]]),
    ),
  /absent/,
);
assert.throws(() => relativeTarget("src/main.ts", "../../outside"), /escapes/);
assert.throws(
  () => relativeTarget("src/main.ts", "node:fs"),
  /Only explicit relative/,
);
console.log(
  "exact production in-inventory resolution tests passed (not native compiler proof)",
);
