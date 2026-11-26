// First-party facade/API/transport only, absent real endpoint. No original app,
// fabricated Host/Ready/window fact or alternate link is loaded.
import assert from "node:assert/strict";
import { app } from "../src/app.ts";
import { captureOriginalWindowQuitScope } from "@keld/api/internal/app";

assert.equal(process.env.KELD_APP_LINK, undefined);
assert.equal(app.isReady(), false);
assert.equal(
  captureOriginalWindowQuitScope({ id: 7, incarnation: 258n }),
  undefined,
);
let before = 0;
app.on("before-quit", () => {
  before++;
});
const result = app.quit();
assert.ok(result instanceof Promise);
await assert.rejects(
  result,
  process.env.KELD_NATIVE_WINDOW_FACADE === "1"
    ? /^Error: KELD-COMPAT-012:/
    : /^Error: KELD-IPC-007:/,
);
assert.equal(before, 1);
console.log("ORIGINAL_HANDOFF_UNQUALIFIED_REFUSAL_OK");
