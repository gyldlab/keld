/**
 * Role half of the `LifecycleLink` contract tests in `src/link.test.ts`.
 *
 * A role opens one `WorkerLink` per realm (GH-527 §4.2), so each case runs
 * in its own process: the test binds a Unix host, spawns
 * `bun lifecycle_link_role.ts <scenario>` with `KELD_APP_LINK`, drives the
 * host half and asserts the `KELD_LL key=value` lines this file prints. Every
 * wait is on an observable (a frame, a handler, a settled promise); none is a
 * sleep.
 */
import { writeSync } from "node:fs";

import { isCallError, LifecycleLink, type LifecycleHandler } from "../src/link.ts";

function report(key: string, value: string | number | boolean): void {
  writeSync(1, `KELD_LL ${key}=${value}\n`);
}

function codeOf(err: unknown): string {
  if (isCallError(err)) return err.code;
  return err instanceof Error ? `untyped:${err.message}` : `untyped:${String(err)}`;
}

function reportError(key: string, err: unknown): void {
  report(`${key}-code`, codeOf(err));
  report(`${key}-callerror`, isCallError(err));
  const message = err instanceof Error ? err.message : String(err);
  report(`${key}-message`, message.replaceAll("\n", " "));
}

function requireLink(): string {
  const link = process.env.KELD_APP_LINK;
  if (link === undefined || link.length === 0) throw new Error("KELD_APP_LINK is unset");
  return link;
}

/** Resolves when the link ends without a local close; reports the error. */
function linkDeath(): { handler: (err: Error) => void; dead: Promise<Error> } {
  let resolve: (err: Error) => void = () => undefined;
  const dead = new Promise<Error>((r) => {
    resolve = r;
  });
  return {
    dead,
    handler: (err) => {
      report("dead", true);
      reportError("dead", err);
      resolve(err);
    },
  };
}

function handlers(overrides: Partial<LifecycleHandler> = {}): LifecycleHandler {
  return {
    onReady: () => report("ready", true),
    onLastWindowClosed: () => report("last-window-closed", true),
    onLinkDead: (err) => {
      report("dead", true);
      reportError("dead", err);
    },
    ...overrides,
  };
}

async function connect(overrides: Partial<LifecycleHandler> = {}): Promise<LifecycleLink> {
  const session = await LifecycleLink.connect(requireLink(), handlers(overrides));
  report("connected", true);
  return session;
}

async function settle(key: string, promise: Promise<unknown>): Promise<void> {
  try {
    await promise;
    report(`${key}-resolved`, true);
  } catch (err) {
    report(`${key}-resolved`, false);
    reportError(key, err);
  }
}

const SCENARIOS: Record<string, () => Promise<void>> = {
  // The host's HELLO reply decides `connect`; nothing else happens.
  async connect() {
    await settle("connect", LifecycleLink.connect(requireLink(), handlers()));
  },

  // Host Echo CALLs reach the application handler; the link stays up until
  // the host closes it.
  async "echo-call"() {
    let calls = 0;
    const death = linkDeath();
    await connect({
      onApplicationCall: async (channel, payload) => {
        calls += 1;
        report("echo-channel", channel);
        return Uint8Array.from([...payload, 0x7f]);
      },
      onLinkDead: death.handler,
    });
    await death.dead;
    report("echo-calls", calls);
  },

  // A slow handler holds no reader: the Worker answers Ping meanwhile. The
  // host's LastWindowClosed EVENT, sent after it read the Ping, releases it.
  async "slow-handler"() {
    let release: () => void = () => undefined;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const death = linkDeath();
    await connect({
      onLastWindowClosed: () => {
        report("last-window-closed", true);
        release();
      },
      onApplicationCall: async (_channel, payload) => {
        await gate;
        return payload;
      },
      onLinkDead: death.handler,
    });
    await death.dead;
  },

  // A handler may quit the app; the Quit closes the link on its REPLY.
  async "handler-quits"() {
    let session: LifecycleLink | undefined;
    let resolveFinished: () => void = () => undefined;
    const finished = new Promise<void>((resolve) => {
      resolveFinished = resolve;
    });
    session = await connect({
      onApplicationCall: async (_channel, payload) => {
        await settle("handler-quit", session!.quit());
        resolveFinished();
        return payload;
      },
    });
    await finished;
    report("handler-finished", true);
  },

  // Lifecycle EVENTs reach their handlers; an undecodable one ends the link.
  async events() {
    const death = linkDeath();
    await connect({ onLinkDead: death.handler });
    await death.dead;
  },

  // The role's Quit: the outcome of the host's answer, then local-close rules.
  async quit() {
    const session = await connect();
    await settle("quit", session.quit());
    await settle("again", session.quit());
  },

  async "quit-twice"() {
    const session = await connect();
    const first = session.quit();
    const second = session.quit();
    report("same-promise", first === second);
    await settle("first", first);
    await settle("second", second);
  },

  async "local-close"() {
    const session = await connect();
    session.close();
    session.close();
    report("closed", true);
    await settle("quit-after-close", session.quit());
  },

  // app.whenReady over the real link: the error that ended the link rejects
  // it, and stays the answer for a later whenReady (sticky, GH-528 T3).
  async "when-ready"() {
    const { app } = await import("../src/app.ts");
    let first: unknown;
    try {
      await app.whenReady();
      report("ready-resolved", true);
    } catch (err) {
      first = err;
      reportError("ready", err);
    }
    const again = await app.whenReady().then(
      () => undefined,
      (err: unknown) => err,
    );
    report("again-same", again === first);
  },

  // app.quit never parks (PANEL-P2, #419): with the link already up, a timer
  // set after app.quit() runs before the host answers the Quit; the host
  // waits for that report.
  async "quit-yields"() {
    const { app } = await import("../src/app.ts");
    await app.whenReady();
    const steps: string[] = [];
    const quit = app.quit().then(
      () => steps.push("quit-resolved"),
      (err: unknown) => steps.push(`quit:${codeOf(err)}`),
    );
    setTimeout(() => {
      steps.push("timer-ran");
      report("timer-ran", true);
    }, 0);
    await quit;
    report("steps", steps.join(","));
  },

  // A throwing onLinkDead is isolated: the process still reports and exits.
  async "throwing-dead"() {
    let resolveDead: () => void = () => undefined;
    const dead = new Promise<void>((resolve) => {
      resolveDead = resolve;
    });
    await connect({
      onLinkDead: () => {
        resolveDead();
        throw new Error("KEL72_ON_LINK_DEAD_THROW");
      },
    });
    await dead;
    report("after-throw", true);
  },
};

const name = process.argv[2] ?? "";
const scenario = SCENARIOS[name];
if (scenario === undefined) {
  writeSync(2, `unknown lifecycle-link scenario: ${name}\n`);
  process.exit(2);
}
let uncaught = 0;
process.on("uncaughtException", (err) => {
  uncaught += 1;
  report("uncaught", err instanceof Error ? err.message : String(err));
});
try {
  await scenario();
  // One more task: an isolated listener failure surfaces before `done`.
  await new Promise<void>((resolve) => setImmediate(resolve));
  report("uncaught-count", uncaught);
  report("done", true);
  process.exit(0);
} catch (err) {
  writeSync(2, `${err instanceof Error ? (err.stack ?? err.message) : String(err)}\n`);
  process.exit(1);
}
