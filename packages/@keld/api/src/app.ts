/**
 * Backend-independent application lifecycle over the canonical app-link.
 *
 * This package owns the singleton lifecycle session. Compatibility layers
 * such as @keld/electron delegate to this object instead of opening another
 * socket or reader.
 */
import { dispatchApplicationCall } from "./channels.ts";
import { LifecycleLink } from "./link.ts";

export type Unsubscribe = () => void;

export interface AppLifecycle {
  whenReady(): Promise<void>;
  isReady(): boolean;
  onLastWindowClosed(listener: () => void): Unsubscribe;
  quit(): Promise<void>;
}

type ReadyWaiter = {
  resolve: () => void;
  reject: (err: Error) => void;
};

let hostReady = false;
let linkDead: Error | undefined;
let readyWaiters: ReadyWaiter[] = [];
/**
 * The role's one link attempt (GH-527 §4.2: a realm opens one link). Never
 * reset: a failed connect, or a link that died before Ready, stays the answer,
 * so every later `whenReady` or `quit` rethrows that same typed error instead
 * of retrying into `KELD-IPC-005`.
 */
let linkPromise: Promise<LifecycleLink> | undefined;

let nextListenerId = 1;
const lastWindowClosedListeners = new Map<number, () => void>();

function onHostReady(): void {
  if (hostReady) return;
  hostReady = true;
  linkDead = undefined;
  const waiters = readyWaiters;
  readyWaiters = [];
  for (const waiter of waiters) waiter.resolve();
}

function onLastWindowClosed(): void {
  const snapshot = [...lastWindowClosedListeners.values()];
  for (const listener of snapshot) {
    try {
      listener();
    } catch {
      // Host events share the app-link read loop; isolate user callbacks.
    }
  }
}
function failReadyWaiters(err: Error): void {
  linkDead = err;
  const waiters = readyWaiters;
  readyWaiters = [];
  for (const waiter of waiters) {
    try {
      waiter.reject(err);
    } catch {
      // One waiter must not prevent the session from being retired.
    }
  }
}

function ignoreIfUnawaited(promise: Promise<unknown>): void {
  void promise.catch(() => {});
}

function ensureLink(): Promise<LifecycleLink> {
  if (linkPromise) return linkPromise;
  const envLink = process.env.KELD_APP_LINK;
  if (!envLink) {
    return Promise.reject(
      new Error(
        "KELD-IPC-007: KELD_APP_LINK is unset. Run under a Keld host so the host mints <endpoint>#<64 hex chars>.",
      ),
    );
  }
  linkPromise = LifecycleLink.connect(envLink, {
    onReady: onHostReady,
    onLastWindowClosed,
    onApplicationCall: dispatchApplicationCall,
    onLinkDead: failReadyWaiters,
  }).catch((err: unknown) => {
    // The first connect failure is sticky: its typed error answers every
    // later `whenReady` and `quit`.
    if (err instanceof Error) linkDead ??= err;
    throw err;
  });
  ignoreIfUnawaited(linkPromise);
  return linkPromise;
}
/** The typed close a `whenReady` still waiting sees once a Quit closed the link. */
function closedByQuit(): Error {
  return Object.assign(
    new Error("KELD-IPC-022: the app quit before the host was ready; the role's link is closed"),
    { code: "KELD-IPC-022" },
  );
}

function sendQuit(): Promise<void> {
  const done = ensureLink().then((link) =>
    link.quit().finally(() => {
      // The Quit closed the link, which suppresses onLinkDead: a whenReady
      // still waiting can never see Ready, so it rejects with the close.
      if (!hostReady && linkDead === undefined) failReadyWaiters(closedByQuit());
    }),
  );
  ignoreIfUnawaited(done);
  return done;
}

export const app: AppLifecycle = {
  whenReady(): Promise<void> {
    if (hostReady) return Promise.resolve();
    const ready = ensureLink().then(() => {
      if (hostReady) return;
      if (linkDead) return Promise.reject(linkDead);
      return new Promise<void>((resolve, reject) => {
        if (hostReady) {
          resolve();
          return;
        }
        if (linkDead) {
          reject(linkDead);
          return;
        }
        readyWaiters.push({ resolve, reject });
      });
    });
    ignoreIfUnawaited(ready);
    return ready;
  },
  isReady(): boolean {
    return hostReady;
  },

  onLastWindowClosed(listener: () => void): Unsubscribe {
    const id = nextListenerId;
    nextListenerId += 1;
    lastWindowClosedListeners.set(id, listener);
    let active = true;
    return () => {
      if (!active) return;
      active = false;
      lastWindowClosedListeners.delete(id);
    };
  },

  quit(): Promise<void> {
    return sendQuit();
  },
};
