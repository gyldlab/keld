/**
 * Controlled Host seam only: real singleton/readiness, real branded wrapper,
 * and real public adapter. No socket, native panel or visibility is simulated
 * as an acceptance claim. Native/WorkerLink integration belongs to the Host
 * owner's separate tests; a returned fixture index is not a witnessed click.
 */
import assert from "node:assert/strict";
import { LifecycleLink, type LifecycleHandler } from "./link.ts";
import { WindowSource } from "./window-source.ts";
import { encodeWindowEvent, type WindowRef } from "./window-codec.ts";
import type { DialogReplyV1, MessageOptions } from "./dialog.generated.ts";
import { linkError } from "@keld/kipc";

const mode = process.argv[2];
const ref: WindowRef = { id: 71, incarnation: 8001n };
const source = new WindowSource(() => {});
let callbacks: LifecycleHandler | undefined;
let connects = 0;
let open = true;
let reply: DialogReplyV1 = {
  variant: "Message",
  response: 0,
  checkbox_checked: false,
};
let failure: Error | undefined;
const calls: Array<{ parent: WindowRef | null; options: MessageOptions }> = [];

const session = {
  windowSource: source,
  get windowAdmissionOpen() {
    return open;
  },
  close() {
    open = false;
  },
  async quit() {
    open = false;
  },
  consumeAppliedLastWindowClosed: () => false,
  callMessageDialogBlocking(
    parent: WindowRef | null,
    options: MessageOptions,
  ): DialogReplyV1 {
    if (!open) throw linkError("KELD-IPC-001", "session is closed");
    calls.push({ parent, options });
    if (parent === null)
      throw linkError(
        "KELD-NATIVE-010",
        "parentless presentation is unsupported",
      );
    if (failure) throw failure;
    return reply;
  },
};

LifecycleLink.connect = (async (
  _address: string,
  handlers: LifecycleHandler,
) => {
  connects += 1;
  callbacks = handlers;
  handlers.onConnected?.(session as unknown as LifecycleLink);
  return session as unknown as LifecycleLink;
}) as typeof LifecycleLink.connect;
process.env.KELD_APP_LINK = "/controlled-gh564-no-socket#" + "a".repeat(64);

// Immutable upstream input, jgraph/drawio-desktop@2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e,
// src/main/electron.js:875-887. The source assigns modifiedModalOpen first.
const originalOptions = Object.freeze({
  type: "question" as const,
  buttons: Object.freeze(["Save", "Cancel", "Discard Changes"]),
  defaultId: 0,
  cancelId: 1,
  noLink: true,
  normalizeAccessKeys: true,
  title: "Confirm",
  message: "The document has unsaved changes. Do you want to save them?",
});
const expectedOptions: MessageOptions = {
  message: originalOptions.message,
  kind: "Question",
  buttons: ["Save", "Cancel", "Discard Changes"],
  default_id: 0,
  cancel_id: 1,
  normalize_access_keys: true,
  title: "Confirm",
  detail: null,
  no_link: true,
  checkbox_label: null,
  checkbox_checked: null,
};

function hasCode(error: unknown, code: string): boolean {
  return (
    error !== null &&
    typeof error === "object" &&
    "code" in error &&
    error.code === code
  );
}

async function main(): Promise<void> {
  const api = await import("./app.ts");
  const invokeBlocking = (): unknown => {
    const invoke = Reflect.get(api, "invokeMessageDialogBlocking");
    assert.equal(
      typeof invoke,
      "function",
      "the synchronous API entry is missing",
    );
    return Reflect.apply(invoke, undefined, [ref, expectedOptions]);
  };
  if (mode === "unconnected") {
    assert.throws(invokeBlocking, (error) => hasCode(error, "KELD-CORE-040"));
    assert.equal(
      connects,
      0,
      "sync refusal must not start an async connection",
    );
    assert.equal(calls.length, 0);
    return;
  }

  const ready = api.app.whenReady();
  assert.equal(connects, 1);
  assert.ok(callbacks);
  if (mode === "connected-before-ready") {
    assert.equal(api.app.isReady(), false);
    assert.throws(invokeBlocking, (error) => hasCode(error, "KELD-CORE-040"));
    assert.equal(connects, 1, "connected but unready must not reconnect");
    assert.equal(calls.length, 0);
    return;
  }

  source.apply(
    source.decode(
      encodeWindowEvent({
        variant: "Created",
        window: ref,
        state: {
          bounds: { x: 5, y: 10, width: 640, height: 480 },
          focused: true,
          maximized: false,
          full_screen: false,
        },
        pending_close: null,
      }),
    ),
  );
  callbacks.onReady();
  await ready;
  assert.equal(api.app.isReady(), true);

  if (mode === "failed-link") {
    const cause = linkError("KELD-IPC-022", "the original link ended");
    callbacks.onLinkDead(cause);
    assert.equal(api.app.isReady(), true, "Ready alone is not live admission");
    assert.throws(invokeBlocking, (error) => error === cause);
    assert.equal(calls.length, 0);
  } else if (mode === "quitting") {
    const quit = api.app.quit();
    assert.equal(
      open,
      false,
      "Quit closes admission before its Promise settles",
    );
    assert.throws(invokeBlocking, (error) => hasCode(error, "KELD-IPC-001"));
    assert.equal(calls.length, 0);
    await quit;
  } else {
    const electron = await import("../../electron/src/index.ts");
    const parent = electron.BrowserWindow.getAllWindows()[0];
    assert.ok(
      parent,
      "use the real branded wrapper for the retained Host pair",
    );
    assert.equal(parent.id, ref.id);
    assert.equal(electron.dialog, electron.default.dialog);
    const show = (...args: unknown[]): unknown =>
      Reflect.apply(electron.dialog.showMessageBoxSync, electron.dialog, args);

    if (mode === "ready-indices") {
      for (const selected of [0, 1, 2]) {
        reply = {
          variant: "Message",
          response: selected,
          checkbox_checked: false,
        };
        const result = show(parent, originalOptions);
        assert.equal(
          typeof result,
          "number",
          "no Promise may escape the adapter",
        );
        assert.equal(
          result,
          selected,
          "preserve the supplied native disposition",
        );
        assert.deepEqual(calls.at(-1)?.parent, ref);
        assert.deepEqual(calls.at(-1)?.options, expectedOptions);
      }
      assert.equal(calls.length, 3);
    } else if (mode === "source-validation") {
      let optionTraps = 0;
      const forged = Object.create(electron.BrowserWindow.prototype);
      const inspectOptions = (): never => {
        optionTraps += 1;
        throw new Error("options inspected before the parent brand");
      };
      const hostileOptions = new Proxy(
        { ...originalOptions },
        {
          get: inspectOptions,
          getPrototypeOf: inspectOptions,
          ownKeys: inspectOptions,
          getOwnPropertyDescriptor: inspectOptions,
        },
      );
      assert.throws(() => show(forged, hostileOptions), TypeError);
      assert.equal(
        optionTraps,
        0,
        "brand refusal must precede all option traps",
      );
      assert.throws(
        () => show(parent, { ...originalOptions, bogus: undefined }),
        /KELD-COMPAT-010/,
      );
      assert.throws(
        () => show(parent, { ...originalOptions, noLink: null }),
        /KELD-COMPAT-010/,
      );
      assert.equal(calls.length, 0, "invalid input sends no dialog");
      for (const args of [
        [originalOptions],
        [null, originalOptions],
        [undefined, originalOptions],
      ]) {
        assert.throws(
          () => show(...args),
          (error) => hasCode(error, "KELD-NATIVE-010"),
        );
      }
      assert.equal(
        calls.length,
        3,
        "all three absent-parent overloads must reach the Host",
      );
      assert.ok(
        calls.every((call) => call.parent === null),
        "parentless refusal must not retarget another genuine window",
      );
    } else if (mode === "native-failure") {
      for (const code of [
        "KELD-NATIVE-014",
        "KELD-NATIVE-015",
        "KELD-IPC-006",
        "KELD-IPC-023",
      ]) {
        failure = linkError(code, "controlled typed original failure");
        assert.throws(
          () => show(parent, originalOptions),
          (error) => error === failure,
        );
      }
      assert.equal(
        calls.length,
        4,
        "no typed error retries or fabricates Cancel",
      );
    } else if (mode === "wrong-reply") {
      reply = { variant: "Cancel", disposition: "Requested" };
      assert.throws(() => show(parent, originalOptions), /KELD-IPC-003/);
      assert.equal(
        calls.length,
        1,
        "a cleanup ACK cannot become a clicked index",
      );
    } else {
      throw new Error("unknown GH564 source test mode: " + String(mode));
    }
  }
  assert.equal(connects, 1, "reuse the role's existing singleton");
}

await main();
console.log("PASS GH564 source " + mode);
