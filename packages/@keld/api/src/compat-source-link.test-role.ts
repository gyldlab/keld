/** Real WorkerLink; peer identities are fixtures, not native authentication proof. */
import assert from "node:assert/strict";
import { LifecycleLink } from "./link.ts";

const outcome = process.argv[2];
let connected: LifecycleLink | undefined;
let complete!: () => void;
let fail!: (error: unknown) => void;
const completed = new Promise<void>((resolve, reject) => {
  complete = resolve;
  fail = reject;
});
const session = await LifecycleLink.connect(process.env.KELD_APP_LINK!, {
  onConnected(value) {
    connected = value;
  },
  onReady() {
    try {
      assert(connected);
      // Capture directly in Ready: all ordered replay facts must already apply.
      connected
        .sendToCurrentWindow(
          { id: 7, incarnation: 258n },
          "isModified",
          Uint8Array.of(0),
          () => true,
        )
        .then(
          () => {
            if (outcome === "denied")
              fail(new Error("denied delivery resolved"));
            else complete();
          },
          (error: unknown) => {
            if (
              outcome === "denied" &&
              error !== null &&
              typeof error === "object" &&
              "code" in error &&
              error.code === "KELD-GUARD003"
            )
              complete();
            else fail(error);
          },
        );
    } catch (error) {
      fail(error);
    }
  },
  onCompatEvent() {
    fail(new Error("binding metadata reached an application listener"));
    return { variant: "NoListener" };
  },
  onCompatError(error) {
    fail(error);
  },
  onLastWindowClosed() {
    fail(new Error("unexpected window closure"));
  },
  onLinkDead(error) {
    fail(error);
  },
});
await completed;
session.close();
console.log("PASS bound-before-first-renderer-message " + outcome);
process.exit(0);
