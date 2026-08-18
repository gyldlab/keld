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
  for (const listener of listeners.get(event) ?? []) {
    listener();
  }
}

let hostReady = false;
let readyWaiters: Array<() => void> = [];
let linkPromise: Promise<LifecycleLink> | undefined;

function onHostReady(): void {
  if (hostReady) return;
  hostReady = true;
  emit("ready");
  const waiters = readyWaiters;
  readyWaiters = [];
  for (const waiter of waiters) waiter();
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
  linkPromise = LifecycleLink.connect(envLink, {
    onReady: onHostReady,
    onLastWindowClosed: () => {
      emit("window-all-closed");
    },
  });
  return linkPromise;
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
    return ensureLink().then(() => {
      if (hostReady) return;
      return new Promise<void>((resolve) => {
        readyWaiters.push(resolve);
      });
    });
  },

  /** True only after the host `Ready` event. */
  isReady(): boolean {
    return hostReady;
  },

  /**
   * Sends a kipc `Quit` Call. The host replies and ends the session.
   */
  async quit(): Promise<void> {
    const link = await ensureLink();
    await link.quit();
  },

  on(event: string, listener: AppListener): void {
    const list = listeners.get(event) ?? [];
    list.push(listener);
    listeners.set(event, list);
  },
};
