/**
 * GH1186 Mac-only child: unmodified S1 public API and Electron getters on the
 * actual WorkerLink. The parent is a scripted peer, never a native Host.
 */
import assert from "node:assert/strict";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import {
  hostLaunchFacts,
  nativeFacadeBootSelected,
  WorkerLink,
} from "@keld/kipc";
import {
  app,
  isDesktopS1Selected,
  onWindowFactApplied,
} from "@keld/api/internal/app";

const root = process.env.W4_SHARED_ROOT;
assert.ok(root, "verified S1 export is required");
assert.equal(nativeFacadeBootSelected, true);
assert.equal(hostLaunchFacts?.kind, "development");
assert.equal(isDesktopS1Selected(), true);

// The direct source and the package export must name the same singleton.
const directApi = await import(
  pathToFileURL(join(root, "packages/@keld/api/src/app.ts")).href
);
assert.equal(app, directApi.app);
const { screen } = await import(
  pathToFileURL(join(root, "packages/@keld/electron/src/screen.ts")).href
);
const { nativeTheme } = await import(
  pathToFileURL(join(root, "packages/@keld/electron/src/native-theme.ts")).href
);
const { channels, echoChannel } = await import(
  pathToFileURL(join(root, "packages/@keld/api/src/channels.ts")).href
);
channels.handle(
  echoChannel,
  (request: { message: string; count: number }) => request,
);

let receivedFacts = 0;
const factOrder: string[] = [];
function send(value: object): void {
  assert.ok(process.send, "role must have its owned test parent");
  process.send(value);
}
function code(error: unknown): string {
  return (error as { code?: string } | null)?.code ?? String(error);
}
function readGetters(): object {
  return {
    all: screen.getAllDisplays(),
    primary: screen.getPrimaryDisplay(),
    dark: nativeTheme.shouldUseDarkColors,
  };
}
function getterError(attempt: () => unknown): string | undefined {
  try {
    attempt();
  } catch (error) {
    return code(error);
  }
  return undefined;
}
function getterErrors() {
  return {
    all: getterError(() => screen.getAllDisplays()),
    primary: getterError(() => screen.getPrimaryDisplay()),
    dark: getterError(() => nativeTheme.shouldUseDarkColors),
  };
}

onWindowFactApplied((fact) => {
  receivedFacts += 1;
  factOrder.push(fact.variant);
  send({
    kind: "fact",
    variant: fact.variant,
    epoch: fact.variant === "DesktopSnapshot" ? fact.epoch.toString() : null,
    publicReady: app.isReady(),
  });
});
let readyOutcome = "pending";
const whenReady = app.whenReady().then(
  () => {
    readyOutcome = "ready";
    send({ kind: "ready-result", outcome: "ready", ready: app.isReady() });
  },
  (error: unknown) => {
    readyOutcome = "rejected";
    send({
      kind: "ready-result",
      outcome: "rejected",
      code: code(error),
      ready: app.isReady(),
    });
  },
);

process.on("message", (input: unknown) => {
  void (async () => {
    assert.ok(input && typeof input === "object");
    const message = input as {
      command: string;
      id: number;
      count?: number;
      expected?: { all: unknown; primary: unknown; dark: boolean };
    };
    if (message.command === "checkpoint") {
      const errors = getterErrors();
      send({
        kind: "response",
        id: message.id,
        ready: app.isReady(),
        readyOutcome,
        getterError: errors.all,
        getterErrors: errors,
        receivedFacts,
        factOrder: [...factOrder],
      });
    } else if (message.command === "getters") {
      assert.ok(message.expected, "independent Host oracle is required");
      assert.equal(app.isReady(), true);
      const originalCall = WorkerLink.prototype.call;
      const originalBlocking = WorkerLink.prototype.callBlocking;
      let hostCalls = 0;
      const unexpected = (): never => {
        hostCalls += 1;
        throw new Error("desktop getter crossed the Host call boundary");
      };
      WorkerLink.prototype.call = unexpected;
      WorkerLink.prototype.callBlocking = unexpected;
      try {
        for (let i = 0; i < (message.count ?? 1); i += 1) {
          const actual = readGetters();
          assert.deepEqual(actual, message.expected);
          // Electron Display is an isolated copy, not an alias into the mirror.
          screen.getPrimaryDisplay().workArea.x = 999999;
          assert.deepEqual(readGetters(), message.expected);
        }
      } finally {
        WorkerLink.prototype.call = originalCall;
        WorkerLink.prototype.callBlocking = originalBlocking;
      }
      send({
        kind: "response",
        id: message.id,
        hostCalls,
        count: message.count ?? 1,
        receivedFacts,
      });
    } else if (message.command === "check-failure") {
      const expected = message.expected as unknown as { code: string };
      const errors = getterErrors();
      assert.deepEqual(errors, {
        all: expected.code,
        primary: expected.code,
        dark: expected.code,
      });
      assert.equal(
        app.isReady(),
        expected.code === "KELD-CORE-049" && readyOutcome === "ready",
      );
      // A readiness failure is sticky for this role, including a later attempt.
      if (readyOutcome === "rejected") {
        let later = "resolved";
        try {
          await app.whenReady();
        } catch (error) {
          later = code(error);
        }
        assert.equal(later, expected.code);
      }
      send({
        kind: "response",
        id: message.id,
        ready: app.isReady(),
        readyOutcome,
        getterError: errors.all,
      });
    } else if (message.command === "finish") {
      await whenReady;
      send({ kind: "response", id: message.id, readyOutcome, receivedFacts });
      console.log(
        JSON.stringify({ cleanup: true, readyOutcome, receivedFacts }),
      );
      process.exit(0);
    } else {
      throw new Error(`unrecognized getter-chain command ${message.command}`);
    }
  })().catch((error) => {
    send({ kind: "fatal", error: String(error) });
    console.error(error);
    process.exit(1);
  });
});
