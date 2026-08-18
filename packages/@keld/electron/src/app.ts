/**
 * Electron `app` shim backed by host lifecycle kipc (KEL-72).
 *
 * Oracle: https://www.electronjs.org/docs/latest/api/app
 * - `app.whenReady()` resolves after the host `Ready` event, not at import.
 * - `app.quit()` is a kipc `Quit` Call; the host ending the session is the
 *   process-lifecycle oracle.
 * - `window-all-closed` is emitted only when the host sends `LastWindowClosed`.
 */

import { LifecycleLink } from "./link";

type AppListener = () => void;

const listeners = new Map<string, AppListener[]>();

function emit(event: string): void {
  const snapshot = listeners.get(event);
  if (!snapshot) return;
  for (const listener of snapshot.slice()) {
    try {
      listener();
    } catch {
      // Isolate each listener. Host Events arrive on the kipc read loop;
      // an uncaught throw would skip remaining listeners and abort the loop.
    }
  }
}

let hostReady = false;
let linkDead: Error | undefined;
let readyWaiters: Array<{ resolve: () => void; reject: (err: Error) => void }> = [];
let linkPromise: Promise<LifecycleLink> | undefined;

function onHostReady(): void {
  if (hostReady) return;
  hostReady = true;
  linkDead = undefined;
  // Drain whenReady waiters before user `ready` listeners so a throw in a
  // listener cannot leave whenReady() pending (even if emit isolation is
  // later weakened).
  const waiters = readyWaiters;
  readyWaiters = [];
  for (const waiter of waiters) waiter.resolve();
  emit("ready");
}

function failReadyWaiters(err: Error): void {
  linkDead = err;
  const waiters = readyWaiters;
  readyWaiters = [];
  for (const waiter of waiters) {
    try {
      waiter.reject(err);
    } catch {
      // Isolate each waiter. A throw must not skip linkPromise clear.
    }
  }
}

/**
 * Attach a handler so an unawaited rejection is not "unhandled", without
 * swallowing it for anyone who does await the same promise.
 */
function ignoreIfUnawaited(promise: Promise<unknown>): void {
  void promise.catch(() => {});
}

function ensureLink(): Promise<LifecycleLink> {
  if (linkPromise) return linkPromise;
  const envLink = process.env.KELD_APP_LINK;
  if (!envLink) {
    return Promise.reject(
      new Error(
        "KELD-IPC-007: KELD_APP_LINK is unset. Run under a Keld host (`keld dev` or a lifecycle test) so the host mints <endpoint>#<64 hex chars>.",
      ),
    );
  }
  linkDead = undefined;
  let tracked: Promise<LifecycleLink>;
  const pending = LifecycleLink.connect(envLink, {
    onReady: onHostReady,
    onLastWindowClosed: () => {
      emit("window-all-closed");
    },
    onLinkDead: (err: Error) => {
      if (linkPromise !== tracked) return;
      try {
        failReadyWaiters(err);
      } finally {
        // Drop the cached session only before Ready so a later whenReady()
        // retries. After Ready, Electron stays isReady(); keep the (dead)
        // link for quit().
        if (!hostReady && linkPromise === tracked) {
          linkPromise = undefined;
        }
      }
    },
  });
  tracked = pending.catch((err: unknown) => {
    if (linkPromise === tracked) {
      linkPromise = undefined;
    }
    throw err;
  });
  linkPromise = tracked;
  return tracked;
}

export const app = {
  /**
   * Resolves after the host sends the lifecycle `Ready` event.
   *
   * Must not be implemented as `Promise.resolve()` at import time: a fixture
   * that logs WAITING then awaits this, with the host still holding Ready,
   * must not print READY. That is the KEL-72 negative control.
   */
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

  /** True only after the host `Ready` event. */
  isReady(): boolean {
    return hostReady;
  },

  /**
   * Sends a kipc `Quit` Call. The host replies and ends the session.
   *
   * Quirk vs Electron oracle `app.quit(): void`
   * (https://www.electronjs.org/docs/latest/api/app#appquit): this returns
   * `Promise<void>` so callers can observe `KELD-IPC-*` when the transport
   * fails. Electron's `void` is process-lifetime and is not a thenable.
   * Scoreboard ▲ (`docs/engineering/compat-scoreboard.md`); not a
   * `keld.compat.ts` toggle. Keep the Promise; do not change the public
   * signature to `void` to paper over kipc.
   */
  quit(): Promise<void> {
    const done = ensureLink().then((link) => link.quit());
    ignoreIfUnawaited(done);
    return done;
  },

  on(event: string, listener: AppListener): void {
    const list = listeners.get(event) ?? [];
    list.push(listener);
    listeners.set(event, list);
  },
};
