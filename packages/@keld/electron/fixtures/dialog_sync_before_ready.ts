/** Pristine synchronous refusal; native acceptance belongs to the Host suite. */
import { strict as assert } from "node:assert";
import { isCallError, LifecycleLink } from "../src/link.ts";
import { isUnsupportedMemberError } from "../src/unsupported.ts";

const originalConnect = LifecycleLink.connect;
const originalInvoke = LifecycleLink.prototype.callMessageDialogBlocking;
let connects = 0;
let invokes = 0;
LifecycleLink.connect = () => {
  connects += 1;
  throw new Error("unready synchronous Message attempted an app-link connect");
};
LifecycleLink.prototype.callMessageDialogBlocking = () => {
  invokes += 1;
  throw new Error("unready synchronous Message attempted an app-link call");
};

try {
  const electron = await import("../src/index.ts");
  assert.equal(electron.app.isReady(), false);
  for (const root of [electron, electron.default]) {
    const fn = root.dialog.showMessageBoxSync;
    assert.equal(typeof fn, "function");
    assert.strictEqual(fn, electron.dialog.showMessageBoxSync);
    let returned = false;
    let result: unknown;
    let caught: unknown;
    try {
      result = fn({
        message: "unsaved",
        buttons: ["Save", "Cancel", "Discard Changes"],
      });
      returned = true;
    } catch (error) {
      caught = error;
    }
    // A Promise substitute fails the synchronous-return assertion regardless
    // of its eventual outcome; observing a rejection only avoids an orphan.
    if (result instanceof Promise) void result.catch(() => {});
    assert.equal(returned, false);
    assert.ok(isCallError(caught));
    assert.equal(isUnsupportedMemberError(caught), false);
    assert.equal(caught.code, "KELD-CORE-040");
    assert.match(caught.message, /^KELD-CORE-040:/);
  }
  assert.equal(connects, 0);
  assert.equal(invokes, 0);
  assert.equal(electron.app.isReady(), false);
} finally {
  LifecycleLink.connect = originalConnect;
  LifecycleLink.prototype.callMessageDialogBlocking = originalInvoke;
}
console.log("GH564_SYNC_BEFORE_READY_OK");
