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
let linkPromise: Promise<LifecycleLink> | undefined;
/** Per-connect identity; prevents a synchronous dead session from being recached. */
let linkSession: object | undefined;

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
  linkDead = undefined;
  const session = {};
  linkSession = session;
  const pending = LifecycleLink.connect(envLink, {
    onReady: onHostReady,
    onLastWindowClosed,
    onApplicationCall: dispatchApplicationCall,
    onLinkDead: (err: Error) => {
      if (linkSession !== session) return;
      try {
        failReadyWaiters(err);
      } finally {
        if (!hostReady && linkSession === session) {
          linkPromise = undefined;
        }
      }
    },
  });
  const tracked = pending.catch((err: unknown) => {
    if (linkSession === session) {
      linkPromise = undefined;
    }
    throw err;
  });
  if (linkSession === session && !linkDead) {
    linkPromise = tracked;
  }
  return tracked;
}
function sendQuit(): Promise<void> {
  const done = ensureLink().then((link) => link.quit());
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
