/** Real WorkerLink retention order; only its dispatch timing is controlled. */
import assert from "node:assert/strict";
import * as transport from "@keld/kipc";
import { openWorkerLinkForTest } from "../../kipc/src/test-hooks.ts";

let firstBatch = 0;
transport.WorkerLink.open = async (options) => {
  const words = new Int32Array(
    new SharedArrayBuffer(4 * transport.WORKER_LINK_TEST_WORDS.LENGTH),
  );
  const { link, control } = await openWorkerLinkForTest(options, {
    words,
    deferDispatch(run) {
      if (firstBatch !== 0) return run();
      function waitForBatch() {
        const { W_RECS, R_RECS } = transport.WORKER_LINK_CONTROL;
        const retained =
          Atomics.load(control, W_RECS) - Atomics.load(control, R_RECS);
        if (retained < 3) return void setImmediate(waitForBatch);
        firstBatch = retained;
        run();
      }
      waitForBatch();
    },
  });
  return link;
};

const { LifecycleLink } = await import("./link.ts");
let ready = 0;
const connected = LifecycleLink.connect(process.env.KELD_APP_LINK!, {
  onReady() {
    ready += 1;
  },
  onLastWindowClosed() {},
  onLinkDead() {},
});
if (process.argv[2] === "ack-first") {
  await assert.rejects(
    connected,
    (error) =>
      transport.isCallError(error) &&
      error.code === "KELD-IPC-005" &&
      error.message.includes("Subscribe before an available snapshot"),
  );
  assert.equal(ready, 0);
} else {
  const link = await connected;
  assert.equal(ready, 1);
  assert.equal(link.windowSource?.desktop()?.primary_id, 17);
  link.close();
}
assert.equal(firstBatch, 3);
process.exit(0);
