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
let readyWaiters: Array<() => void> = [];
let linkPromise: Promise<LifecycleLink> | undefined;

function onHostReady(): void {
  if (hostReady) return;
  hostReady = true;
  // Drain whenReady waiters before user `ready` listeners so a throw in a
  // listener cannot leave whenReady() pending (even if emit isolation is
  // later weakened).
  const waiters = readyWaiters;
  readyWaiters = [];
  for (const waiter of waiters) waiter();
  emit("ready");
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
  const pending = LifecycleLink.connect(envLink, {
    onReady: onHostReady,
    onLastWindowClosed: () => {
      emit("window-all-closed");
    },
  });
  let tracked: Promise<LifecycleLink>;
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
    const ready = ensureLink().then(() => {
      if (hostReady) return;
      return new Promise<void>((resolve) => {
        readyWaiters.push(resolve);
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
   * `Promise<void>` so callers can await the kipc Quit reply. Electron's
   * `void` is process-lifetime and is not a thenable. Keep the Promise; do
   * not change the public signature to `void` to paper over kipc.
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
