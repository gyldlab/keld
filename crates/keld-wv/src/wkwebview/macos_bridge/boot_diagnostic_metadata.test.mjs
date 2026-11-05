import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
const source = readFileSync(
  new URL("./boot_diagnostic_common.js", import.meta.url),
  "utf8",
);
const factory = Function(`"use strict";return (${source});`)();
const metadata = factory(() => {});
let parses = 0;
const parse = (raw) => {
  parses++;
  return JSON.parse(raw);
};
const oversized = metadata.readAttribute("x".repeat(4097), parse);
assert.equal(parses, 0, "oversize metadata must not reach JSON.parse");
assert.equal(oversized.status, "oversize");
const row = {
  kind: "js-error",
  class: "TypeError",
  resource: "inert-control",
  line: 1,
  column: 1,
  directive: "other",
  blocked: "other",
};
const raw = JSON.stringify({ v: 1, records: [row], dropped: 0 });
assert.deepEqual(metadata.readAttribute(raw, parse), {
  status: "ok",
  records: [row],
  dropped: 0,
});
assert.equal(parses, 1);
assert.equal(metadata.readAttribute(null, parse).status, "absent");
assert.equal(
  metadata.readAttribute(
    JSON.stringify({
      v: 1,
      records: [{ ...row, message: "not allowed" }],
      dropped: 0,
    }),
    parse,
  ).status,
  "malformed",
);
assert.equal(
  metadata.readAttribute(
    JSON.stringify({
      v: 1,
      records: [{ ...row, resource: "https://private.invalid/secret" }],
      dropped: 0,
    }),
    parse,
  ).status,
  "malformed",
);
assert.equal(
  metadata.readAttribute(
    JSON.stringify({ v: 1, records: Array(9).fill(row), dropped: 0 }),
    parse,
  ).status,
  "malformed",
);
assert.equal(metadata.readAttribute("{", parse).status, "malformed");
console.log(
  "BOOT_DIAGNOSTIC_METADATA_CAPS_PASS no_application_or_native_execution",
);
