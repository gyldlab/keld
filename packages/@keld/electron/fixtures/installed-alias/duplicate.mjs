import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";
import { WorkerLink } from "@keld/kipc";

const originalOpen = WorkerLink.open;
let opens = 0;
WorkerLink.open = function () {
  opens++;
  throw new Error("unexpected API operation");
};
const first = import.meta.resolve("electron");
const second = pathToFileURL(process.argv[2]).href;
const initial = await import(first);
assert.equal(await import(first), initial, "same cached evaluation must pass");
let rejected;
try {
  await import(second);
} catch (error) {
  rejected = error;
}
assert(rejected instanceof Error, "a second genuine entry must refuse");
assert.equal(rejected.code, "KELD-COMPAT-051");
assert(rejected.message.includes(first), "first observed URL required");
assert(rejected.message.includes(second), "second observed URL required");
assert.match(
  rejected.message,
  /Use one installed @keld\/electron package for all electron imports\./,
);
assert.equal(opens, 0, "duplicate refusal must not open an API link");
WorkerLink.open = originalOpen;
console.log(JSON.stringify({ code: rejected.code, first, second, opens }));
