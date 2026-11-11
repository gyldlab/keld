import { readFileSync } from "node:fs";
import assert from "node:assert/strict";
// Exercise the exact production pure helpers; no Bun/AST/native claim.
const source = readFileSync(new URL("./closed-app.ts", import.meta.url), "utf8");
const start = source.indexOf("function reservedTransport(");
const end = source.indexOf("const MAX_FILES", start);
const functions = source.slice(start, end).replaceAll(": string", "").replaceAll(": boolean", "").replace(".at(-1)!", ".at(-1)");
const { reservedTransport, transportAlias } = new Function(functions + ";return {reservedTransport,transportAlias};")();
for (const name of ["kipc-transport.ts", "src/KIPC-TRANSPORT.js", "src/Kipc-Transport.TS"]) assert.equal(reservedTransport(name), true);
assert.equal(transportAlias("src/main.ts", "src/kipc-transport.ts"), true);
assert.equal(transportAlias("src/main.ts", "src/kipc-transport.js"), true);
assert.equal(transportAlias("main.js", "kipc-transport.js"), true);
assert.equal(transportAlias("src/main.js", "src/kipc-transport.ts"), false);
assert.equal(transportAlias("main.js", "kipc-transport.ts"), false);
assert.equal(transportAlias("src/main.ts", "other/kipc-transport.ts"), false);
assert.equal(transportAlias("src/main.ts", "src/KIPC-TRANSPORT.ts"), false);
console.log("production alias helper tests passed (not compiler/native evidence)");
